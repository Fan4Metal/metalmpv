//! User interface pieces: the app icon as a texture, the update button, the
//! icon buttons of the panels, the close cross of a dialog, and the parts
//! of the window in modules of their own.

pub mod about;
pub mod assoc;
pub mod controls;
pub mod icons;
pub mod info;
pub mod osd;
pub mod picture;
pub mod playlist;
pub mod settings;
pub mod style;
pub mod title;
pub mod tracks;

use egui::{Response, TextureHandle, TextureOptions, Ui};

/// The app icon as a texture of exactly `points` at the display's pixel
/// density, so that it is never scaled (a scaled icon is blurred): an
/// embedded rendering when there is one of that size, else drawn now
/// (a millisecond or two). Kept in `cache`, one per place it is shown, and
/// replaced when the size in pixels changes.
pub fn app_icon(
    ctx: &egui::Context,
    points: f32,
    cache: &mut Option<TextureHandle>,
) -> TextureHandle {
    let px = (points * ctx.pixels_per_point()).round().max(1.0) as u32;
    let n = px as usize;
    if cache.as_ref().is_none_or(|t| t.size() != [n, n]) {
        let drawn;
        let rgba = match crate::embedded_icon(px) {
            Some(rgba) => rgba,
            None => {
                let start = std::time::Instant::now();
                drawn = crate::icon::rgba(px);
                log::debug!("icon drawn at {px} px in {:?}", start.elapsed());
                &drawn
            }
        };
        let image = egui::ColorImage::from_rgba_unmultiplied([n, n], rgba);
        *cache = Some(ctx.load_texture(format!("app_icon_{px}"), image, TextureOptions::LINEAR));
    }
    cache.clone().expect("set above")
}

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
