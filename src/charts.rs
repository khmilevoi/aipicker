use aipicker::domain::{Model, Preferences};
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

pub const ACCENT: Color32 = Color32::from_rgb(153, 112, 222);
pub const MINT: Color32 = Color32::from_rgb(130, 94, 192);
pub const CORAL: Color32 = Color32::from_rgb(206, 134, 98);
pub const MUTED: Color32 = Color32::from_rgb(130, 125, 140);
pub fn color(model: &Model) -> Color32 {
    if model.provider == "openai" {
        MINT
    } else {
        CORAL
    }
}
pub fn number(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.1}"))
        .unwrap_or_else(|| "Нет данных".into())
}
pub fn money(value: Option<f64>) -> String {
    value
        .map(|v| format!("${v:.2}"))
        .unwrap_or_else(|| "Нет данных".into())
}

pub enum ControlIcon {
    Filter,
    Expand,
    Collapse,
    Close,
}

pub fn model_title(title: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let split = title
        .rsplit_once(" (")
        .and_then(|(base, suffix)| suffix.strip_suffix(')').map(|level| (base, level)));
    let (base, level) = split
        .map(|(base, level)| (base, Some(level)))
        .unwrap_or((title, None));
    job.append(
        base,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(17.0),
            color: Color32::from_rgb(42, 38, 52),
            ..Default::default()
        },
    );
    if let Some(level) = level {
        let label = match level.to_lowercase().as_str() {
            "low" => "Низкое",
            "medium" => "Среднее",
            "high" => "Высокое",
            "xhigh" => "Максимум",
            "minimal" => "Минимум",
            _ => level,
        };
        job.append(
            &format!(" {label}"),
            0.0,
            egui::TextFormat {
                font_id: FontId::proportional(16.0),
                color: MUTED,
                ..Default::default()
            },
        );
    }
    job
}

pub fn icon_button(ui: &mut Ui, icon: ControlIcon, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 28.0), Sense::click());
    let p = ui.painter();
    if response.hovered() {
        p.rect_filled(rect, 6.0, Color32::from_rgb(244, 239, 250));
    }
    let c = rect.center();
    let stroke = Stroke::new(1.5, if response.hovered() { ACCENT } else { MUTED });
    match icon {
        ControlIcon::Filter => {
            for (y, x) in [(-6.0, -3.0), (0.0, 4.0), (6.0, -1.0)] {
                p.line_segment([c + vec2(-8.0, y), c + vec2(8.0, y)], stroke);
                p.circle_filled(c + vec2(x, y), 2.5, Color32::WHITE);
                p.circle_stroke(c + vec2(x, y), 2.5, stroke);
            }
        }
        ControlIcon::Expand => {
            p.line_segment([c + vec2(-2.5, -5.0), c + vec2(2.5, 0.0)], stroke);
            p.line_segment([c + vec2(2.5, 0.0), c + vec2(-2.5, 5.0)], stroke);
        }
        ControlIcon::Collapse => {
            p.line_segment([c + vec2(2.5, -5.0), c + vec2(-2.5, 0.0)], stroke);
            p.line_segment([c + vec2(-2.5, 0.0), c + vec2(2.5, 5.0)], stroke);
        }
        ControlIcon::Close => {
            p.line_segment([c - vec2(4.0, 4.0), c + vec2(4.0, 4.0)], stroke);
            p.line_segment([c + vec2(-4.0, 4.0), c + vec2(4.0, -4.0)], stroke);
        }
    }
    response.on_hover_text(hint)
}

pub fn simple_rail(ui: &mut Ui, models: &[Model], selected: &mut Option<String>) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 32.0));
    let start = track.left() + 18.0;
    let end = track.right() - 18.0;
    let count = models.len();
    let mut current = models
        .iter()
        .position(|m| Some(&m.id) == selected.as_ref())
        .unwrap_or(0);
    let index_at = |x: f32| {
        if count <= 1 {
            0
        } else {
            (((x - start) / (end - start) * (count - 1) as f32)
                .round()
                .max(0.0) as usize)
                .min(count - 1)
        }
    };
    if !models.is_empty() {
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
        {
            current = index_at(pos.x);
            *selected = Some(models[current].id.clone());
            response.request_focus();
        }
        if response.has_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                current = current.saturating_sub(1);
                *selected = Some(models[current].id.clone());
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                current = (current + 1).min(count - 1);
                *selected = Some(models[current].id.clone());
            }
        }
    }
    let painter = ui.painter();
    painter.rect_filled(track, 16.0, Color32::from_rgb(238, 234, 245));
    if count == 0 {
        return;
    }
    let x_for = |i: usize| {
        if count == 1 {
            track.center().x
        } else {
            start + (end - start) * i as f32 / (count - 1) as f32
        }
    };
    let x = x_for(current);
    painter.rect_filled(
        Rect::from_min_max(
            track.min,
            pos2((x + 15.0).min(track.right()), track.bottom()),
        ),
        16.0,
        Color32::from_rgb(173, 133, 235),
    );
    for i in 0..count {
        let dot = pos2(x_for(i), track.center().y);
        painter.circle_filled(
            dot,
            if count > 30 { 1.2 } else { 2.5 },
            if i <= current {
                Color32::from_rgb(221, 200, 252)
            } else {
                Color32::from_rgb(192, 183, 208)
            },
        );
    }
    let thumb = pos2(x, track.center().y);
    painter.circle_filled(thumb + vec2(0.0, 1.5), 17.0, Color32::from_black_alpha(14));
    painter.circle_filled(thumb, 16.5, Color32::WHITE);
    painter.circle_stroke(
        thumb,
        16.5,
        Stroke::new(0.7, Color32::from_rgb(235, 228, 246)),
    );
    if response.has_focus() {
        painter.rect_stroke(rect, 18.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
    }
    if let Some(pos) = response.hover_pos() {
        response.on_hover_text(&models[index_at(pos.x)].name);
    }
}

pub fn scatter(ui: &mut Ui, models: &[Model], prefs: &mut Preferences) {
    ui.horizontal(|ui| {
        ui.heading("Цена и качество");
        ui.checkbox(&mut prefs.logarithmic, "Лог. шкала цены");
    });
    ui.label(
        egui::RichText::new(
            "Левее — дешевле · выше — лучше по выбранному индексу · нажмите на точку",
        )
        .color(MUTED),
    );
    let points: Vec<_> = models
        .iter()
        .filter_map(|m| {
            Some((
                m,
                m.price(prefs.price_mode, prefs.input_share)?,
                m.score(prefs.metric)?,
            ))
        })
        .collect();
    if points.is_empty() {
        ui.add_space(25.0);
        ui.label("Для графика нужны одновременно цена и оценка. Выберите другой индекс или включите модели в фильтре.");
        return;
    }
    let transform = |v: f64| if prefs.logarithmic { v.ln_1p() } else { v };
    let max_x = points
        .iter()
        .map(|(_, x, _)| transform(*x))
        .fold(0.0f64, f64::max)
        .max(0.1)
        * 1.12;
    let min_y = points.iter().map(|(_, _, y)| *y).fold(0.0f64, f64::min);
    let max_y = points.iter().map(|(_, _, y)| *y).fold(1.0f64, f64::max) * 1.1;
    let height = (ui.available_height() - 48.0).clamp(220.0, 440.0);
    let (outer, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let plot = Rect::from_min_max(outer.min + vec2(46.0, 18.0), outer.max - vec2(18.0, 38.0));
    let painter = ui.painter();
    painter.rect_filled(plot, 8.0, Color32::from_rgb(250, 248, 253));
    for tick in 0..=4 {
        let fraction = tick as f32 / 4.0;
        let x = plot.left() + fraction * plot.width();
        let y = plot.bottom() - fraction * plot.height();
        let grid = Stroke::new(1.0, Color32::from_rgb(234, 230, 241));
        painter.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], grid);
        painter.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], grid);
        let raw_x = max_x * fraction as f64;
        let cost = if prefs.logarithmic {
            raw_x.exp_m1()
        } else {
            raw_x
        };
        painter.text(
            pos2(x, plot.bottom() + 14.0),
            Align2::CENTER_CENTER,
            format!("${cost:.1}"),
            FontId::proportional(11.0),
            MUTED,
        );
        painter.text(
            pos2(plot.left() - 9.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.0}", min_y + (max_y - min_y) * fraction as f64),
            FontId::proportional(11.0),
            MUTED,
        );
    }
    let mut nearest: Option<(f32, &Model, f64, f64)> = None;
    for (model, price, score) in &points {
        let point = pos2(
            plot.left() + (transform(*price) / max_x) as f32 * plot.width(),
            plot.bottom() - ((*score - min_y) / (max_y - min_y)) as f32 * plot.height(),
        );
        let selected = prefs.selected.as_ref() == Some(&model.id);
        painter.circle_filled(point, if selected { 6.5 } else { 4.5 }, color(model));
        if selected {
            painter.circle_stroke(point, 11.0, Stroke::new(1.5, color(model)));
            let anchor = if point.x > plot.center().x {
                Align2::RIGHT_BOTTOM
            } else {
                Align2::LEFT_BOTTOM
            };
            let offset = if point.x > plot.center().x {
                -10.0
            } else {
                10.0
            };
            painter.text(
                point + vec2(offset, -10.0),
                anchor,
                &model.name,
                FontId::proportional(12.0),
                color(model),
            );
        }
        if let Some(pointer) = response.hover_pos() {
            let distance = point.distance(pointer);
            if distance < 18.0 && nearest.as_ref().is_none_or(|n| distance < n.0) {
                nearest = Some((distance, model, *price, *score));
            }
        }
    }
    if let Some((_, model, price, score)) = nearest {
        if response.clicked() {
            prefs.selected = Some(model.id.clone());
        }
        response.on_hover_text(format!(
            "{}\n{}: {:.1}\n${:.2} · {}",
            model.name,
            prefs.metric.label(),
            score,
            price,
            prefs.price_mode.unit()
        ));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.label(
        egui::RichText::new(format!(
            "{} · {} · без пары цена/оценка: {}",
            prefs.price_mode.unit(),
            prefs.price_mode.label(),
            models.len() - points.len()
        ))
        .size(11.0)
        .color(MUTED),
    );
}

pub fn bars(ui: &mut Ui, models: &[Model], prefs: &mut Preferences) {
    ui.heading(format!("{} · сравнение моделей", prefs.metric.label()));
    ui.label(
        egui::RichText::new(
            "Баллы индекса, не процент правильных ответов. Нажмите строку для выбора.",
        )
        .color(MUTED),
    );
    let mut rows: Vec<_> = models.iter().collect();
    rows.sort_by(|a, b| {
        b.score(prefs.metric)
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.score(prefs.metric).unwrap_or(f64::NEG_INFINITY))
    });
    let max = rows
        .iter()
        .filter_map(|m| m.score(prefs.metric))
        .fold(1.0f64, f64::max);
    egui::ScrollArea::vertical()
        .id_salt("benchmark-bars")
        .max_height(390.0)
        .show(ui, |ui| {
            for model in rows {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
                let selected = prefs.selected.as_ref() == Some(&model.id);
                let painter = ui.painter();
                if selected {
                    painter.rect_stroke(
                        rect.shrink(1.0),
                        6.0,
                        Stroke::new(1.0, color(model)),
                        StrokeKind::Inside,
                    );
                }
                let name_width = (rect.width() * 0.42).clamp(100.0, 280.0);
                let short: String = model.name.chars().take(34).collect();
                painter.text(
                    rect.left_center() + vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    short,
                    FontId::proportional(12.0),
                    Color32::from_rgb(44, 39, 57),
                );
                if let Some(score) = model.score(prefs.metric) {
                    let start = pos2(rect.left() + name_width, rect.top() + 13.0);
                    let width =
                        (rect.width() - name_width - 80.0).max(0.0) * (score / max).max(0.0) as f32;
                    painter.rect_filled(
                        Rect::from_min_size(start, vec2(width, 16.0)),
                        4.0,
                        color(model),
                    );
                }
                painter.text(
                    rect.right_center() - vec2(8.0, 0.0),
                    Align2::RIGHT_CENTER,
                    number(model.score(prefs.metric)),
                    FontId::proportional(12.0),
                    MUTED,
                );
                if response.clicked() {
                    prefs.selected = Some(model.id.clone());
                }
                response.on_hover_text(&model.name);
            }
        });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Бесплатный API предоставляет сводные индексы. Результаты отдельных тестов и исходные графики сайта не включены; здесь графики построены по загруженным баллам.").size(12.0).color(MUTED));
}
