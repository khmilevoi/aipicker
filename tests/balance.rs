use aipicker::domain::{Model, Preferences, balance, balanced_models};

fn model(id: &str, score: f64, cost: f64) -> Model {
    Model {
        id: id.into(),
        name: id.into(),
        slug: id.into(),
        provider: "openai".into(),
        input_price: Some(cost),
        output_price: Some(cost * 4.0),
        task_cost: Some(cost / 10.0),
        intelligence: Some(score),
        coding: Some(score),
        agentic: Some(score),
    }
}

#[test]
fn balance_aggregates_all_six_metrics_with_explicit_quality_weight() {
    let models = vec![
        model("cheap", 20.0, 1.0),
        model("middle", 50.0, 2.0),
        model("strong", 80.0, 4.0),
    ];
    assert_eq!(balance(&models[0], &models, 0.65).score, Some(35.0));
    assert_eq!(balance(&models[1], &models, 0.65).score, Some(50.0));
    assert_eq!(balance(&models[2], &models, 0.65).score, Some(65.0));
    assert_eq!(balance(&models[1], &models, 0.65).covered, 6);
}

#[test]
fn disabling_models_does_not_change_the_remaining_models_scores() {
    let models = vec![
        model("cheap", 20.0, 1.0),
        model("middle", 50.0, 2.0),
        model("strong", 80.0, 4.0),
    ];
    let mut prefs = Preferences::default();
    prefs.disabled.insert("cheap".into());
    let sorted = balanced_models(&models, &prefs);
    assert_eq!(
        sorted.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["middle", "strong"]
    );
    assert_eq!(
        balance(sorted[0], &models, prefs.quality_weight).score,
        Some(50.0)
    );
}

#[test]
fn every_metric_can_change_balance_and_missing_metrics_are_reported() {
    let baseline = vec![
        model("a", 20.0, 1.0),
        model("b", 50.0, 2.0),
        model("c", 80.0, 4.0),
    ];
    for field in 0..6 {
        let mut models = baseline.clone();
        match field {
            0 => models[1].intelligence = Some(90.0),
            1 => models[1].coding = Some(90.0),
            2 => models[1].agentic = Some(90.0),
            3 => models[1].input_price = Some(0.1),
            4 => models[1].output_price = Some(0.1),
            _ => models[1].task_cost = Some(0.01),
        }
        assert!(balance(&models[1], &models, 0.65).score.unwrap() > 50.0);
    }
    let mut models = baseline;
    models[1].agentic = None;
    assert_eq!(balance(&models[1], &models, 0.65).covered, 5);
    models[1].intelligence = None;
    models[1].coding = None;
    assert_eq!(balance(&models[1], &models, 0.65).score, None);
}
