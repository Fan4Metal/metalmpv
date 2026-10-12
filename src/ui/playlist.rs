//! The playlist panel on the right: a row of icon buttons (add, remove,
//! sort, clear; their names in the tooltips) and the list; a click selects
//! an entry, a double click plays it.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Ui, vec2};

use super::{icons, style};
use crate::input::Cmd;
use crate::playlist::Playlist;

const ROW: f32 = 22.0;

pub fn panel(ui: &mut Ui, playlist: &mut Playlist) -> Vec<Cmd> {
    let mut out = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let s = 5.5;
        let add = tr!("Add files to the list", "Добавить файлы в список");
        if tool(ui, add, true, |p, o, c| icons::plus(p, o, s, c)).clicked() {
            out.push(Cmd::PlaylistAdd);
        }
        let selected = playlist.selected.filter(|i| *i < playlist.len());
        let remove = tr!(
            "Remove the selected entry (Delete)",
            "Убрать выбранный файл (Delete)"
        );
        if tool(ui, remove, selected.is_some(), |p, o, c| {
            icons::minus(p, o, s, c)
        })
        .clicked()
            && let Some(i) = selected
        {
            out.push(Cmd::PlaylistRemove(i));
        }
        let sort = tr!("Sort by name", "Упорядочить по имени");
        if tool(ui, sort, playlist.len() > 1, |p, o, c| {
            icons::sort(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistSort);
        }
        let clear = tr!("Clear the list", "Очистить список");
        if tool(ui, clear, !playlist.is_empty(), |p, o, c| {
            icons::trash(p, o, 7.0, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistClear);
        }
    });
    ui.add_space(2.0);
    let frame = egui::Frame::new()
        .fill(style::LIST_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(2));
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        if playlist.is_empty() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.weak(tr!(
                    "Drop files here or use Add",
                    "Перетащите файлы сюда или нажмите «Добавить»"
                ));
            });
            return;
        }
        let total = playlist.len();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW, total, |ui, range| {
                for i in range {
                    let path = &playlist.items()[i];
                    let name = path
                        .file_name()
                        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy());
                    let current = playlist.current() == Some(i);
                    let selected = playlist.selected == Some(i);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if selected {
                        ui.painter().rect_filled(rect, 2.0, style::SELECTED_BG);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 2.0, style::HOVER);
                    }
                    let color = if current { style::CURRENT } else { style::TEXT };
                    let number = format!("{}.", i + 1);
                    let painter = ui.painter_at(rect.shrink2(vec2(4.0, 0.0)));
                    let font = egui::FontId::proportional(13.0);
                    let n = painter.text(
                        rect.left_center() + vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        number,
                        font.clone(),
                        style::TEXT_WEAK,
                    );
                    let text_left = n.right() + 6.0;
                    let galley = ui.fonts_mut(|f| {
                        let mut job = egui::text::LayoutJob::simple_singleline(
                            name.into_owned(),
                            font,
                            color,
                        );
                        job.wrap = egui::text::TextWrapping::truncate_at_width(
                            rect.right() - 6.0 - text_left,
                        );
                        f.layout_job(job)
                    });
                    painter.galley(
                        egui::pos2(text_left, rect.center().y - galley.size().y / 2.0),
                        galley,
                        color,
                    );
                    let response = response.on_hover_text(path.display().to_string());
                    if crate::ui::double_clicked(&response) {
                        out.push(Cmd::PlayIndex(i));
                    } else if response.clicked() {
                        playlist.selected = Some(i);
                    }
                }
            });
    });
    out
}

/// A small button with no frame and an icon painted by `paint(painter,
/// centre, colour)`; its name is in the tooltip. Not focusable.
fn tool(
    ui: &mut Ui,
    tip: &str,
    enabled: bool,
    paint: impl FnOnce(&Painter, Pos2, Color32),
) -> Response {
    let sense = if enabled {
        Sense::CLICK
    } else {
        Sense::hover()
    };
    let (rect, response): (Rect, Response) = ui.allocate_exact_size(vec2(26.0, 24.0), sense);
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
