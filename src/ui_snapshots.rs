// Included inside app::tests: on-demand visual review without a native window.
mod svg_review {
    use super::*;
    use std::fmt::Write as _;

    fn escaped(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    fn color(value: egui::Color32) -> String {
        let [r, g, b, a] = value.to_srgba_unmultiplied();
        format!("rgba({r},{g},{b},{:.4})", a as f32 / 255.0)
    }

    fn base64(bytes: &[u8]) -> String {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        for chunk in bytes.chunks(3) {
            let value = ((chunk[0] as u32) << 16)
                | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
                | chunk.get(2).copied().unwrap_or(0) as u32;
            for index in 0..4 {
                result.push(if index > chunk.len() {
                    '='
                } else {
                    alphabet[((value >> (18 - index * 6)) & 63) as usize] as char
                });
            }
        }
        result
    }

    fn path_color(stroke: &egui::epaint::PathStroke) -> String {
        match &stroke.color {
            egui::epaint::ColorMode::Solid(value) => color(*value),
            _ => panic!("SVG review cannot render a procedural path color"),
        }
    }

    fn shape_svg(shape: &egui::Shape, svg: &mut String) {
        match shape {
            egui::Shape::Noop => {}
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    shape_svg(shape, svg);
                }
            }
            egui::Shape::Circle(circle) => {
                writeln!(
                    svg,
                    "<circle cx='{}' cy='{}' r='{}' fill='{}' stroke='{}' stroke-width='{}'/>",
                    circle.center.x,
                    circle.center.y,
                    circle.radius,
                    color(circle.fill),
                    color(circle.stroke.color),
                    circle.stroke.width
                )
                .unwrap();
            }
            egui::Shape::Ellipse(ellipse) => {
                writeln!(svg, "<ellipse cx='{}' cy='{}' rx='{}' ry='{}' fill='{}' stroke='{}' stroke-width='{}'/>", ellipse.center.x, ellipse.center.y, ellipse.radius.x, ellipse.radius.y, color(ellipse.fill), color(ellipse.stroke.color), ellipse.stroke.width).unwrap();
            }
            egui::Shape::LineSegment { points, stroke } => {
                writeln!(
                    svg,
                    "<path d='M {} {} L {} {}' fill='none' stroke='{}' stroke-width='{}'/>",
                    points[0].x,
                    points[0].y,
                    points[1].x,
                    points[1].y,
                    color(stroke.color),
                    stroke.width
                )
                .unwrap();
            }
            egui::Shape::Rect(rectangle) => {
                assert!(
                    rectangle.brush.is_none(),
                    "Textured rects need a raster-aware exporter"
                );
                let offset = match rectangle.stroke_kind {
                    egui::StrokeKind::Inside => rectangle.stroke.width * 0.5,
                    egui::StrokeKind::Middle => 0.0,
                    egui::StrokeKind::Outside => -rectangle.stroke.width * 0.5,
                };
                let rect = rectangle.rect.shrink(offset);
                let radius = rectangle.corner_radius;
                let nw = (f32::from(radius.nw) - offset)
                    .max(0.0)
                    .min(rect.width().min(rect.height()) / 2.0);
                let ne = (f32::from(radius.ne) - offset)
                    .max(0.0)
                    .min(rect.width().min(rect.height()) / 2.0);
                let sw = (f32::from(radius.sw) - offset)
                    .max(0.0)
                    .min(rect.width().min(rect.height()) / 2.0);
                let se = (f32::from(radius.se) - offset)
                    .max(0.0)
                    .min(rect.width().min(rect.height()) / 2.0);
                let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
                writeln!(svg, "<path transform='rotate({} {} {})' d='M {} {t} H {} Q {r} {t} {r} {} V {} Q {r} {b} {} {b} H {} Q {l} {b} {l} {} V {} Q {l} {t} {} {t} Z' fill='{}' stroke='{}' stroke-width='{}'/>", rectangle.angle.to_degrees(), rect.center().x, rect.center().y, l+nw, r-ne, t+ne, b-se, r-se, l+sw, b-sw, t+nw, l+nw, color(rectangle.fill), color(rectangle.stroke.color), rectangle.stroke.width).unwrap();
            }
            egui::Shape::Path(path) => {
                let mut data = String::new();
                for (index, point) in path.points.iter().enumerate() {
                    write!(
                        data,
                        "{} {} {} ",
                        if index == 0 { "M" } else { "L" },
                        point.x,
                        point.y
                    )
                    .unwrap();
                }
                if path.closed {
                    data.push('Z');
                }
                writeln!(svg, "<path d='{data}' fill='{}' stroke='{}' stroke-width='{}' stroke-linejoin='round'/>", if path.closed { color(path.fill) } else { "none".into() }, path_color(&path.stroke), path.stroke.width).unwrap();
            }
            egui::Shape::Text(text) => {
                writeln!(
                    svg,
                    "<g transform='translate({} {}) rotate({})' opacity='{}'>",
                    text.pos.x,
                    text.pos.y,
                    text.angle.to_degrees(),
                    text.opacity_factor
                )
                .unwrap();
                let mut byte_index = egui::text::ByteIndex(0);
                for row in &text.galley.rows {
                    for glyph in &row.glyphs {
                        let section = text
                            .galley
                            .job
                            .sections
                            .iter()
                            .find(|section| section.byte_range.contains(&byte_index))
                            .unwrap_or(&text.galley.job.sections[0]);
                        let vertex_color = row
                            .visuals
                            .mesh
                            .vertices
                            .get(glyph.first_vertex as usize)
                            .map(|vertex| vertex.color)
                            .unwrap_or(section.format.color);
                        let fill = text.override_text_color.unwrap_or(
                            if vertex_color == egui::Color32::PLACEHOLDER {
                                text.fallback_color
                            } else {
                                vertex_color
                            },
                        );
                        let family = match section.format.font_id.family {
                            egui::FontFamily::Monospace => "Hack",
                            _ => "Ubuntu-Light",
                        };
                        writeln!(svg, "<text x='{}' y='{}' font-family='{},NotoEmoji-Regular,sans-serif' font-size='{}' fill='{}' xml:space='preserve'>{}</text>", row.pos.x + glyph.pos.x, row.pos.y + glyph.pos.y, family, section.format.font_id.size, color(fill), escaped(&glyph.chr.to_string())).unwrap();
                        byte_index += glyph.chr.len_utf8();
                    }
                    if row.ends_with_newline {
                        byte_index += 1;
                    }
                }
                svg.push_str("</g>\n");
            }
            egui::Shape::QuadraticBezier(curve) => {
                let p = curve.points;
                writeln!(
                    svg,
                    "<path d='M {} {} Q {} {} {} {} {}' fill='{}' stroke='{}' stroke-width='{}'/>",
                    p[0].x,
                    p[0].y,
                    p[1].x,
                    p[1].y,
                    p[2].x,
                    p[2].y,
                    if curve.closed { "Z" } else { "" },
                    color(curve.fill),
                    path_color(&curve.stroke),
                    curve.stroke.width
                )
                .unwrap();
            }
            egui::Shape::CubicBezier(curve) => {
                let p = curve.points;
                writeln!(svg,"<path d='M {} {} C {} {} {} {} {} {} {}' fill='{}' stroke='{}' stroke-width='{}'/>",p[0].x,p[0].y,p[1].x,p[1].y,p[2].x,p[2].y,p[3].x,p[3].y,if curve.closed {"Z"} else {""},color(curve.fill),path_color(&curve.stroke),curve.stroke.width).unwrap();
            }
            egui::Shape::Mesh(mesh) => {
                for indices in mesh.indices.chunks_exact(3) {
                    let vertices = [
                        mesh.vertices[indices[0] as usize],
                        mesh.vertices[indices[1] as usize],
                        mesh.vertices[indices[2] as usize],
                    ];
                    assert!(
                        vertices
                            .iter()
                            .all(|vertex| vertex.uv == egui::epaint::WHITE_UV),
                        "Textured mesh needs raster-aware export"
                    );
                    writeln!(
                        svg,
                        "<polygon points='{},{} {},{} {},{}' fill='{}'/>",
                        vertices[0].pos.x,
                        vertices[0].pos.y,
                        vertices[1].pos.x,
                        vertices[1].pos.y,
                        vertices[2].pos.x,
                        vertices[2].pos.y,
                        color(vertices[0].color)
                    )
                    .unwrap();
                }
            }
            egui::Shape::Callback(_) => panic!("Native callback cannot be represented in SVG"),
        }
    }

    #[test]
    fn narrow_picker_keeps_thumb_axes_and_metrics_inside_visible_clip() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = PickerApp::load(Store::new(temp.path().into()).unwrap(), true, false);
        app.expanded = true;
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let mut output = egui::FullOutput::default();
        for frame in 0..3 {
            output.textures_delta.clear();
            output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(780.0, 580.0),
                    )),
                    time: Some(frame as f64),
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
        }
        output.textures_delta.clear();
        let mut found = 0;
        for clipped in &output.shapes {
            let bounds = match &clipped.shape {
                egui::Shape::Circle(circle)
                    if circle.fill == theme::CANVAS
                        && circle.radius == theme::PICKER_THUMB_RADIUS =>
                {
                    found += 1;
                    Some(egui::Rect::from_center_size(
                        circle.center,
                        vec2(circle.radius * 2.0, circle.radius * 2.0),
                    ))
                }
                egui::Shape::Text(text)
                    if ["Дешевле", "Дороже →", "Задача AA"].contains(&text.galley.text()) =>
                {
                    found += 1;
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                }
                _ => None,
            };
            if let Some(bounds) = bounds {
                assert!(
                    clipped.clip_rect.expand(0.5).contains_rect(bounds),
                    "essential picker element {:?} escapes visible clip {:?}",
                    bounds,
                    clipped.clip_rect
                );
            }
        }
        assert_eq!(
            found, 4,
            "thumb, both axes and final model metric must render"
        );
    }

    #[test]
    #[ignore = "On-demand headless visual review: writes SVG artifacts"]
    fn export_design_review_snapshots() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts/design-review");
        std::fs::create_dir_all(&directory).unwrap();
        let mut fonts = String::new();
        for (name, data) in egui::FontDefinitions::default().font_data {
            write!(
                fonts,
                "@font-face{{font-family:'{}';src:url(data:font/ttf;base64,{})}}",
                escaped(&name),
                base64(&data.font)
            )
            .unwrap();
        }
        for (name, size, expanded, filters, tab) in [
            ("compact", COMPACT, false, false, Tab::Map),
            ("picker", EXPANDED, true, false, Tab::Map),
            ("benchmarks", EXPANDED, true, false, Tab::Charts),
            ("settings", EXPANDED, true, false, Tab::Settings),
            ("filters", FILTER, false, true, Tab::Map),
            ("picker-narrow", vec2(780.0, 580.0), true, false, Tab::Map),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let mut app = PickerApp::load(Store::new(temp.path().into()).unwrap(), true, false);
            app.expanded = expanded;
            app.filters = filters;
            app.tab = tab;
            let ctx = egui::Context::default();
            configure_style(&ctx);
            let mut output = egui::FullOutput::default();
            for frame in 0..3 {
                output.textures_delta.clear();
                output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(frame as f64),
                        ..Default::default()
                    },
                    |ui| app.render(ui),
                );
            }
            output.textures_delta.clear();
            let mut svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}' viewBox='0 0 {} {}'><style>{fonts}</style>\n",
                size.x, size.y, size.x, size.y
            );
            for (index, clipped) in output.shapes.iter().enumerate() {
                let clip = clipped
                    .clip_rect
                    .intersect(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
                if !clip.is_positive() {
                    continue;
                }
                writeln!(svg,"<defs><clipPath id='c{index}'><rect x='{}' y='{}' width='{}' height='{}'/></clipPath></defs><g clip-path='url(#c{index})'>",clip.left(),clip.top(),clip.width(),clip.height()).unwrap();
                shape_svg(&clipped.shape, &mut svg);
                svg.push_str("</g>\n");
            }
            svg.push_str("</svg>\n");
            let path = directory.join(format!("{name}.svg"));
            std::fs::write(&path, svg).unwrap();
            println!("{}", path.display());
        }
    }
}
