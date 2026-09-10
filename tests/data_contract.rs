use aipicker::domain::{Metric, Model, Preferences, PriceMode, SortBy, ordered_models};
use aipicker::source::parse_page;
use serde_json::json;

fn model(id: &str, provider: &str, score: Option<f64>) -> Model {
    Model {
        id: id.into(),
        name: id.into(),
        slug: id.into(),
        provider: provider.into(),
        input_price: Some(2.0),
        output_price: Some(10.0),
        task_cost: None,
        intelligence: score,
        coding: None,
        agentic: None,
    }
}

#[test]
fn blend_uses_input_share_and_preserves_missing_prices() {
    let mut m = model("a", "openai", Some(60.0));
    assert_eq!(m.price(PriceMode::Blended, 0.75), Some(4.0));
    m.output_price = None;
    assert_eq!(m.price(PriceMode::Blended, 0.75), None);
    assert_eq!(m.price(PriceMode::Input, 0.75), Some(2.0));
    assert_eq!(m.score(Metric::Coding), None);
}

#[test]
fn sort_puts_missing_last_and_filters_by_stable_id() {
    let models = vec![
        model("missing", "openai", None),
        model("low", "openai", Some(20.0)),
        model("high", "anthropic", Some(80.0)),
        model("other", "google", Some(90.0)),
    ];
    let mut p = Preferences {
        sort: SortBy::Quality,
        metric: Metric::Intelligence,
        ..Default::default()
    };
    let ids = |p: &Preferences| {
        ordered_models(&models, p)
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&p), ["high", "low", "missing"]);
    p.disabled.insert("high".into());
    assert_eq!(ids(&p), ["low", "missing"]);
    p.openai = false;
    assert!(ids(&p).is_empty());
}

#[test]
fn current_free_schema_keeps_null_separate_from_zero() {
    let body = json!({"tier":"free", "intelligence_index_version":4.3,
        "pagination":{"page":1,"page_size":200,"total_pages":1,"has_more":false},
        "data":[{"id":"abc","name":"Model (high)","slug":"model-high",
            "model_creator":{"id":"creator","name":"Anthropic","slug":"anthropic"},
            "evaluations":{"artificial_analysis_intelligence_index":0,"artificial_analysis_coding_index":null},
            "pricing":{"price_1m_input_tokens":2,"price_1m_output_tokens":10},"performance":{},
            "artificial_analysis_intelligence_index_cost":{"cost_per_task":{"total_cost":0.1678}}}]});
    let page = parse_page(&body.to_string()).unwrap();
    assert_eq!(page.models[0].intelligence, Some(0.0));
    assert_eq!(page.models[0].coding, None);
    assert_eq!(page.models[0].output_price, Some(10.0));
    assert_eq!(page.models[0].task_cost, Some(0.1678));
    assert_eq!(page.version, 4.3);
}

#[test]
fn schema_drift_and_negative_prices_are_rejected() {
    assert!(parse_page(r#"{"data":[]}"#).is_err());
    let body = json!({"intelligence_index_version":4.3,
        "pagination":{"page":1,"page_size":200,"total_pages":1,"has_more":false},
        "data":[{"id":"abc","name":"x","slug":"x","model_creator":{"slug":"openai"},
        "evaluations":{},"pricing":{"price_1m_input_tokens":-1}}]});
    assert!(parse_page(&body.to_string()).is_err());
}
