use crate::domain::{Model, Snapshot, now};
use serde::Deserialize;
use std::{collections::BTreeSet, io::Read, time::Duration};

pub const ENDPOINT: &str = "https://artificialanalysis.ai/api/v2/language/models/free";

#[derive(Debug, Clone)]
pub struct FetchError {
    pub message: String,
    pub retry_after: Option<u64>,
}
impl From<String> for FetchError {
    fn from(message: String) -> Self {
        Self {
            message,
            retry_after: None,
        }
    }
}
impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for FetchError {}

#[derive(Deserialize)]
struct WirePage {
    intelligence_index_version: f64,
    pagination: Pagination,
    data: Vec<WireModel>,
}
#[derive(Deserialize)]
struct Pagination {
    page: u32,
    total_pages: u32,
    has_more: bool,
}
#[derive(Deserialize)]
struct WireModel {
    id: String,
    name: String,
    slug: String,
    model_creator: Creator,
    evaluations: Evaluations,
    pricing: Pricing,
    artificial_analysis_intelligence_index_cost: Option<WireCost>,
}
#[derive(Deserialize)]
struct WireCost {
    cost_per_task: Option<TaskCost>,
}
#[derive(Deserialize)]
struct TaskCost {
    total_cost: Option<f64>,
}
#[derive(Deserialize)]
struct Creator {
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    name: Option<String>,
}
#[derive(Deserialize)]
struct Evaluations {
    artificial_analysis_intelligence_index: Option<f64>,
    artificial_analysis_coding_index: Option<f64>,
    artificial_analysis_agentic_index: Option<f64>,
}
#[derive(Deserialize)]
struct Pricing {
    price_1m_input_tokens: Option<f64>,
    price_1m_output_tokens: Option<f64>,
}

pub struct Page {
    pub version: f64,
    pub page: u32,
    pub total_pages: u32,
    pub has_more: bool,
    pub models: Vec<Model>,
}

pub fn parse_page(body: &str) -> Result<Page, FetchError> {
    let wire: WirePage = serde_json::from_str(body).map_err(|_| {
        FetchError::from("Формат ответа API изменился или повреждён. Кеш сохранён.".to_string())
    })?;
    let p = wire.pagination;
    if !wire.intelligence_index_version.is_finite()
        || wire.intelligence_index_version <= 0.0
        || p.page == 0
        || p.total_pages == 0
        || p.page > p.total_pages
        || p.has_more != (p.page < p.total_pages)
    {
        return Err("Некорректная версия индекса или пагинация API"
            .to_string()
            .into());
    }
    let mut models = Vec::new();
    for m in wire.data {
        let Some(provider) = m.model_creator.known_provider() else {
            continue;
        };
        let model = Model {
            id: m.id,
            name: m.name,
            slug: m.slug,
            provider: provider.to_string(),
            input_price: m.pricing.price_1m_input_tokens,
            output_price: m.pricing.price_1m_output_tokens,
            task_cost: m
                .artificial_analysis_intelligence_index_cost
                .and_then(|c| c.cost_per_task)
                .and_then(|c| c.total_cost),
            intelligence: m.evaluations.artificial_analysis_intelligence_index,
            coding: m.evaluations.artificial_analysis_coding_index,
            agentic: m.evaluations.artificial_analysis_agentic_index,
        };
        model.validate()?;
        models.push(model);
    }
    Ok(Page {
        version: wire.intelligence_index_version,
        page: p.page,
        total_pages: p.total_pages,
        has_more: p.has_more,
        models,
    })
}

impl Creator {
    fn known_provider(&self) -> Option<&'static str> {
        let slug = self
            .slug
            .as_deref()
            .map(str::trim)
            .filter(|slug| !slug.is_empty());
        if let Some(slug) = slug {
            return known_provider_name(slug);
        }
        self.name.as_deref().and_then(known_provider_name)
    }
}

fn known_provider_name(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" => Some("openai"),
        "anthropic" => Some("anthropic"),
        _ => None,
    }
}

pub fn fetch_snapshot(endpoint: &str, key: &str) -> Result<Snapshot, FetchError> {
    if key.trim().is_empty() {
        return Err("Добавьте бесплатный ключ Artificial Analysis в настройках"
            .to_string()
            .into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("AI-Picker/0.1 (personal benchmark widget)")
        .build()
        .map_err(|_| FetchError::from("Не удалось создать HTTP-клиент".to_string()))?;
    let mut models = Vec::new();
    let mut version = None;
    let mut total = None;
    let mut ids = BTreeSet::new();
    for page_number in 1..=50 {
        let response = client
            .get(endpoint)
            .header("x-api-key", key.trim())
            .query(&[("page", page_number)])
            .send()
            .map_err(|_| {
                FetchError::from(
                    "Не удалось связаться с Artificial Analysis. Проверьте интернет; кеш сохранён."
                        .to_string(),
                )
            })?;
        if !response.status().is_success() {
            let code = response.status().as_u16();
            let retry_after = if code == 429 {
                Some(
                    response
                        .headers()
                        .get("Retry-After")
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .or_else(|| {
                            response
                                .headers()
                                .get("X-RateLimit-Reset")
                                .and_then(|h| h.to_str().ok())
                                .and_then(|s| s.parse::<u64>().ok())
                                .map(|reset| reset.saturating_sub(now()))
                        })
                        .unwrap_or(3600)
                        .max(1),
                )
            } else {
                None
            };
            let message = match code {
                401 => "Ключ API не принят. Проверьте его в настройках.".into(),
                403 => "API запретил доступ. Проверьте права ключа Artificial Analysis.".into(),
                429 => "Лимит API исчерпан. Повторное обновление доступно после указанной паузы."
                    .into(),
                _ => format!("Artificial Analysis вернул HTTP {code}. Кеш сохранён."),
            };
            return Err(FetchError {
                message,
                retry_after,
            });
        }
        let mut bytes = Vec::new();
        response
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                FetchError::from("Ответ API не удалось прочитать полностью".to_string())
            })?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("Ответ API слишком большой".to_string().into());
        }
        let body = std::str::from_utf8(&bytes)
            .map_err(|_| FetchError::from("Некорректная кодировка ответа API".to_string()))?;
        let page = parse_page(body)?;
        if page.page != page_number
            || version.is_some_and(|v| v != page.version)
            || total.is_some_and(|n| n != page.total_pages)
        {
            return Err(
                "Данные изменились во время загрузки. Повторите обновление; кеш сохранён."
                    .to_string()
                    .into(),
            );
        }
        version = Some(page.version);
        total = Some(page.total_pages);
        for model in page.models {
            if !ids.insert(model.id.clone()) {
                return Err("API вернул дубликат модели на разных страницах"
                    .to_string()
                    .into());
            }
            if matches!(model.provider.as_str(), "openai" | "anthropic") {
                models.push(model);
            }
        }
        if !page.has_more {
            let snapshot = Snapshot {
                schema: 1,
                fetched_at: now(),
                index_version: page.version,
                models,
                demo: false,
            };
            snapshot.validate()?;
            return Ok(snapshot);
        }
    }
    Err("API превысил безопасный предел в 50 страниц. Кеш сохранён."
        .to_string()
        .into())
}
