use crate::{
    charts::{self, ACCENT, MUTED},
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
use eframe::egui::{self, Color32, RichText, Vec2, ViewportCommand, vec2};
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

pub const COMPACT: Vec2 = vec2(420.0, 148.0);
const FILTER: Vec2 = vec2(420.0, 590.0);
const EXPANDED: Vec2 = vec2(1000.0, 720.0);
const INK: Color32 = Color32::from_rgb(42, 38, 52);
const SOFT: Color32 = Color32::from_rgb(248, 246, 252);
const WARNING: Color32 = Color32::from_rgb(157, 91, 39);

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
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = SOFT;
    style.visuals.selection.bg_fill = Color32::from_rgb(239, 231, 252);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    style.visuals.widgets.inactive.bg_fill = SOFT;
    style.visuals.widgets.inactive.weak_bg_fill = SOFT;
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, Color32::from_rgb(231, 226, 240));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(242, 235, 251);
    style.visuals.override_text_color = Some(INK);
    style.spacing.item_spacing = vec2(8.0, 7.0);
    style.spacing.button_padding = vec2(10.0, 6.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    ctx.set_style_of(egui::Theme::Light, style);
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
            self.tab = Tab::Settings;
            self.resize(ctx, true, false);
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
                    RichText::new(format!("Баланс {}", charts::number(summary.score)))
                        .size(11.0)
                        .color(MUTED),
                )
                .on_hover_text(format!(
                    "{} из 6 показателей · формула в расширенном виде. Правее — выше баланс.",
                    summary.covered
                ));
            } else if self.snapshot.is_none() {
                if ui
                    .link(RichText::new("Подключить данные").size(11.0))
                    .clicked()
                {
                    self.tab = Tab::Settings;
                    self.resize(ui.ctx(), true, false);
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
        ui.heading("Модели в пикере");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.prefs.openai, "OpenAI / Codex");
            ui.checkbox(&mut self.prefs.anthropic, "Claude");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Найти модель")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            if ui.small_button("Все").clicked() {
                self.prefs.disabled.clear();
                self.prefs.openai = true;
                self.prefs.anthropic = true;
            }
            if ui.small_button("Ни одной").clicked()
                && let Some(s) = &self.snapshot
            {
                self.prefs
                    .disabled
                    .extend(s.models.iter().map(|m| m.id.clone()));
            }
        });
        let query = self.query.to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("model-filter-list")
            .max_height(180.0)
            .show(ui, |ui| {
                if let Some(snapshot) = &self.snapshot {
                    for model in &snapshot.models {
                        if !model.name.to_lowercase().contains(&query) {
                            continue;
                        }
                        let active = (model.provider == "openai" && self.prefs.openai)
                            || (model.provider == "anthropic" && self.prefs.anthropic);
                        let mut enabled = !self.prefs.disabled.contains(&model.id);
                        if ui
                            .add_enabled(active, egui::Checkbox::new(&mut enabled, &model.name))
                            .changed()
                        {
                            if enabled {
                                self.prefs.disabled.remove(&model.id);
                            } else {
                                self.prefs.disabled.insert(model.id.clone());
                            }
                        }
                    }
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
        ui.heading("Данные и баланс");
        ui.label("Artificial Analysis · бесплатный личный ключ");
        ui.add(
            egui::TextEdit::singleline(&mut self.key_draft)
                .password(true)
                .hint_text("API-ключ")
                .desired_width(480.0),
        );
        ui.checkbox(&mut self.remember_key, "Сохранить защищённо в Windows");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.fetch.is_none() && !self.is_demo(),
                    egui::Button::new("Применить и загрузить"),
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
        ui.heading("Единый балл простого пикера");
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
                egui::Button::new(if self.is_demo() {
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
        egui::ComboBox::from_id_salt("detail-model")
            .width(ui.available_width() - 12.0)
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
            .spacing(vec2(24.0, 12.0))
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
            ui.selectable_value(&mut self.tab, Tab::Map, "Карта моделей");
            ui.selectable_value(&mut self.tab, Tab::Charts, "Бенчмарки");
            ui.selectable_value(&mut self.tab, Tab::Settings, "Данные и баланс");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.fetch.is_some() {
                    ui.spinner();
                }
                if ui
                    .add_enabled(
                        self.fetch.is_none()
                            && !self.is_demo()
                            && now() >= self.prefs.next_request_at,
                        egui::Button::new("Обновить"),
                    )
                    .clicked()
                {
                    self.refresh(ui.ctx());
                }
            });
        });
        ui.separator();
        if self.filters {
            self.filters_ui(ui);
            ui.separator();
        }
        if self.tab == Tab::Settings {
            self.settings_ui(ui);
            return;
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).size(12.0).color(WARNING));
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
            ui.selectable_value(&mut self.prefs.sort, SortBy::Price, "Сначала дешевле");
            ui.selectable_value(&mut self.prefs.sort, SortBy::Quality, "Сначала сильнее");
        });
        if self.prefs.price_mode == PriceMode::Blended {
            let mut input = self.prefs.input_share * 100.0;
            ui.add(
                egui::Slider::new(&mut input, 0.0..=100.0)
                    .suffix("%")
                    .text("доля входных токенов"),
            );
            self.prefs.input_share = input / 100.0;
        }
        let models: Vec<Model> = self
            .snapshot
            .as_ref()
            .map(|s| {
                ordered_models(&s.models, &self.prefs)
                    .into_iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        ui.label(
            RichText::new(format!(
                "{} моделей · все включённые уровни reasoning",
                models.len()
            ))
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            egui::Frame::new()
                .fill(SOFT)
                .inner_margin(16.0)
                .corner_radius(12.0)
                .show(&mut columns[0], |ui| {
                    self.detail(ui, &models);
                });
            if self.tab == Tab::Map {
                charts::scatter(&mut columns[1], &models, &mut self.prefs);
            } else {
                charts::bars(&mut columns[1], &models, &mut self.prefs);
            }
        });
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
                    .fill(Color32::WHITE)
                    .corner_radius(18.0)
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(232, 229, 238)))
                    .inner_margin(14.0),
            )
            .show(root, |ui| {
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
        assert!(texts.iter().any(|s| s.contains("Баланс")));
        assert!(
            max_bottom <= COMPACT.y,
            "text extends below widget: {max_bottom}"
        );
        assert!(app.prefs.selected.is_some());
    }
}
