use aipicker::domain::{Model, Preferences, balance, balanced_models, collapse_reasoning};

#[test]
fn slider_quality_order_is_independent_of_price_weight() {
    let models = vec![model("strong", 80.0, 1.0), model("weak", 20.0, 8.0)];
    for quality_weight in [0.0, 0.65, 1.0] {
        let prefs = Preferences {
            quality_weight,
            ..Default::default()
        };
        assert_eq!(
            balanced_models(&models, &prefs)
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["weak", "strong"]
        );
    }
    let models = vec![model("strong", 80.0, 8.0), model("weak", 20.0, 1.0)];
    let prefs = Preferences {
        quality_weight: 0.0,
        ..Default::default()
    };
    assert_eq!(
        balanced_models(&models, &prefs)
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["weak", "strong"]
    );
}

#[test]
fn slider_equal_quality_uses_task_cost_before_id_or_token_prices() {
    let mut models = vec![model("a-expensive", 50.0, 1.0), model("z-cheap", 50.0, 8.0)];
    models[0].task_cost = Some(2.0);
    models[1].task_cost = Some(0.1);
    let prefs = Preferences {
        quality_weight: 1.0,
        ..Default::default()
    };
    assert_eq!(
        balanced_models(&models, &prefs)
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["z-cheap", "a-expensive"]
    );
}

#[test]
fn slider_collapse_preserves_quality_order() {
    let mut models = vec![
        model("low", 40.0, 1.0),
        model("high", 41.0, 8.0),
        model("strong", 80.0, 12.0),
    ];
    models[0].name = "Alpha (low)".into();
    models[1].name = "Alpha (high)".into();
    let prefs = Preferences {
        quality_weight: 0.0,
        ..Default::default()
    };
    let ordered: Vec<_> = balanced_models(&models, &prefs)
        .into_iter()
        .cloned()
        .collect();
    let collapsed = collapse_reasoning(&ordered, &prefs);
    assert_eq!(
        collapsed
            .models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        ["low", "strong"]
    );
    assert_eq!(
        collapsed.replacements.get("high").map(String::as_str),
        Some("low")
    );
}

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
