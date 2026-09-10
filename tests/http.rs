use aipicker::source::fetch_snapshot;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

fn serve(responses: Vec<(u16, String, String)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/models", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        for (i, (status, headers, body)) in responses.into_iter().enumerate() {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|s| s == b"\r\n\r\n") {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
            }
            let request = String::from_utf8(request).unwrap().to_lowercase();
            assert!(request.contains("x-api-key: test-only-key"));
            assert!(request.contains(&format!("page={}", i + 1)));
            write!(socket, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len()).unwrap();
        }
    });
    (url, handle)
}

fn page(n: u32, version: f64) -> String {
    json!({"tier":"free", "intelligence_index_version":version,
        "pagination":{"page":n,"page_size":1,"total_pages":2,"has_more":n < 2},
        "data":[{"id":format!("model-{n}"),"name":format!("Model {n}"),"slug":format!("model-{n}"),
            "model_creator":{"slug":"openai"},"evaluations":{"artificial_analysis_coding_index":42},
            "pricing":{"price_1m_input_tokens":1,"price_1m_output_tokens":4}}]})
    .to_string()
}

#[test]
fn download_joins_all_pages() {
    let (url, handle) = serve(vec![
        (200, String::new(), page(1, 4.3)),
        (200, String::new(), page(2, 4.3)),
    ]);
    let result = fetch_snapshot(&url, "test-only-key").unwrap();
    handle.join().unwrap();
    assert_eq!(result.models.len(), 2);
    assert_eq!(result.models[1].id, "model-2");
    assert_eq!(result.index_version, 4.3);
}

#[test]
fn rejects_mixed_index_versions_instead_of_returning_partial_data() {
    let (url, handle) = serve(vec![
        (200, String::new(), page(1, 4.3)),
        (200, String::new(), page(2, 5.0)),
    ]);
    let result = fetch_snapshot(&url, "test-only-key");
    handle.join().unwrap();
    assert!(result.is_err());
}

#[test]
fn rate_limit_preserves_retry_after_and_auth_failure_is_safe() {
    for (status, headers, wait) in [(429, "Retry-After: 120\r\n", Some(120)), (401, "", None)] {
        let (url, handle) = serve(vec![(
            status,
            headers.into(),
            "private upstream response".into(),
        )]);
        let error = fetch_snapshot(&url, "test-only-key").unwrap_err();
        handle.join().unwrap();
        assert_eq!(error.retry_after, wait);
        assert!(!error.message.contains("test-only-key"));
        assert!(!error.message.contains("private upstream"));
    }
}

#[test]
fn rejects_malformed_success_response() {
    let (url, handle) = serve(vec![(
        200,
        String::new(),
        "<html>maintenance</html>".into(),
    )]);
    assert!(fetch_snapshot(&url, "test-only-key").is_err());
    handle.join().unwrap();
}
