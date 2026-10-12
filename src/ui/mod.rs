//! User interface pieces: the app icon as a texture, the update button, the
//! close cross of a dialog, and the parts of the window in modules of their
//! own.

pub mod about;
pub mod controls;
pub mod icons;
pub mod osd;
pub mod playlist;
pub mod settings;
pub mod style;
pub mod title;
pub mod tracks;

use egui::{Response, TextureHandle, TextureOptions, Ui};

/// The app icon as a texture for `points` wide, from the smallest embedded
/// size (64, 128, 256 px) that covers it at the display's pixel density;
/// kept in `cache` and replaced when that density changes.
pub fn app_icon(
    ctx: &egui::Context,
    points: f32,
    cache: &mut Option<TextureHandle>,
) -> TextureHandle {
    let px = (points * ctx.pixels_per_point()).round() as u32;
    let size = [64u32, 128, 256]
        .into_iter()
        .find(|&s| s >= px)
        .unwrap_or(256);
    let n = size as usize;
    if cache.as_ref().is_none_or(|t| t.size() != [n, n]) {
        let image = egui::ColorImage::from_rgba_unmultiplied([n, n], crate::embedded_icon(size));
        let options = TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
        *cache = Some(ctx.load_texture(format!("app_icon_{size}"), image, options));
    }
    cache.clone().expect("set above")
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
