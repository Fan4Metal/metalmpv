//! The title row of the frameless window: the icon, the file's name in the
//! middle, minimise, maximise and close on the right. Its free area drags
//! the window; a double click maximises it or brings it back.

use egui::{Align2, FontId, Rect, Sense, TextureHandle, Ui, pos2};

use super::{icons, style};
use crate::input::Cmd;

const BUTTON: f32 = 46.0;

/// `update` is the tag of a newer release, shown as a button left of the
/// window's buttons.
pub fn row(
    ui: &mut Ui,
    title: &str,
    maximized: bool,
    icon: &TextureHandle,
    update: Option<&str>,
) -> Option<Cmd> {
    let rect = ui.max_rect();
    // First, under the buttons: what they leave drags the window.
    let drag = crate::frameless::drag_area(ui, rect, ui.id().with("title_drag"), Sense::CLICK);
    let mut cmd = None;
    if crate::ui::double_clicked(&drag) {
        cmd = Some(Cmd::ToggleMaximize);
    }
    super::paint_icon(
        ui.painter(),
        icon,
        pos2(rect.left() + 18.0, rect.center().y),
        egui::Color32::WHITE,
    );

    let buttons = [
        (Cmd::Close, tr!("Close", "Закрыть")),
        (
            Cmd::ToggleMaximize,
            if maximized {
                tr!("Restore down", "Свернуть в окно")
            } else {
                tr!("Maximize", "Развернуть")
            },
        ),
        (Cmd::Minimize, tr!("Minimize", "Свернуть")),
        (
            Cmd::About,
            tr!("About MetalMPV (F1)", "О программе MetalMPV (F1)"),
        ),
    ];
    let mut right = rect.right();
    for (kind, tip) in buttons {
        let r = Rect::from_min_max(pos2(right - BUTTON, rect.top()), pos2(right, rect.bottom()));
        // The About button is narrower than the window's own.
        let r = if kind == Cmd::About {
            Rect::from_min_max(pos2(r.right() - 34.0, r.top()), r.max)
        } else {
            r
        };
        right = r.left();
        let response = ui.interact(r, ui.id().with(("title_button", tip)), Sense::CLICK);
        let hovered = response.hovered();
        if hovered {
            let fill = if kind == Cmd::Close {
                style::CLOSE_HOVER
            } else {
                style::HOVER
            };
            ui.painter().rect_filled(r, 0.0, fill);
        }
        let color = if hovered {
            style::TEXT_BRIGHT
        } else {
            style::TEXT
        };
        let o = r.center();
        match kind {
            Cmd::Close => icons::close(ui.painter(), o, 5.0, color),
            Cmd::ToggleMaximize => icons::maximize(ui.painter(), o, 5.0, color, maximized),
            Cmd::About => icons::info(ui.painter(), o, 6.5, color),
            _ => icons::minimize(ui.painter(), o, 5.0, color),
        }
        if response.on_hover_text(tip).clicked() {
            cmd = Some(kind);
        }
    }

    if let Some(tag) = update {
        let r = Rect::from_min_max(
            pos2(right - 90.0, rect.top()),
            pos2(right - 4.0, rect.bottom()),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(r)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        if super::update_button(&mut child, tag).clicked() {
            crate::win::shell_open(crate::update::release_url(tag));
        }
        right = child.min_rect().left() - 4.0;
    }

    // The name, centred on the window and cut to the room between the
    // icon and the buttons.
    let room = 2.0 * (rect.center().x - (rect.left() + 36.0)).min(right - 8.0 - rect.center().x);
    if room > 40.0 {
        let galley = ui.fonts_mut(|f| {
            let mut job = egui::text::LayoutJob::simple_singleline(
                title.to_owned(),
                FontId::proportional(12.5),
                style::TEXT,
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(room);
            f.layout_job(job)
        });
        let pos = Align2::CENTER_CENTER
            .anchor_size(rect.center(), galley.size())
            .min;
        ui.painter().galley(pos, galley, style::TEXT);
    }
    cmd
}
