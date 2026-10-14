//! User interface pieces: the app icon as a texture, the update button, the
//! icon buttons of the panels, the close cross of a dialog, and the parts
//! of the window in modules of their own.

pub mod about;
pub mod assoc;
pub mod controls;
pub mod goto;
pub mod icons;
pub mod info;
pub mod licenses;
pub mod osd;
pub mod picture;
pub mod playlist;
pub mod popover;
pub mod settings;
pub mod style;
pub mod title;
pub mod tracks;
pub mod url;

use egui::{Response, TextureHandle, TextureOptions, Ui};

/// The app icon as a texture of exactly `points` at the display's pixel
/// density, so that it is never scaled (a scaled icon is blurred): an
/// embedded rendering when there is one of that size, else drawn now
/// (a millisecond or two; ~30 ms at 224 px). The embedded ones are blue,
/// another accent's are drawn. Kept in `cache`, one per place it is shown,
/// and replaced when the size in pixels or the accent changes.
pub fn app_icon(ctx: &egui::Context, points: f32, cache: &mut IconCache) -> TextureHandle {
    let px = (points * ctx.pixels_per_point()).round().max(1.0) as u32;
    let n = px as usize;
    let accent = style::accent_choice();
    // A colour dragged in the picker: the icon follows once it is let go
    // (a big one takes ~30 ms to draw).
    let dragging = ctx.input(|i| i.pointer.any_down());
    if cache
        .as_ref()
        .is_none_or(|(t, a)| t.size() != [n, n] || (*a != accent && !dragging))
    {
        let drawn;
        let embedded = crate::embedded_icon(px).filter(|_| accent == style::Accent::Blue);
        let rgba = match embedded {
            Some(rgba) => rgba,
            None => {
                let start = std::time::Instant::now();
                drawn = crate::icon::rgba_with(px, accent.palette().icon);
                log::debug!("icon drawn at {px} px in {:?}", start.elapsed());
                &drawn
            }
        };
        let image = egui::ColorImage::from_rgba_unmultiplied([n, n], rgba);
        let name = format!("app_icon_{px}_{}", accent.name());
        let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
        *cache = Some((texture, accent));
    }
    cache.as_ref().expect("set above").0.clone()
}

/// A modal's content size: `wanted`, or less in a window too small for
/// it, leaving `margin` points free around it, so its close buttons stay
/// on the screen.
pub fn modal_size(ctx: &egui::Context, wanted: egui::Vec2, margin: f32) -> egui::Vec2 {
    let room = ctx.content_rect().size() - egui::vec2(2.0 * margin, 2.0 * margin);
    wanted.min(room).max(egui::vec2(200.0, 160.0))
}

/// How many whole wheel notches turned this frame, up or down. A touchpad
/// or a free-spinning wheel sends many small deltas: they add up, and the
/// rest waits for the next frames (forgotten after a second of rest or
/// when the direction changes), so one swipe is not dozens of notches.
pub fn wheel_notches(ui: &Ui) -> i32 {
    /// Points of a pixel-precise delta per notch.
    const POINTS_PER_NOTCH: f32 = 50.0;
    let (delta, time) = ui.input(|i| {
        let delta: f32 = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                    egui::MouseWheelUnit::Point => delta.y / POINTS_PER_NOTCH,
                    egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
                }),
                _ => None,
            })
            .sum();
        (delta, i.time)
    });
    if delta == 0.0 || !delta.is_finite() {
        return 0;
    }
    let id = egui::Id::new("wheel_notches");
    ui.ctx().data_mut(|d| {
        let (rest, last) = d.get_temp_mut_or_default::<(f32, f64)>(id);
        if time - *last > 1.0 || (*rest != 0.0 && rest.signum() != delta.signum()) {
            *rest = 0.0;
        }
        *last = time;
        *rest += delta;
        let whole = rest.trunc();
        *rest -= whole;
        whole as i32
    })
}

/// The app icon of one place, with the accent it was drawn in.
pub type IconCache = Option<(TextureHandle, style::Accent)>;

/// Paint `icon` (from `app_icon`) pixel for pixel, centred on `centre` as
/// near as the pixel grid allows; its rectangle.
pub fn paint_icon(
    painter: &egui::Painter,
    icon: &TextureHandle,
    centre: egui::Pos2,
    tint: egui::Color32,
) -> egui::Rect {
    let ppp = painter.ctx().pixels_per_point();
    let size = egui::vec2(icon.size()[0] as f32, icon.size()[1] as f32) / ppp;
    let min = ((centre - size / 2.0) * ppp).round() / ppp;
    let rect = egui::Rect::from_min_size(min, size);
    painter.image(
        icon.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        tint,
    );
    rect
}

/// A newer release: its version with a download arrow in the link colour;
/// a click opens its release page. Shown in the title row only while
/// `Updates::newer` is `Some`.
pub fn update_button(ui: &mut Ui, tag: &str) -> Response {
    use egui::{Stroke, pos2, vec2};
    let version = crate::update::version_of(tag);
    let icon = ui.id().with("update_icon");
    let resp = egui::Button::new((egui::Atom::custom(icon, vec2(12.0, 14.0)), version))
        .frame(false)
        .atom_ui(ui);
    let response = resp.response.clone().on_hover_text(tr!(
        format!("Version {version} is available: open its release page"),
        format!("Доступна версия {version}: открыть страницу выпуска")
    ));
    if let Some(rect) = resp.rect(icon) {
        let stroke = Stroke::new(1.6, ui.visuals().hyperlink_color);
        let painter = ui.painter();
        let (cx, top, bottom) = (rect.center().x, rect.top() + 1.5, rect.bottom() - 1.5);
        let tip = bottom - 3.0;
        // Arrow down onto a tray.
        painter.line_segment([pos2(cx, top), pos2(cx, tip)], stroke);
        painter.line_segment([pos2(cx - 4.0, tip - 4.0), pos2(cx, tip)], stroke);
        painter.line_segment([pos2(cx + 4.0, tip - 4.0), pos2(cx, tip)], stroke);
        painter.line_segment(
            [
                pos2(rect.left() + 0.5, bottom),
                pos2(rect.right() - 0.5, bottom),
            ],
            stroke,
        );
    }
    response
}

/// A small button with no frame and an icon painted by `paint(painter,
/// centre, colour)`; its name is in the tooltip. Not focusable.
pub fn tool(
    ui: &mut Ui,
    tip: &str,
    enabled: bool,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, egui::Color32),
) -> Response {
    let sense = if enabled {
        egui::Sense::CLICK
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), sense);
    let color = if !enabled {
        style::TEXT_WEAK.gamma_multiply(0.5)
    } else if response.hovered() {
        style::TEXT_BRIGHT
    } else {
        style::TEXT
    };
    if enabled && response.hovered() {
        let fill = if response.is_pointer_button_down_on() {
            style::PRESSED
        } else {
            style::HOVER
        };
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    paint(ui.painter(), rect.center(), color);
    response.on_hover_text(tip)
}

/// A cross that closes a dialog or a panel; true when clicked.
pub fn close_cross(ui: &mut Ui) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::CLICK);
    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0, visuals.bg_fill);
    }
    let (c, r) = (rect.center(), 5.0);
    let stroke = egui::Stroke::new(1.5, visuals.fg_stroke.color);
    ui.painter()
        .line_segment([c + egui::vec2(-r, -r), c + egui::vec2(r, r)], stroke);
    ui.painter()
        .line_segment([c + egui::vec2(-r, r), c + egui::vec2(r, -r)], stroke);
    response.on_hover_text(tr!("Close", "Закрыть")).clicked()
}

/// A double click; a click right after one counts as another (egui
/// reports the third click as a triple click only).
pub fn double_clicked(response: &Response) -> bool {
    response.double_clicked() || response.triple_clicked()
}
