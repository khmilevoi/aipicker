use eframe::egui;
use std::sync::mpsc::{self, Receiver};
use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

pub enum Event {
    Show(Option<(f64, f64)>),
    Quit,
}
pub struct Tray {
    pub events: Receiver<Event>,
    _icon: TrayIcon,
}

pub fn icon_rgba() -> Vec<u8> {
    let mut pixels = vec![0; 32 * 32 * 4];
    for y in 0..32usize {
        for x in 0..32usize {
            let i = (y * 32 + x) * 4;
            let background = (3..29).contains(&x) && (3..29).contains(&y);
            let mark = ((8..12).contains(&x) && (17..24).contains(&y))
                || ((14..18).contains(&x) && (12..24).contains(&y))
                || ((20..24).contains(&x) && (7..24).contains(&y));
            pixels[i..i + 4].copy_from_slice(if mark {
                &[176, 141, 234, 255]
            } else if background {
                &[22, 31, 43, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    pixels
}

impl Tray {
    pub fn new(ctx: &egui::Context) -> Result<Self, String> {
        let menu = Menu::new();
        let show = MenuItem::new("Открыть AI Picker", true, None);
        let quit = MenuItem::new("Выход", true, None);
        menu.append_items(&[&show, &quit])
            .map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_tooltip("AI Picker · модели, цены, бенчмарки")
            .with_icon(Icon::from_rgba(icon_rgba(), 32, 32).map_err(|e| e.to_string())?)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        let (tx, events) = mpsc::channel();
        let tray_tx = tx.clone();
        let wake = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                let _ = tray_tx.send(Event::Show(Some((position.x, position.y))));
                wake.request_repaint();
            }
        }));
        let show_id = show.id().clone();
        let quit_id = quit.id().clone();
        let wake = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == show_id {
                let _ = tx.send(Event::Show(None));
            }
            if event.id == quit_id {
                let _ = tx.send(Event::Quit);
            }
            wake.request_repaint();
        }));
        Ok(Self {
            events,
            _icon: icon,
        })
    }
}

pub fn position_near(ctx: &egui::Context, location: (f64, f64), size: egui::Vec2) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::POINT,
            Graphics::Gdi::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            },
        };
        let monitor = MonitorFromPoint(
            POINT {
                x: location.0 as i32,
                y: location.1 as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            let scale = ctx.pixels_per_point();
            let work = info.rcWork;
            let left = work.left as f32 / scale + 8.0;
            let top = work.top as f32 / scale + 8.0;
            let right = work.right as f32 / scale - size.x - 8.0;
            let bottom = work.bottom as f32 / scale - size.y - 40.0;
            let x = (location.0 as f32 / scale - size.x).clamp(left, right.max(left));
            let y = (location.1 as f32 / scale - size.y - 40.0).clamp(top, bottom.max(top));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }
    }
}
