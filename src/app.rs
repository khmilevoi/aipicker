use crate::{
    charts::{self, ACCENT, MUTED},
    theme::{self, WARNING},
    tray::{self, Tray},
};
use aipicker::{
    domain::{
        Metric, Model, Preferences, PriceMode, Snapshot, SortBy, balance, balanced_models,
        collapse_reasoning, now, ordered_models,
    },
    source::{self, FetchError},
    storage::Store,
};
use eframe::egui::{self, RichText, Vec2, ViewportCommand, vec2};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
    time::Duration,
};

pub const COMPACT: Vec2 = vec2(420.0, 148.0);
const FILTER: Vec2 = vec2(420.0, 590.0);
const EXPANDED: Vec2 = vec2(1000.0, 720.0);

fn version_name(name: &str) -> &str {
    let Some((base, suffix)) = name.trim().rsplit_once(" (") else {
        return name;
    };
    let Some(level) = suffix.strip_suffix(')') else {
        return name;
    };
    let mut has_reasoning = false;
    // Strip only known variant metadata; dates and unknown qualifiers identify releases.
    for part in level.split(',') {
        let part = part.trim().to_ascii_lowercase();
        if part == "default fallback" {
            continue;
        }
        let part = part.strip_suffix(" effort").unwrap_or(&part);
        if !matches!(
            part,
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
                | "adaptive reasoning"
                | "thinking"
                | "non-reasoning"
                | "reasoning"
        ) {
            return name;
        }
        has_reasoning = true;
    }
    if has_reasoning { base } else { name }
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "openai" => "OpenAI / Codex",
        "anthropic" => "Anthropic / Claude",
        other => other,
    }
}

// This is only a UI parent label. Exact releases and selection IDs stay intact.
fn base_model_name(name: &str) -> &str {
    let mut base = version_name(name);
    while let Some((prefix, suffix)) = base.rsplit_once(" (") {
        if prefix.is_empty()
            || !suffix.ends_with(')')
            || suffix[..suffix.len() - 1].contains(['(', ')'])
        {
            break;
        }
        base = prefix;
    }
    base
}

fn matching_models<'a>(models: &'a [Model], query: &str) -> Vec<&'a Model> {
    let query = query.trim().to_lowercase();
    models
        .iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&query)
                || provider_label(&m.provider).to_lowercase().contains(&query)
                || model_family(&m.name).to_lowercase().contains(&query)
        })
        .collect()
}

fn provider_enabled(prefs: &Preferences, provider: &str) -> bool {
    match provider {
        "openai" => prefs.openai,
        "anthropic" => prefs.anthropic,
        _ => false,
    }
}

fn selected_count(models: &[&Model], prefs: &Preferences) -> usize {
    models
        .iter()
        .filter(|m| provider_enabled(prefs, &m.provider) && !prefs.disabled.contains(&m.id))
        .count()
}

fn select_models(pool: &[Model], models: &[&Model], prefs: &mut Preferences, selected: bool) {
    for model in models {
        if selected {
            // Preserve effective exclusions when enabling a previously disabled provider.
            if !provider_enabled(prefs, &model.provider) {
                prefs.disabled.extend(
                    pool.iter()
                        .filter(|m| m.provider == model.provider)
                        .map(|m| m.id.clone()),
                );
                match model.provider.as_str() {
                    "openai" => prefs.openai = true,
                    "anthropic" => prefs.anthropic = true,
                    _ => continue,
                }
            }
            prefs.disabled.remove(&model.id);
        } else {
            prefs.disabled.insert(model.id.clone());
        }
    }
}

fn selection_checkbox(
    ui: &mut egui::Ui,
    label: &str,
    models: &[&Model],
    pool: &[Model],
    prefs: &mut Preferences,
) {
    let count = selected_count(models, prefs);
    let mut selected = count == models.len();
    if ui
        .add(
            egui::Checkbox::new(&mut selected, label)
                .indeterminate(count > 0 && count < models.len()),
        )
        .changed()
    {
        select_models(pool, models, prefs, selected);
    }
}

fn model_family(name: &str) -> &str {
    let name = base_model_name(name);
    let words: Vec<_> = name.split(|c: char| !c.is_alphanumeric()).collect();
    if words.iter().any(|w| w.eq_ignore_ascii_case("codex")) {
        return "Codex";
    }
    if name.starts_with("Claude ") {
        for (word, family) in [
            ("Sonnet", "Claude Sonnet"),
            ("Opus", "Claude Opus"),
            ("Haiku", "Claude Haiku"),
            ("Fable", "Claude Fable"),
        ] {
            if words.iter().any(|w| w.eq_ignore_ascii_case(word)) {
                return family;
            }
        }
    }
    if name.starts_with("gpt-oss-") {
        return "gpt-oss";
    }
    if name.starts_with("GPT-") || name.starts_with("GPT ") {
        return "GPT";
    }
    if name.as_bytes().first() == Some(&b'o')
        && name.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
    {
        return "o-series";
    }
    base_model_name(name)
}

fn filter_group(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    group: &[&Model],
    pool: &[Model],
    prefs: &mut Preferences,
    body: impl FnOnce(&mut egui::Ui, &mut Preferences),
) {
    let searching = ui
        .data(|d| d.get_temp::<bool>(egui::Id::new("filter-searching")))
        .unwrap_or(false);
    let id = ui.make_persistent_id((key, searching));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    if searching {
        state.set_open(true);
    }
    state
        .show_header(ui, |ui| {
            selection_checkbox(ui, label, group, pool, prefs);
            ui.label(
                RichText::new(format!("{}/{}", selected_count(group, prefs), group.len()))
                    .small()
                    .color(MUTED),
            );
        })
        .body(|ui| body(ui, prefs));
}

fn filter_versions(ui: &mut egui::Ui, models: &[&Model], pool: &[Model], prefs: &mut Preferences) {
    let mut versions: BTreeMap<&str, Vec<&Model>> = BTreeMap::new();
    for model in models {
        versions
            .entry(version_name(&model.name))
            .or_default()
            .push(model);
    }
    for (version, mut variants) in versions {
        variants.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        let has_variants = pool
            .iter()
            .filter(|m| m.provider == variants[0].provider && version_name(&m.name) == version)
            .count()
            > 1;
        if !has_variants {
            ui.push_id(&variants[0].id, |ui| {
                selection_checkbox(ui, &variants[0].name, &variants, pool, prefs)
            });
        } else {
            filter_group(
                ui,
                ("version", version),
                version,
                &variants,
                pool,
                prefs,
                |ui, prefs| {
                    for model in &variants {
                        ui.push_id(&model.id, |ui| {
                            selection_checkbox(ui, &model.name, &[*model], pool, prefs)
                        });
                    }
                },
            );
        }
    }
}

fn filter_base_models(
    ui: &mut egui::Ui,
    models: &[&Model],
    pool: &[Model],
    prefs: &mut Preferences,
) {
    let mut bases: BTreeMap<&str, Vec<&Model>> = BTreeMap::new();
    for model in models {
        bases
            .entry(base_model_name(&model.name))
            .or_default()
            .push(model);
    }
    for (base, releases) in bases {
        let versions: std::collections::BTreeSet<_> = pool
            .iter()
            .filter(|m| m.provider == releases[0].provider && base_model_name(&m.name) == base)
            .map(|m| version_name(&m.name))
            .collect();
        if versions.len() > 1 {
            filter_group(
                ui,
                ("base-model", base),
                base,
                &releases,
                pool,
                prefs,
                |ui, prefs| filter_versions(ui, &releases, pool, prefs),
            );
        } else {
            filter_versions(ui, &releases, pool, prefs);
        }
    }
}

fn filter_tree(
    ui: &mut egui::Ui,
    pool: &[Model],
    models: &[&Model],
    prefs: &mut Preferences,
    searching: bool,
) {
    // Search expansion is separate so clearing it restores the normal tree.
    ui.data_mut(|d| d.insert_temp(egui::Id::new("filter-searching"), searching));
    let mut providers: BTreeMap<&str, Vec<&Model>> = BTreeMap::new();
    for model in models {
        providers.entry(&model.provider).or_default().push(model);
    }
    for (provider, group) in providers {
        filter_group(
            ui,
            ("provider", provider),
            provider_label(provider),
            &group,
            pool,
            prefs,
            |ui, prefs| {
                let mut families: BTreeMap<&str, Vec<&Model>> = BTreeMap::new();
                for model in &group {
                    families
                        .entry(model_family(&model.name))
                        .or_default()
                        .push(model);
                }
                for (family, variants) in families {
                    let distinct: std::collections::BTreeSet<_> = pool
                        .iter()
                        .filter(|m| m.provider == provider && model_family(&m.name) == family)
                        .map(|m| base_model_name(&m.name))
                        .collect();
                    if distinct.len() == 1 {
                        filter_base_models(ui, &variants, pool, prefs);
                    } else {
                        filter_group(
                            ui,
                            ("family", family),
                            family,
                            &variants,
                            pool,
                            prefs,
                            |ui, prefs| filter_base_models(ui, &variants, pool, prefs),
                        );
                    }
                }
            },
        );
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Map,
    Charts,
    Settings,
}

pub struct PickerApp {
    store: Store,
    prefs: Preferences,
    snapshot: Option<Snapshot>,
    tray: Option<Tray>,
    fetch: Option<Receiver<Result<Snapshot, FetchError>>>,
    key: String,
    key_draft: String,
    remember_key: bool,
    error: Option<String>,
    notice: String,
    expanded: bool,
    tab: Tab,
    filters: bool,
    query: String,
    next_auto: u64,
    quitting: bool,
    start_hidden: bool,
    demo_backup: Option<Preferences>,
}

pub fn configure_style(ctx: &egui::Context) {
    theme::configure(ctx);
}

impl PickerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        store: Store,
        demo: bool,
        start_hidden: bool,
    ) -> Self {
        configure_style(&cc.egui_ctx);
        let mut app = Self::load(store, demo, start_hidden);
        match Tray::new(&cc.egui_ctx) {
            Ok(tray) => app.tray = Some(tray),
            Err(e) => app.error = Some(format!("Трей недоступен: {e}")),
        }
        app
    }

    fn load(store: Store, demo: bool, start_hidden: bool) -> Self {
        let mut errors = Vec::new();
        let prefs = store.load_preferences().unwrap_or_else(|e| {
            errors.push(e);
            Preferences::default()
        });
        let snapshot = store.load_snapshot().unwrap_or_else(|e| {
            errors.push(e);
            None
        });
        let saved_key = store.load_key().unwrap_or_else(|e| {
            errors.push(e);
            None
        });
        let remember_key = saved_key.is_some();
        let key = std::env::var("ARTIFICIAL_ANALYSIS_API_KEY")
            .ok()
            .or(saved_key)
            .unwrap_or_default();
        let next_auto = snapshot
            .as_ref()
            .map(|s| s.fetched_at.saturating_add(86400))
            .unwrap_or(0);
        let demo_backup = demo.then(|| prefs.clone());
        Self {
            store,
            prefs: if demo { Preferences::default() } else { prefs },
            snapshot: if demo {
                Some(Snapshot::demo())
            } else {
                snapshot
            },
            tray: None,
            fetch: None,
            key_draft: key.clone(),
            key,
            remember_key,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("\n"))
            },
            notice: String::new(),
            expanded: false,
            tab: Tab::Map,
            filters: false,
            query: String::new(),
            next_auto,
            quitting: false,
            start_hidden,
            demo_backup,
        }
    }

    fn is_demo(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.demo)
    }

    fn persist(&mut self, before: &Preferences) {
        if self.prefs != *before
            && !self.is_demo()
            && let Err(e) = self.store.save_preferences(&self.prefs)
        {
            self.error = Some(e);
        }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if self.fetch.is_some() || self.is_demo() || now() < self.prefs.next_request_at {
            return;
        }
        if self.key.trim().is_empty() {
            self.resize(ctx, true, false);
            self.tab = Tab::Settings;
            self.error = Some("Добавьте бесплатный API-ключ.".into());
            return;
        }
        self.error = None;
        self.notice = "Загрузка Artificial Analysis…".into();
        self.prefs.next_request_at = now().saturating_add(60);
        self.next_auto = now().saturating_add(900);
        let key = self.key.clone();
        let store = self.store.clone();
        let wake = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.fetch = Some(rx);
        std::thread::spawn(move || {
            let result = source::fetch_snapshot(source::ENDPOINT, &key).and_then(|snapshot| {
                store.save_snapshot(&snapshot).map_err(FetchError::from)?;
                Ok(snapshot)
            });
            let _ = tx.send(result);
            wake.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.fetch {
            match receiver.try_recv() {
                Ok(result) => {
                    self.fetch = None;
                    match result {
                        Ok(snapshot) => {
                            self.next_auto = snapshot.fetched_at.saturating_add(86400);
                            self.notice = format!("Обновлено: {} моделей", snapshot.models.len());
                            self.snapshot = Some(snapshot);
                            self.error = None;
                        }
                        Err(error) => {
                            if let Some(delay) = error.retry_after {
                                self.prefs.next_request_at = now().saturating_add(delay);
                                self.next_auto = self.prefs.next_request_at;
                            }
                            self.error = Some(error.message);
                            self.notice.clear();
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.fetch = None;
                    self.error = Some("Загрузка прервана. Повторите обновление.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.key.is_empty()
            && !self.is_demo()
            && now() >= self.next_auto
            && now() >= self.prefs.next_request_at
        {
            self.refresh(ctx);
        }
    }

    fn size(&self) -> Vec2 {
        if self.expanded {
            EXPANDED
        } else if self.filters {
            FILTER
        } else {
            COMPACT
        }
    }

    fn resize(&mut self, ctx: &egui::Context, expanded: bool, filters: bool) {
        if expanded && !self.expanded {
            self.tab = Tab::Map;
        }
        self.expanded = expanded;
        self.filters = filters;
        let available = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(EXPANDED + vec2(60.0, 80.0));
        let size = self.size().min((available - vec2(24.0, 64.0)).max(COMPACT));
        ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(if expanded {
            vec2(780.0, 580.0).min(size)
        } else {
            size
        }));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(expanded));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let scale = ctx.pixels_per_point();
            tray::position_near(
                ctx,
                (
                    ((rect.left() + size.x) * scale) as f64,
                    ((rect.top() + size.y + 40.0) * scale) as f64,
                ),
                size,
            );
        }
    }

    fn toggle_demo(&mut self) {
        if self.is_demo() {
            if let Some(p) = self.demo_backup.take() {
                self.prefs = p;
            }
            match self.store.load_snapshot() {
                Ok(s) => self.snapshot = s,
                Err(e) => {
                    self.snapshot = None;
                    self.error = Some(e);
                }
            }
        } else {
            self.demo_backup = Some(self.prefs.clone());
            self.prefs = Preferences::default();
            self.snapshot = Some(Snapshot::demo());
            self.error = None;
        }
        self.notice.clear();
    }

    fn compact_models(&mut self) -> Vec<Model> {
        let pool = self
            .snapshot
            .as_ref()
            .map(|s| s.models.as_slice())
            .unwrap_or_default();
        let ordered: Vec<Model> = balanced_models(pool, &self.prefs)
            .into_iter()
            .cloned()
            .collect();
        let collapsed = collapse_reasoning(&ordered, &self.prefs);
        if let Some(id) = &self.prefs.selected
            && let Some(kept) = collapsed.replacements.get(id)
        {
            self.prefs.selected = Some(kept.clone());
        }
        if !collapsed
            .models
            .iter()
            .any(|m| Some(&m.id) == self.prefs.selected.as_ref())
        {
            self.prefs.selected = collapsed.models.last().map(|m| m.id.clone());
        }
        collapsed.models
    }

    fn header(&mut self, ui: &mut egui::Ui, models: &[Model]) {
        ui.horizontal(|ui| {
            if charts::icon_button(ui, charts::ControlIcon::Filter, "Выбрать модели").clicked()
            {
                self.resize(ui.ctx(), self.expanded, !self.filters);
            }
            let title = if self.expanded {
                "AI Picker".to_string()
            } else {
                models
                    .iter()
                    .find(|m| Some(&m.id) == self.prefs.selected.as_ref())
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| {
                        if self.snapshot.is_some() {
                            "Выберите модели".into()
                        } else {
                            "Ваш пикер моделей".into()
                        }
                    })
            };
            let width = (ui.available_width() - 68.0).max(80.0);
            let label = ui.add_sized(
                [width, 30.0],
                egui::Label::new(charts::model_title(&title))
                    .truncate()
                    .sense(egui::Sense::drag()),
            );
            label.clone().on_hover_text(format!(
                "{title}\nПеретащите заголовок, чтобы переместить окно."
            ));
            if label.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if charts::icon_button(
                ui,
                if self.expanded {
                    charts::ControlIcon::Collapse
                } else {
                    charts::ControlIcon::Expand
                },
                if self.expanded {
                    "Компактный пикер"
                } else {
                    "Расширенный вид"
                },
            )
            .clicked()
            {
                self.resize(ui.ctx(), !self.expanded, false);
            }
            if charts::icon_button(ui, charts::ControlIcon::Close, "Скрыть в трей").clicked()
            {
                ui.ctx().send_viewport_cmd(ViewportCommand::Close);
            }
        });
    }

    fn compact(&mut self, ui: &mut egui::Ui, models: &[Model]) {
        charts::simple_rail(ui, models, &mut self.prefs.selected);
        ui.horizontal(|ui| {
            let pool = self
                .snapshot
                .as_ref()
                .map(|s| s.models.as_slice())
                .unwrap_or_default();
            if let Some(selected) = models
                .iter()
                .find(|m| Some(&m.id) == self.prefs.selected.as_ref())
            {
                let summary = balance(selected, pool, self.prefs.quality_weight);
                ui.label(
                    RichText::new(format!("Качество {}", charts::number(summary.quality.map(|q| q * 100.0))))
                        .size(11.0)
                        .color(MUTED),
                )
                .on_hover_text("Средний ранг доступных индексов Intelligence, Coding и Agentic среди всех загруженных моделей. Правее — выше качество; при равном качестве — выше стоимость задачи AA. Цена не влияет на оценку качества.");
            } else if self.snapshot.is_none() {
                if ui
                    .link(RichText::new("Подключить данные").size(11.0))
                    .clicked()
                {
                    self.resize(ui.ctx(), true, false);
                    self.tab = Tab::Settings;
                }
            } else {
                ui.label(
                    RichText::new("Все модели отключены")
                        .size(11.0)
                        .color(MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !models.is_empty() {
                    ui.label(
                        RichText::new(format!("{} вариантов", models.len()))
                            .size(11.0)
                            .color(MUTED),
                    );
                }
            });
        });
    }

    fn filters_ui(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Модели в пикере", "Выберите провайдеров, семейства и версии.");
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Найти провайдера или модель")
                .margin(egui::Margin::symmetric(10, 8))
                .desired_width(f32::INFINITY),
        );
        let pool = self
            .snapshot
            .as_ref()
            .map(|s| s.models.as_slice())
            .unwrap_or_default();
        let models = matching_models(pool, &self.query);
        let searching = !self.query.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !models.is_empty(),
                    egui::Button::new(if searching {
                        "Все найденные"
                    } else {
                        "Все"
                    })
                    .small(),
                )
                .clicked()
            {
                select_models(pool, &models, &mut self.prefs, true);
            }
            if ui
                .add_enabled(
                    !models.is_empty(),
                    egui::Button::new(if searching {
                        "Убрать найденные"
                    } else {
                        "Ни одной"
                    })
                    .small(),
                )
                .clicked()
            {
                select_models(pool, &models, &mut self.prefs, false);
            }
            ui.label(
                RichText::new(format!(
                    "{}/{}",
                    selected_count(&models, &self.prefs),
                    models.len()
                ))
                .small()
                .color(MUTED),
            );
        });
        egui::ScrollArea::vertical()
            .id_salt("model-filter-list")
            .max_height(210.0)
            .show(ui, |ui| {
                if models.is_empty() {
                    ui.label(if pool.is_empty() {
                        "Нет загруженных моделей"
                    } else {
                        "Ничего не найдено"
                    });
                } else {
                    filter_tree(ui, pool, &models, &mut self.prefs, searching);
                }
            });
        ui.separator();
        ui.checkbox(
            &mut self.prefs.collapse_reasoning,
            "Оптимальные уровни reasoning",
        );
        ui.label(
            RichText::new(
                "Схлопывать почти одинаковые, но более дорогие варианты в простом пикере.",
            )
            .size(11.0)
            .color(MUTED),
        );
        egui::CollapsingHeader::new("Правила и скрытые варианты").show(ui,|ui| {
            ui.add(egui::Slider::new(&mut self.prefs.reasoning_tolerance,0.0..=10.0).step_by(0.5).text("потеря баллов, не более"));
            let mut saving=self.prefs.reasoning_savings*100.0;
            ui.add(egui::Slider::new(&mut saving,1.0..=90.0).suffix("%").text("экономия, не менее"));
            self.prefs.reasoning_savings=saving/100.0;
            ui.label(RichText::new("Одна версия модели. Проверяем каждый доступный индекс и стоимость задачи AA. Неизвестные уровни и отсутствие нужных данных не заменяем догадкой. Карта показывает все включённые варианты.").size(11.0).color(MUTED));
            if let Some(snapshot)=&self.snapshot {
                let models:Vec<_>=ordered_models(&snapshot.models,&self.prefs).into_iter().cloned().collect();
                for (hidden,kept) in collapse_reasoning(&models,&self.prefs).replacements {
                    if let (Some(a),Some(b))=(models.iter().find(|m|m.id==hidden),models.iter().find(|m|m.id==kept)) {
                        ui.label(RichText::new(format!("{} → {}",a.name,b.name)).size(11.0));
                        ui.label(RichText::new(format!("Задача AA: {} → {}",charts::money(a.task_cost),charts::money(b.task_cost))).size(10.0).color(MUTED));
                    }
                }
            }
        });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Подключение данных", "Artificial Analysis · личный API-ключ");
        ui.add(
            egui::TextEdit::singleline(&mut self.key_draft)
                .password(true)
                .hint_text("API-ключ")
                .margin(egui::Margin::symmetric(10, 8))
                .desired_width(480.0),
        );
        ui.checkbox(&mut self.remember_key, "Сохранить защищённо в Windows");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.fetch.is_none() && !self.is_demo(),
                    theme::primary_button("Применить и загрузить"),
                )
                .clicked()
            {
                let key = self.key_draft.trim().to_string();
                match self
                    .store
                    .save_key(if self.remember_key { &key } else { "" })
                {
                    Ok(()) => {
                        self.key = key;
                        self.error = None;
                        self.refresh(ui.ctx());
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            ui.hyperlink_to(
                "Получить бесплатный ключ",
                "https://artificialanalysis.ai/data-api",
            );
        });
        ui.label(RichText::new("Обновление раз в сутки. 100 запросов/сутки на Free; каждая страница — отдельный запрос. Личное/внутреннее использование с указанием источника.").size(12.0).color(MUTED));
        ui.add_space(12.0);
        ui.separator();
        theme::section(ui, "Баланс качества и стоимости", "Настройте приоритет для оценки выбранной модели.");
        ui.label("Основной слайдер упорядочен по качеству. Этот вес меняет только балл баланса.");
        let mut weight = self.prefs.quality_weight * 100.0;
        ui.add(
            egui::Slider::new(&mut weight, 0.0..=100.0)
                .suffix("%")
                .text("вес качества"),
        );
        self.prefs.quality_weight = weight / 100.0;
        ui.label(format!(
            "Качество: {:.0}% · стоимость: {:.0}%",
            weight,
            100.0 - weight
        ));
        ui.label("Качество объединяет AA Intelligence, Coding и Agentic. Стоимость объединяет тариф входа, выхода и стоимость задачи AA. Каждый показатель переводится в относительный ранг среди всего загруженного пула. Внутри группы веса равны.");
        ui.label(RichText::new("Баланс = 100 × (вес качества × средний ранг качества + вес стоимости × средний обратный ранг цен). Это балл приложения, не официальный бенчмарк. Выключение моделей не меняет базу расчёта. При неполных данных показываем число доступных показателей; без группы качества или стоимости балл отсутствует.").size(12.0).color(MUTED));
        ui.add_space(12.0);
        ui.separator();
        if ui
            .add_enabled(
                self.fetch.is_none(),
                theme::button(if self.is_demo() {
                    "Вернуться к данным API"
                } else {
                    "Посмотреть демо без ключа"
                }),
            )
            .clicked()
        {
            self.toggle_demo();
        }
        if !self.notice.is_empty() {
            ui.label(&self.notice);
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).color(WARNING));
        }
        if self.prefs.next_request_at > now() {
            ui.label(format!(
                "Следующий запрос доступен через {} с",
                self.prefs.next_request_at - now()
            ));
        }
    }

    fn detail(&mut self, ui: &mut egui::Ui, models: &[Model]) {
        if models.is_empty() {
            ui.label("Включите модели в фильтре.");
            return;
        }
        if !models
            .iter()
            .any(|m| Some(&m.id) == self.prefs.selected.as_ref())
        {
            self.prefs.selected = Some(models[0].id.clone());
        }
        let selected = models
            .iter()
            .find(|m| Some(&m.id) == self.prefs.selected.as_ref())
            .unwrap_or(&models[0]);
        ui.label(RichText::new("ВЫБРАННАЯ МОДЕЛЬ").size(11.0).color(MUTED));
        egui::ComboBox::from_id_salt("detail-model")
            .width(ui.available_width())
            .selected_text(&selected.name)
            .show_ui(ui, |ui| {
                for model in models {
                    ui.selectable_value(
                        &mut self.prefs.selected,
                        Some(model.id.clone()),
                        &model.name,
                    );
                }
            });
        ui.add_space(10.0);
        let pool = self
            .snapshot
            .as_ref()
            .map(|s| s.models.as_slice())
            .unwrap_or_default();
        let summary = balance(selected, pool, self.prefs.quality_weight);
        ui.label(
            RichText::new(format!("Баланс {}", charts::number(summary.score)))
                .size(27.0)
                .color(ACCENT),
        );
        ui.label(
            RichText::new(format!(
                "{} / 6 показателей{}",
                summary.covered,
                if summary.covered < 6 {
                    " · неполные данные"
                } else {
                    ""
                }
            ))
            .size(12.0)
            .color(MUTED),
        );
        ui.add_space(10.0);
        egui::Grid::new("model-metrics")
            .spacing(vec2(12.0, 12.0))
            .show(ui, |ui| {
                for metric in Metric::ALL {
                    ui.label(metric.label());
                    ui.label(charts::number(selected.score(metric)));
                    ui.end_row();
                }
                for (label, value) in [
                    ("Вход / 1 млн токенов", selected.input_price),
                    ("Выход / 1 млн токенов", selected.output_price),
                    ("Задача AA", selected.task_cost),
                ] {
                    ui.label(label);
                    ui.label(charts::money(value));
                    ui.end_row();
                }
            });
        ui.add_space(14.0);
        ui.label(RichText::new("Стоимость задачи AA учитывает расход токенов на тест. Она не предсказывает цену вашей задачи.").size(11.0).color(MUTED));
        ui.hyperlink_to(
            "Методика Artificial Analysis",
            "https://artificialanalysis.ai/methodology/intelligence-benchmarking",
        );
    }

    fn expanded_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [(Tab::Map, "Пикер"), (Tab::Charts, "Бенчмарки"), (Tab::Settings, "Настройки")] {
                if theme::segment(ui, self.tab == tab, label).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.fetch.is_some() { ui.spinner(); }
                if ui.add_enabled(
                    self.fetch.is_none() && !self.is_demo() && now() >= self.prefs.next_request_at,
                    theme::button("Обновить"),
                ).clicked() { self.refresh(ui.ctx()); }
            });
        });
        ui.add_space(16.0);
        if self.filters {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.filters_ui(ui);
            });
            ui.add_space(16.0);
        }
        if self.tab == Tab::Settings {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.settings_ui(ui);
            });
            return;
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).size(12.0).color(WARNING));
        }
        if self.tab == Tab::Map {
            theme::section(ui, "Двумерный пикер", "Двигайте точку по качеству и стоимости. Отпустите — она плавно выберет ближайшую модель.");
        } else {
            theme::section(ui, "Сравнение моделей", "Расположение на карте и результаты бенчмарков для включённых моделей.");
        }
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("metric")
                .selected_text(self.prefs.metric.label())
                .show_ui(ui, |ui| {
                    for metric in Metric::ALL {
                        ui.selectable_value(&mut self.prefs.metric, metric, metric.label());
                    }
                });
            egui::ComboBox::from_id_salt("price-basis")
                .selected_text(self.prefs.price_mode.label())
                .show_ui(ui, |ui| {
                    for mode in PriceMode::ALL {
                        ui.selectable_value(&mut self.prefs.price_mode, mode, mode.label());
                    }
                });
            if self.tab == Tab::Charts {
                for (sort, label) in [(SortBy::Price, "Дешевле"), (SortBy::Quality, "Сильнее")] {
                    if theme::segment(ui, self.prefs.sort == sort, label).clicked() {
                        self.prefs.sort = sort;
                    }
                }
            }
        });
        if self.prefs.price_mode == PriceMode::Blended {
            let mut input = self.prefs.input_share * 100.0;
            ui.add(egui::Slider::new(&mut input, 0.0..=100.0).suffix("%").text("доля входных токенов"));
            self.prefs.input_share = input / 100.0;
        }
        let models: Vec<Model> = self.snapshot.as_ref().map(|s| {
            ordered_models(&s.models, &self.prefs).into_iter().cloned().collect()
        }).unwrap_or_default();
        ui.label(RichText::new(format!("{} моделей · все включённые уровни reasoning", models.len())).size(11.0).color(MUTED));
        ui.add_space(8.0);
        if self.tab == Tab::Map {
            let gap = 16.0;
            let width = ui.available_width();
            let detail_width = ((width - gap) * 0.4).max(280.0);
            let picker_width = (width - gap - detail_width).max(240.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.allocate_ui_with_layout(vec2(picker_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    charts::picker_2d(ui, &models, &mut self.prefs);
                });
                ui.allocate_ui_with_layout(vec2(detail_width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    theme::card().show(ui, |ui| {
                        ui.set_width((detail_width - 34.0).max(0.0));
                        self.detail(ui, &models);
                    });
                });
            });
        } else {
            ui.columns(2, |columns| {
                theme::section(&mut columns[0], "Карта моделей", "Стоимость и выбранный показатель");
                charts::scatter(&mut columns[0], &models, &mut self.prefs);
                theme::section(&mut columns[1], "Рейтинг моделей", self.prefs.metric.label());
                charts::bars(&mut columns[1], &models, &mut self.prefs);
            });
            ui.add_space(16.0);
            theme::card().show(ui, |ui| self.detail(ui, &models));
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.is_demo() {
                ui.label(
                    RichText::new("Демо · синтетические данные")
                        .size(10.0)
                        .color(WARNING),
                );
            } else if self.fetch.is_some() {
                ui.label(RichText::new("Обновление…").size(10.0).color(MUTED));
            } else if self.error.is_some() {
                ui.label(
                    RichText::new("Ошибка обновления · откройте большой вид")
                        .size(10.0)
                        .color(WARNING),
                );
            } else if let Some(snapshot) = &self.snapshot {
                let age = now().saturating_sub(snapshot.fetched_at);
                let age_text = if age < 60 {
                    "только что".into()
                } else if age < 3600 {
                    format!("{} мин назад", age / 60)
                } else {
                    format!("{} ч назад", age / 3600)
                };
                ui.label(
                    RichText::new(format!("Обновлено {age_text}"))
                        .size(10.0)
                        .color(if age > 86400 { WARNING } else { MUTED }),
                );
            } else {
                ui.label(
                    RichText::new("Бесплатный источник бенчмарков")
                        .size(10.0)
                        .color(MUTED),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.hyperlink_to(
                    RichText::new("Artificial Analysis").size(10.0),
                    "https://artificialanalysis.ai",
                );
            });
        });
    }

    fn render(&mut self, root: &mut egui::Ui) {
        let models = if self.expanded {
            Vec::new()
        } else {
            self.compact_models()
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::CANVAS)
                    .corner_radius(theme::WINDOW_RADIUS)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(14.0),
            )
            .show(root, |ui| {
                if !self.expanded {
                    ui.spacing_mut().item_spacing.y = 5.0;
                    ui.spacing_mut().interact_size.y = 18.0;
                }
                self.header(ui, &models);
                if self.expanded {
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("expanded-content")
                        .max_height(ui.available_height() - 32.0)
                        .show(ui, |ui| self.expanded_ui(ui));
                    ui.add_space(8.0);
                    self.footer(ui);
                } else {
                    self.compact(ui, &models);
                    if self.filters {
                        ui.add_space(8.0);
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt("filters-content")
                            .max_height(ui.available_height() - 24.0)
                            .show(ui, |ui| self.filters_ui(ui));
                    }
                    self.footer(ui);
                }
            });
    }
}

impl eframe::App for PickerApp {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let before = self.prefs.clone();
        if let Some(tray) = &self.tray {
            while let Ok(event) = tray.events.try_recv() {
                match event {
                    tray::Event::Show(location) => {
                        if let Some(point) = location {
                            tray::position_near(
                                ctx,
                                point,
                                ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()))
                                    .unwrap_or(self.size()),
                            );
                        }
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                    tray::Event::Quit => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            }
        }
        if self.start_hidden && self.tray.is_some() {
            self.start_hidden = false;
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        self.poll(ctx);
        self.persist(&before);
        ctx.request_repaint_after(if self.fetch.is_some() {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(30)
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let before = self.prefs.clone();
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.tray.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        self.render(ui);
        self.persist(&before);
    }

    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expanded_opens_picker_and_benchmarks_contain_both_comparisons() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PickerApp::load(Store::new(directory.path().to_path_buf()).unwrap(), true, false);
        let ctx = egui::Context::default();
        configure_style(&ctx);
        app.tab = Tab::Settings;
        app.resize(&ctx, true, false);
        assert!(app.tab == Tab::Map, "expanding must return to the picker");
        let render = |app: &mut PickerApp| {
            let mut output = ctx.run_ui(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, EXPANDED)),
                ..Default::default()
            }, |root| app.render(root));
            output.textures_delta.clear();
            output.shapes.iter().filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            }).collect::<Vec<_>>()
        };
        render(&mut app);
        let picker = render(&mut app);
        assert!(picker.iter().any(|s| s == "Двумерный пикер"));
        assert!(!picker.iter().any(|s| s == "Карта моделей"));
        app.tab = Tab::Charts;
        render(&mut app);
        let benchmarks = render(&mut app);
        assert!(benchmarks.iter().any(|s| s == "Карта моделей"));
        assert!(benchmarks.iter().any(|s| s == "Рейтинг моделей"));
    }

    #[test]
    fn versions_group_compound_claude_reasoning_variants() {
        for base in ["Claude Sonnet 5", "Claude Opus 5"] {
            for suffix in [
                "Adaptive Reasoning, High Effort",
                "Adaptive Reasoning, Low Effort",
                "Adaptive Reasoning, Max Effort",
                "Adaptive Reasoning, Medium Effort",
                "Adaptive Reasoning, Xhigh Effort",
                "Non-reasoning, High Effort",
                "Adaptive Reasoning, Low Effort, Default Fallback",
            ] {
                let name = format!("{base} ({suffix})");
                assert_eq!(version_name(&name), base, "{name}");
            }
        }
    }

    #[test]
    fn versions_preserve_release_dates_and_unknown_suffixes() {
        assert_eq!(
            version_name("Claude 3.5 Sonnet (Oct '24) (high)"),
            "Claude 3.5 Sonnet (Oct '24)"
        );
        assert_eq!(
            version_name("Claude 3.5 Sonnet (Oct '24)"),
            "Claude 3.5 Sonnet (Oct '24)"
        );
        assert_eq!(
            version_name("GPT-4o mini (preview)"),
            "GPT-4o mini (preview)"
        );
        assert_eq!(version_name("GPT-5 (High effort)"), "GPT-5");
        assert_eq!(
            version_name("Claude 3.5 Sonnet (Oct '24) (Adaptive Reasoning, High Effort)"),
            "Claude 3.5 Sonnet (Oct '24)"
        );
        for name in [
            "Claude Sonnet 5 (Adaptive Reasoning, Sep '26)",
            "Claude Sonnet 5 (Adaptive Reasoning, Preview)",
            "Claude Sonnet 5 (High Effort, Unknown)",
            "Claude Sonnet 5 (Default Fallback)",
            "Claude Sonnet 5 (Adaptive Reasoning, )",
        ] {
            assert_eq!(version_name(name), name);
        }
    }

    #[test]
    fn selecting_search_results_preserves_other_provider_and_hidden_choices() {
        let pool = Snapshot::demo().models;
        let mut prefs = Preferences {
            openai: false,
            ..Default::default()
        };
        let found = matching_models(&pool, "Codex Пример (low)");
        assert_eq!(
            found.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["demo-0"]
        );
        select_models(&pool, &found, &mut prefs, true);
        assert!(prefs.openai);
        assert_eq!(
            ordered_models(&pool, &prefs)
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            vec!["demo-0", "demo-1", "demo-3", "demo-5", "demo-7"]
        );
        let openai = matching_models(&pool, " OPENAI ");
        assert_eq!(selected_count(&openai, &prefs), 1);
        select_models(&pool, &openai, &mut prefs, true);
        assert_eq!(selected_count(&openai, &prefs), 4);
        select_models(&pool, &found, &mut prefs, false);
        assert_eq!(selected_count(&openai, &prefs), 3);
        assert_eq!(
            selected_count(&matching_models(&pool, "Anthropic"), &prefs),
            4
        );
    }

    #[test]
    fn families_keep_legacy_names_together_without_guessing_unknown_names() {
        assert_eq!(model_family("Claude 3.5 Sonnet (Oct '24)"), "Claude Sonnet");
        assert_eq!(model_family("Claude Sonnet 4.5"), "Claude Sonnet");
        assert_eq!(model_family("GPT-4o mini"), "GPT");
        assert_eq!(model_family("GPT-5.3 Codex"), "Codex");
        assert_eq!(model_family("o3-pro"), "o-series");
        assert_eq!(model_family("Unknown (preview)"), "Unknown");
    }

    #[test]
    fn families_use_model_identity_instead_of_fallback_metadata() {
        assert_eq!(
            model_family("Claude Fable 5 (Adaptive Reasoning, Max Effort, Opus 4.8 Fallback)"),
            "Claude Fable"
        );
        assert_eq!(
            model_family("Claude Fable 5.1 (Adaptive Reasoning, High Effort, Default Fallback)"),
            "Claude Fable"
        );
        assert_eq!(model_family("gpt-oss-20b (high)"), "gpt-oss");
        assert_eq!(model_family("gpt-oss-120b (low)"), "gpt-oss");
        assert_eq!(version_name("gpt-oss-20b (high)"), "gpt-oss-20b");
        assert_eq!(version_name("gpt-oss-120b (low)"), "gpt-oss-120b");
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::default(),
            },
        ]
    }

    #[test]
    fn full_family_tree_collapses_and_expands_gpt4o_releases_without_search() {
        let names = [
            "GPT-4o (Aug '24)",
            "GPT-4o (ChatGPT)",
            "GPT-4o (March 2025, chatgpt-4o-latest)",
            "GPT-4o (May '24)",
            "GPT-4o (Nov '24)",
            "GPT-4o mini",
            "GPT-4.5 (Preview)",
        ];
        let template = Snapshot::demo().models[0].clone();
        let pool: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, name)| Model {
                id: format!("release-{i}"),
                name: (*name).into(),
                provider: "openai".into(),
                ..template.clone()
            })
            .collect();
        let models = matching_models(&pool, "");
        let ctx = egui::Context::default();
        configure_style(&ctx);
        ctx.style_mut_of(egui::Theme::Light, |style| style.animation_time = 0.0);
        let mut prefs = Preferences::default();
        let before = prefs.clone();
        let render = |prefs: &mut Preferences, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, FILTER)),
                    events,
                    ..Default::default()
                },
                |ui| filter_tree(ui, &pool, &models, prefs, false),
            );
            output.textures_delta.clear();
            output
        };
        let label = |frame: &egui::FullOutput, name: &str| {
            frame.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == name => Some(t.pos),
                _ => None,
            })
        };
        render(&mut prefs, Vec::new());
        for parent in ["OpenAI / Codex", "GPT"] {
            let frame = render(&mut prefs, Vec::new());
            let chevron = label(&frame, parent).unwrap() + vec2(-35.0, 7.0);
            render(&mut prefs, pointer(chevron, true));
            render(&mut prefs, pointer(chevron, false));
        }
        let frame = render(&mut prefs, Vec::new());
        let chevron = label(&frame, "GPT-4o").expect("base group") + vec2(-35.0, 7.0);
        assert!(label(&frame, "GPT-4o mini").is_some());
        assert!(label(&frame, "GPT-4.5 (Preview)").is_some());
        for name in &names[..5] {
            assert!(
                label(&frame, name).is_none(),
                "release should start collapsed: {name}"
            );
        }
        render(&mut prefs, pointer(chevron, true));
        render(&mut prefs, pointer(chevron, false));
        let frame = render(&mut prefs, Vec::new());
        for name in &names[..5] {
            assert!(
                label(&frame, name).is_some(),
                "release should expand: {name}"
            );
        }
        assert_eq!(prefs, before);
        render(&mut prefs, pointer(chevron, true));
        render(&mut prefs, pointer(chevron, false));
        let frame = render(&mut prefs, Vec::new());
        for name in &names[..5] {
            assert!(label(&frame, name).is_none());
        }
    }

    #[test]
    fn release_tree_groups_screenshot_models_and_keeps_individual_choices() {
        for (base, names) in [
            (
                "GPT-3.5 Turbo",
                vec!["GPT-3.5 Turbo", "GPT-3.5 Turbo (0613)"],
            ),
            (
                "GPT-4o",
                vec![
                    "GPT-4o (Aug '24)",
                    "GPT-4o (ChatGPT)",
                    "GPT-4o (March 2025, chatgpt-4o-latest)",
                    "GPT-4o (May '24)",
                    "GPT-4o (Nov '24)",
                ],
            ),
            (
                "GPT-5.5 Instant",
                vec!["GPT-5.5 Instant (June 2026)", "GPT-5.5 Instant (May 2026)"],
            ),
        ] {
            let template = Snapshot::demo().models[0].clone();
            let pool: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(i, name)| Model {
                    id: format!("release-{i}"),
                    name: (*name).into(),
                    provider: "openai".into(),
                    ..template.clone()
                })
                .collect();
            let models = matching_models(&pool, "");
            let ctx = egui::Context::default();
            configure_style(&ctx);
            ctx.style_mut_of(egui::Theme::Light, |style| style.animation_time = 0.0);
            let mut prefs = Preferences::default();
            let render = |prefs: &mut Preferences, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, FILTER)),
                        events,
                        ..Default::default()
                    },
                    |ui| filter_tree(ui, &pool, &models, prefs, true),
                );
                output.textures_delta.clear();
                output
            };
            render(&mut prefs, Vec::new());
            let frame = render(&mut prefs, Vec::new());
            let labels: Vec<_> = frame
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some((t.galley.text(), t.pos)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels.iter().filter(|(name, _)| *name == base).count(),
                1 + usize::from(names.contains(&base)),
                "missing parent group for {base}"
            );
            let leaf =
                labels.iter().find(|(name, _)| *name == names[1]).unwrap().1 + vec2(4.0, 4.0);
            render(&mut prefs, pointer(leaf, true));
            render(&mut prefs, pointer(leaf, false));
            assert_eq!(
                prefs
                    .disabled
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                vec!["release-1"]
            );
        }
    }

    #[test]
    fn tree_expansion_preserves_selection_and_group_checkbox_changes_children() {
        let pool = Snapshot::demo().models;
        let models = matching_models(&pool, "");
        let ctx = egui::Context::default();
        configure_style(&ctx);
        ctx.style_mut_of(egui::Theme::Light, |style| style.animation_time = 0.0);
        let mut prefs = Preferences::default();
        let before = prefs.clone();
        let render = |prefs: &mut Preferences, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, FILTER)),
                    events,
                    ..Default::default()
                },
                |ui| {
                    filter_tree(ui, &pool, &models, prefs, false);
                },
            );
            output.textures_delta.clear();
            output
        };
        let label = |frame: &egui::FullOutput, name: &str| {
            frame.shapes.iter().find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == name => Some(t.pos),
                _ => None,
            })
        };
        render(&mut prefs, Vec::new());
        let frame = render(&mut prefs, Vec::new());
        let provider = label(&frame, "Anthropic / Claude").unwrap();
        assert!(label(&frame, "Claude Пример").is_none());
        // The chevron precedes the checkbox and its label.
        let chevron = provider + vec2(-35.0, 7.0);
        render(&mut prefs, pointer(chevron, true));
        render(&mut prefs, pointer(chevron, false));
        let frame = render(&mut prefs, Vec::new());
        assert!(label(&frame, "Claude Пример").is_some());
        assert_eq!(prefs, before);
        let checkbox = provider + vec2(4.0, 4.0);
        render(&mut prefs, pointer(checkbox, true));
        render(&mut prefs, pointer(checkbox, false));
        assert_eq!(
            selected_count(&matching_models(&pool, "Anthropic"), &prefs),
            0
        );
        assert_eq!(selected_count(&matching_models(&pool, "OpenAI"), &prefs), 4);
    }

    fn chart_frame(
        ctx: &egui::Context,
        models: &[Model],
        prefs: &mut Preferences,
        events: Vec<egui::Event>,
        bars: bool,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(650.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if bars {
                        charts::bars(ui, models, prefs)
                    } else {
                        charts::scatter(ui, models, prefs)
                    }
                });
            },
        );
        output.textures_delta.clear();
        output
    }

    #[test]
    fn clicking_scatter_point_and_benchmark_row_selects_their_model() {
        let models = Snapshot::demo().models;
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut prefs = Preferences {
            selected: Some("demo-6".into()),
            ..Default::default()
        };
        chart_frame(&ctx, &models, &mut prefs, Vec::new(), false);
        let frame = chart_frame(&ctx, &models, &mut prefs, Vec::new(), false);
        let point = frame
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Circle(c) if c.fill == charts::MINT && c.radius == 4.5 => {
                    Some(c.center)
                }
                _ => None,
            })
            .unwrap();
        chart_frame(&ctx, &models, &mut prefs, pointer(point, true), false);
        chart_frame(&ctx, &models, &mut prefs, pointer(point, false), false);
        assert_eq!(prefs.selected.as_deref(), Some("demo-0"));
        chart_frame(&ctx, &models, &mut prefs, Vec::new(), true);
        let frame = chart_frame(&ctx, &models, &mut prefs, Vec::new(), true);
        let label = frame
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Codex Пример (xhigh)" => {
                    Some(t.pos + vec2(4.0, 4.0))
                }
                _ => None,
            })
            .unwrap();
        chart_frame(&ctx, &models, &mut prefs, pointer(label, true), true);
        chart_frame(&ctx, &models, &mut prefs, pointer(label, false), true);
        assert_eq!(prefs.selected.as_deref(), Some("demo-6"));
    }

    #[test]
    fn changing_a_model_checkbox_persists_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path().to_path_buf()).unwrap();
        let mut snapshot = Snapshot::demo();
        snapshot.demo = false;
        store.save_snapshot(&snapshot).unwrap();
        let mut app = PickerApp::load(store.clone(), false, false);
        app.filters = true;
        app.query = "OpenAI".into();
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let render = |app: &mut PickerApp, events: Vec<egui::Event>| {
            let mut frame = eframe::Frame::_new_kittest();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, FILTER)),
                    events,
                    ..Default::default()
                },
                |ui| eframe::App::ui(app, ui, &mut frame),
            );
            output.textures_delta.clear();
            output
        };
        render(&mut app, Vec::new());
        let frame = render(&mut app, Vec::new());
        let label = frame
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Codex Пример (low)" => {
                    Some(t.pos + vec2(4.0, 4.0))
                }
                _ => None,
            })
            .unwrap();
        render(&mut app, pointer(label, true));
        render(&mut app, pointer(label, false));
        assert!(app.prefs.disabled.contains("demo-0"));
        let restarted = PickerApp::load(store, false, false);
        assert!(restarted.prefs.disabled.contains("demo-0"));
        assert_eq!(restarted.prefs.selected, app.prefs.selected);
    }
    #[test]
    fn compact_has_no_advanced_controls_and_fits_reference_sized_widget() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = PickerApp::load(
            Store::new(directory.path().to_path_buf()).unwrap(),
            true,
            false,
        );
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut texts = Vec::new();
        let mut max_bottom = 0.0f32;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, COMPACT)),
                ..Default::default()
            },
            |root| {
                app.render(root);
            },
        );
        for shape in &output.shapes {
            if let egui::Shape::Text(t) = &shape.shape {
                texts.push(t.galley.text().to_string());
                max_bottom = max_bottom.max(t.pos.y + t.galley.size().y);
            }
        }
        output.textures_delta.clear();
        assert!(
            !texts
                .iter()
                .any(|s| s == "Обновить" || s == "Сначала дешевле" || s == "API-ключ")
        );
        assert!(texts.iter().any(|s| s.contains("Качество")));
        assert!(
            max_bottom <= COMPACT.y,
            "text extends below widget: {max_bottom}"
        );
        assert!(app.prefs.selected.is_some());
    }
}
