use aipicker::domain::{Metric, Model, Preferences, collapse_reasoning};

fn model(id: &str, name: &str, score: Option<f64>, cost: Option<f64>) -> Model {
    Model {
        id: id.into(),
        name: name.into(),
        slug: id.into(),
        provider: "openai".into(),
        input_price: Some(2.0),
        output_price: Some(10.0),
        task_cost: cost,
        intelligence: score,
        coding: score,
        agentic: None,
    }
}

#[test]
fn collapses_similar_reasoning_using_actual_task_cost_despite_equal_token_rates() {
    let models = vec![
        model("low", "Codex Alpha (low)", Some(60.0), Some(0.1)),
        model("high", "Codex Alpha (high)", Some(61.0), Some(0.8)),
    ];
    let p = Preferences::default();
    assert!(p.collapse_reasoning);
    let result = collapse_reasoning(&models, &p);
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["low"]
    );
    assert_eq!(
        result.replacements.get("high").map(String::as_str),
        Some("low")
    );
}

#[test]
fn keeps_different_versions_unknown_scores_and_significant_quality_gains() {
    let models = vec![
        model("low", "Codex Alpha (low)", Some(60.0), Some(0.1)),
        model("high", "Codex Alpha (high)", Some(66.0), Some(0.8)),
        model("other", "Codex Beta (high)", Some(61.0), Some(0.8)),
        model("unknown", "Codex Alpha (xhigh)", None, Some(1.0)),
    ];
    assert_eq!(
        collapse_reasoning(&models, &Preferences::default())
            .models
            .len(),
        4
    );
}

#[test]
fn does_not_chain_small_losses_into_one_large_quality_loss() {
    let models = vec![
        model("low", "Codex Alpha (low)", Some(46.0), Some(0.1)),
        model("mid", "Codex Alpha (medium)", Some(48.0), Some(0.4)),
        model("high", "Codex Alpha (high)", Some(50.0), Some(0.8)),
    ];
    let result = collapse_reasoning(&models, &Preferences::default());
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["low", "high"]
    );
}

#[test]
fn toggle_and_thresholds_control_collapse_and_missing_task_cost_is_not_guessed() {
    let models = vec![
        model("low", "Codex Alpha (low)", Some(60.0), Some(0.1)),
        model("high", "Codex Alpha (high)", Some(61.0), Some(0.8)),
    ];
    let mut p = Preferences {
        collapse_reasoning: false,
        ..Default::default()
    };
    assert_eq!(collapse_reasoning(&models, &p).models.len(), 2);
    p.collapse_reasoning = true;
    p.reasoning_tolerance = 0.5;
    assert_eq!(collapse_reasoning(&models, &p).models.len(), 2);
    p.reasoning_tolerance = 2.0;
    p.metric = Metric::Agentic;
    assert_eq!(collapse_reasoning(&models, &p).models.len(), 1);
    p.metric = Metric::Coding;
    let missing = vec![
        model("low", "Codex Alpha (low)", Some(60.0), None),
        model("high", "Codex Alpha (high)", Some(61.0), Some(0.8)),
    ];
    assert_eq!(collapse_reasoning(&missing, &p).models.len(), 2);
}
