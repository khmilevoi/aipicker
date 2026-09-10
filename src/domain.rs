use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub provider: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    #[serde(default)]
    pub task_cost: Option<f64>,
    pub intelligence: Option<f64>,
    pub coding: Option<f64>,
    pub agentic: Option<f64>,
}

impl Model {
    pub fn score(&self, metric: Metric) -> Option<f64> {
        match metric {
            Metric::Intelligence => self.intelligence,
            Metric::Coding => self.coding,
            Metric::Agentic => self.agentic,
        }
    }

    pub fn price(&self, mode: PriceMode, input_share: f64) -> Option<f64> {
        match mode {
            PriceMode::Input => self.input_price,
            PriceMode::Output => self.output_price,
            PriceMode::Task => self.task_cost,
            PriceMode::Blended => {
                if !input_share.is_finite() || !(0.0..=1.0).contains(&input_share) {
                    return None;
                }
                if input_share == 1.0 {
                    return self.input_price;
                }
                if input_share == 0.0 {
                    return self.output_price;
                }
                Some(self.input_price? * input_share + self.output_price? * (1.0 - input_share))
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.slug.trim().is_empty()
            || self.provider.trim().is_empty()
        {
            return Err("У модели отсутствует идентификатор, название или поставщик".into());
        }
        if [self.input_price, self.output_price, self.task_cost]
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || v < 0.0)
            || [self.intelligence, self.coding, self.agentic]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite())
        {
            return Err("Источник вернул некорректную цену или оценку".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Metric {
    Intelligence,
    #[default]
    Coding,
    Agentic,
}
impl Metric {
    pub const ALL: [Self; 3] = [Self::Coding, Self::Intelligence, Self::Agentic];
    pub fn label(self) -> &'static str {
        match self {
            Self::Intelligence => "AA Intelligence",
            Self::Coding => "AA Coding",
            Self::Agentic => "AA Agentic",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceMode {
    Input,
    Output,
    Task,
    #[default]
    Blended,
}
impl PriceMode {
    pub const ALL: [Self; 4] = [Self::Blended, Self::Input, Self::Output, Self::Task];
    pub fn label(self) -> &'static str {
        match self {
            Self::Input => "Вход",
            Self::Output => "Выход",
            Self::Blended => "Смешанная",
            Self::Task => "Задача AA",
        }
    }
    pub fn unit(self) -> &'static str {
        if self == Self::Task {
            "USD / задачу AA"
        } else {
            "USD / 1 млн токенов"
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortBy {
    #[default]
    Price,
    Quality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub selected: Option<String>,
    pub disabled: BTreeSet<String>,
    pub openai: bool,
    pub anthropic: bool,
    pub sort: SortBy,
    pub metric: Metric,
    pub price_mode: PriceMode,
    pub input_share: f64,
    pub logarithmic: bool,
    pub next_request_at: u64,
    pub collapse_reasoning: bool,
    pub reasoning_tolerance: f64,
    pub reasoning_savings: f64,
    pub quality_weight: f64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            selected: None,
            disabled: BTreeSet::new(),
            openai: true,
            anthropic: true,
            sort: SortBy::Price,
            metric: Metric::Coding,
            price_mode: PriceMode::Blended,
            input_share: 0.75,
            logarithmic: true,
            next_request_at: 0,
            collapse_reasoning: true,
            reasoning_tolerance: 2.0,
            reasoning_savings: 0.20,
            quality_weight: 0.65,
        }
    }
}

fn optional_cmp(a: Option<f64>, b: Option<f64>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.total_cmp(&a)
            } else {
                a.total_cmp(&b)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub fn ordered_models<'a>(models: &'a [Model], prefs: &Preferences) -> Vec<&'a Model> {
    let mut filtered: Vec<_> = models
        .iter()
        .filter(|m| {
            ((prefs.openai && m.provider == "openai")
                || (prefs.anthropic && m.provider == "anthropic"))
                && !prefs.disabled.contains(&m.id)
        })
        .collect();
    filtered.sort_by(|a, b| {
        let order = match prefs.sort {
            SortBy::Price => optional_cmp(
                a.price(prefs.price_mode, prefs.input_share),
                b.price(prefs.price_mode, prefs.input_share),
                false,
            ),
            SortBy::Quality => optional_cmp(a.score(prefs.metric), b.score(prefs.metric), true),
        };
        order
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    filtered
}

#[derive(Debug, Clone, Copy)]
pub struct Balance {
    pub score: Option<f64>,
    pub quality: Option<f64>,
    pub affordability: Option<f64>,
    pub covered: usize,
}

fn percentile(value: f64, values: impl Iterator<Item = f64>) -> f64 {
    let values: Vec<_> = values.collect();
    if values.len() <= 1 {
        return 0.5;
    }
    let less = values.iter().filter(|&&v| v < value).count() as f64;
    let equal = values.iter().filter(|&&v| v == value).count() as f64;
    ((less + (equal - 1.0) * 0.5) / (values.len() - 1) as f64).clamp(0.0, 1.0)
}

/// Relative ranks are computed against the full downloaded pool, before filters.
/// Missing measurements are omitted, never imputed as zero; coverage is exposed.
pub fn balance(model: &Model, pool: &[Model], quality_weight: f64) -> Balance {
    let mut qualities = Vec::new();
    let mut costs = Vec::new();
    for metric in Metric::ALL {
        if let Some(value) = model.score(metric) {
            qualities.push(percentile(
                value,
                pool.iter().filter_map(|m| m.score(metric)),
            ));
        }
    }
    for mode in [PriceMode::Input, PriceMode::Output, PriceMode::Task] {
        if let Some(value) = model.price(mode, 0.75) {
            costs.push(1.0 - percentile(value, pool.iter().filter_map(|m| m.price(mode, 0.75))));
        }
    }
    let average = |values: &[f64]| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let quality = average(&qualities);
    let affordability = average(&costs);
    let weight = quality_weight.clamp(0.0, 1.0);
    Balance {
        score: quality
            .zip(affordability)
            .map(|(q, c)| (100.0 * (weight * q + (1.0 - weight) * c) * 100.0).round() / 100.0),
        quality,
        affordability,
        covered: qualities.len() + costs.len(),
    }
}

/// The compact rail increases in quality, with task cost breaking equal-quality ties.
pub fn balanced_models<'a>(models: &'a [Model], prefs: &Preferences) -> Vec<&'a Model> {
    let mut filtered = ordered_models(models, prefs);
    let scores: std::collections::BTreeMap<_, _> = models
        .iter()
        .map(|m| (&m.id, balance(m, models, prefs.quality_weight).quality))
        .collect();
    filtered.sort_by(|a, b| {
        match (scores[&a.id], scores[&b.id]) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| optional_cmp(a.task_cost, b.task_cost, false))
        .then_with(|| a.id.cmp(&b.id))
    });
    filtered
}

pub struct CollapsedModels {
    pub models: Vec<Model>,
    pub replacements: std::collections::BTreeMap<String, String>,
}

fn reasoning_family(model: &Model) -> Option<(String, String)> {
    let (base, suffix) = model.name.trim().rsplit_once(" (")?;
    let level = suffix.strip_suffix(')')?.to_ascii_lowercase();
    let level = level.trim().strip_suffix(" effort").unwrap_or(level.trim());
    if !matches!(
        level,
        "minimal"
            | "low"
            | "medium"
            | "high"
            | "xhigh"
            | "extra high"
            | "max"
            | "none"
            | "off"
            | "adaptive"
            | "thinking"
            | "non-reasoning"
            | "reasoning"
    ) {
        return None;
    }
    Some((model.provider.clone(), base.to_lowercase()))
}

/// Compare directly with retained representatives, never chain quality losses.
pub fn collapse_reasoning(models: &[Model], prefs: &Preferences) -> CollapsedModels {
    let mut replacements = std::collections::BTreeMap::new();
    if prefs.collapse_reasoning {
        let mut candidates: Vec<_> = models.iter().collect();
        candidates.sort_by(|a, b| {
            optional_cmp(a.task_cost, b.task_cost, false).then_with(|| a.id.cmp(&b.id))
        });
        let mut retained: Vec<&Model> = Vec::new();
        let tolerance = prefs.reasoning_tolerance.clamp(0.0, 20.0);
        let savings = prefs.reasoning_savings.clamp(0.01, 1.0);
        for candidate in candidates {
            let replacement = if let (Some(family), Some(_score), Some(cost)) = (
                reasoning_family(candidate),
                candidate
                    .score(Metric::Intelligence)
                    .or(candidate.coding)
                    .or(candidate.agentic),
                candidate.task_cost,
            ) {
                retained
                    .iter()
                    .find(|other| {
                        reasoning_family(other).as_ref() == Some(&family)
                            && Metric::ALL.iter().all(|&metric| {
                                match (candidate.score(metric), other.score(metric)) {
                                    (Some(a), Some(b)) => b >= a - tolerance,
                                    (None, _) => true,
                                    (Some(_), None) => false,
                                }
                            })
                            && other.task_cost.is_some_and(|c| {
                                cost > 0.0 && c < cost && c <= cost * (1.0 - savings)
                            })
                    })
                    .copied()
            } else {
                None
            };
            if let Some(other) = replacement {
                replacements.insert(candidate.id.clone(), other.id.clone());
            } else {
                retained.push(candidate);
            }
        }
    }
    CollapsedModels {
        models: models
            .iter()
            .filter(|m| !replacements.contains_key(&m.id))
            .cloned()
            .collect(),
        replacements,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema: u32,
    pub fetched_at: u64,
    pub index_version: f64,
    pub models: Vec<Model>,
    #[serde(default)]
    pub demo: bool,
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || !self.index_version.is_finite()
            || self.index_version <= 0.0
            || self.models.is_empty()
        {
            return Err("Пустой или несовместимый набор бенчмарков".into());
        }
        let mut ids = BTreeSet::new();
        for m in &self.models {
            m.validate()?;
            if !ids.insert(&m.id) {
                return Err("Источник вернул повторяющиеся ID моделей".into());
            }
        }
        Ok(())
    }

    pub fn demo() -> Self {
        let models = (0..8)
            .map(|i| {
                let provider = if i % 2 == 0 { "openai" } else { "anthropic" };
                let family = if i % 2 == 0 { "Codex" } else { "Claude" };
                let level = ["low", "medium", "high", "xhigh"][i / 2];
                Model {
                    id: format!("demo-{i}"),
                    slug: format!("demo-{i}"),
                    name: format!("{family} Пример ({level})"),
                    provider: provider.into(),
                    input_price: Some(0.5 + i as f64 * 0.8),
                    output_price: Some(2.0 + i as f64 * 3.0),
                    task_cost: Some(0.04 + i as f64 * 0.08),
                    intelligence: Some(50.0 + i as f64 * 0.6),
                    coding: if i == 7 {
                        None
                    } else {
                        Some(55.0 + i as f64 * 0.8)
                    },
                    agentic: Some(45.0 + i as f64 * 0.7),
                }
            })
            .collect();
        Self {
            schema: 1,
            fetched_at: now(),
            index_version: 4.3,
            models,
            demo: true,
        }
    }
}
