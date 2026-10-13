//! The playlist panel on the right, or in a window of its own: a row of
//! icon buttons (add, remove, sort, clear, save as M3U8; after a line,
//! shuffle and the repeat mode; on the right the button that moves the
//! list into a window of its own or back; their names in the tooltips)
//! and the list; a click selects
//! an entry, a double click plays it. Under it, when the playing file has
//! chapters, a section of them that folds: a click goes to a chapter.

use egui::{Align2, FontId, Response, Sense, Ui, pos2, vec2};

use super::{icons, style, tool};
use crate::chapters::{self, Chapter};
use crate::input::Cmd;
use crate::logos::Logos;
use crate::playlist::{Playlist, Repeat};

const ROW: f32 = 22.0;
/// The narrowest the panel or its window may be, in points, so that the
/// buttons do not run into each other: eight buttons of 26, the line
/// between the groups with its margins (12), the gaps (2 each), a gap
/// before the button on the right and the panel's margins (6 a side).
pub const MIN_WIDTH: f32 = 260.0;
/// The height of the chapters' header.
const HEADER: f32 = 26.0;

/// The panel; `chapters` and `time` are the playing file's; `logos`, when
/// they are fetched, pictures the channels of an IPTV list; `detached`,
/// it is in a window of its own.
pub fn panel(
    ui: &mut Ui,
    playlist: &mut Playlist,
    logos: Option<&mut Logos>,
    repeat: Repeat,
    chapters: &[Chapter],
    time: Option<f64>,
    detached: bool,
) -> Vec<Cmd> {
    let mut out = Vec::new();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let s = 5.5;
        let add = tr!("Add files to the list", "Добавить файлы в плейлист");
        if tool(ui, add, true, |p, o, c| icons::plus(p, o, s, c)).clicked() {
            out.push(Cmd::PlaylistAdd);
        }
        let selected = playlist.selected.filter(|i| *i < playlist.len());
        let remove = crate::keymap::with_key(
            tr!("Remove the selected entry", "Убрать выбранный файл"),
            Cmd::Delete,
        );
        if tool(ui, &remove, selected.is_some(), |p, o, c| {
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
        let clear = tr!("Clear the list", "Очистить плейлист");
        if tool(ui, clear, !playlist.is_empty(), |p, o, c| {
            icons::trash(p, o, 7.0, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistClear);
        }
        let save = tr!("Save the list as M3U8…", "Сохранить плейлист в M3U8…");
        if tool(ui, save, !playlist.is_empty(), |p, o, c| {
            icons::save(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistSave);
        }
        // The order of play, apart from what acts on the list.
        ui.add_space(3.0);
        ui.separator();
        ui.add_space(3.0);
        let shuffle = tr!("Shuffle the list", "Перемешать плейлист");
        if tool(ui, shuffle, playlist.len() > 1, |p, o, c| {
            icons::shuffle(p, o, s, c)
        })
        .clicked()
        {
            out.push(Cmd::PlaylistShuffle);
        }
        let tip = match repeat {
            Repeat::Off => tr!("Repeat: off", "Повтор: выключен"),
            Repeat::One => tr!("Repeat: the file", "Повтор: файл"),
            Repeat::All => tr!("Repeat: the list", "Повтор: плейлист"),
        };
        let tip = crate::keymap::with_key(tip, Cmd::CycleRepeat);
        if tool(ui, &tip, true, |p, o, c| {
            let c = if repeat == Repeat::Off {
                c
            } else {
                style::accent()
            };
            icons::repeat(p, o, s, c, repeat == Repeat::One)
        })
        .clicked()
        {
            out.push(Cmd::CycleRepeat);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let window = if detached {
                tr!("Back into the player's window", "Вернуть в окно плеера")
            } else {
                tr!("Into a window of its own", "В отдельное окно")
            };
            let window = crate::keymap::with_key(window, Cmd::TogglePlaylistWindow);
            if tool(ui, &window, true, |p, o, c| {
                icons::detach(p, o, s, c, detached)
            })
            .clicked()
            {
                out.push(Cmd::TogglePlaylistWindow);
            }
        });
    });
    ui.add_space(2.0);
    // The chapters take up to half of the height, the files the rest.
    let open_id = ui.id().with("chapters_open");
    let open = ui.data_mut(|d| *d.get_persisted_mut_or(open_id, true));
    let chapters_height = if chapters.is_empty() {
        0.0
    } else if open {
        HEADER + (chapters.len() as f32 * ROW + 4.0).min(ui.available_height() * 0.5)
    } else {
        HEADER
    };
    let files = vec2(
        ui.available_width(),
        ui.available_height() - chapters_height,
    );
    ui.allocate_ui(files, |ui| files_list(ui, playlist, logos, &mut out));
    if !chapters.is_empty() {
        if chapters_header(ui, chapters.len(), open).clicked() {
            ui.data_mut(|d| d.insert_persisted(open_id, !open));
        }
        if open {
            chapters_list(ui, chapters, time, &mut out);
        }
    }
    out
}

/// The files, filling `ui`: an IPTV list's channels by their names, with
/// their logos.
fn files_list(
    ui: &mut Ui,
    playlist: &mut Playlist,
    mut logos: Option<&mut Logos>,
    out: &mut Vec<Cmd>,
) {
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
            .id_salt("files")
            .auto_shrink([false, false])
            .show_rows(ui, ROW, total, |ui, range| {
                for i in range {
                    let path = &playlist.items()[i];
                    let name = playlist.title(path);
                    let channel = playlist.channel(path);
                    let current = playlist.current() == Some(i);
                    let selected = playlist.selected == Some(i);
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if selected {
                        ui.painter().rect_filled(rect, 2.0, style::selected_bg());
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 2.0, style::HOVER);
                    }
                    let color = if current {
                        style::current()
                    } else {
                        style::TEXT
                    };
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
                    let mut text_left = n.right() + 6.0;
                    if let Some(url) = channel.and_then(|c| c.logo.as_deref())
                        && let Some(logos) = logos.as_deref_mut()
                    {
                        // A square the row's height, the logo fitted in it.
                        let side = ROW - 4.0;
                        let square = egui::Rect::from_min_size(
                            pos2(text_left, rect.center().y - side / 2.0),
                            vec2(side, side),
                        );
                        if let Some(texture) = logos.get(ui.ctx(), url) {
                            let size = texture.size_vec2();
                            let scale = (side / size.x).min(side / size.y);
                            let shown = egui::Rect::from_center_size(square.center(), size * scale);
                            painter.image(
                                texture.id(),
                                shown,
                                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                        text_left = square.right() + 6.0;
                    }
                    let galley = ui.fonts_mut(|f| {
                        let mut job = egui::text::LayoutJob::simple_singleline(name, font, color);
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
                    let tip = match channel.and_then(|c| c.group.as_deref()) {
                        Some(group) => format!(
                            "{group}
{}",
                            path.display()
                        ),
                        None => path.display().to_string(),
                    };
                    let response = response.on_hover_text(tip);
                    if crate::ui::double_clicked(&response) {
                        out.push(Cmd::PlayIndex(i));
                    } else if response.clicked() {
                        playlist.selected = Some(i);
                    }
                }
            });
    });
}

/// The header of the chapters: an arrow (down while open), "Chapters",
/// their number; a click folds or unfolds them.
fn chapters_header(ui: &mut Ui, count: usize, open: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), HEADER), Sense::CLICK);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(0.0, 2.0)), 3.0, style::HOVER);
    }
    let painter = ui.painter();
    let o = pos2(rect.left() + 12.0, rect.center().y);
    let s = 3.5;
    let arrow = if open {
        vec![
            o + vec2(-s, -s * 0.6),
            o + vec2(s, -s * 0.6),
            o + vec2(0.0, s * 0.7),
        ]
    } else {
        vec![
            o + vec2(-s * 0.6, -s),
            o + vec2(s * 0.7, 0.0),
            o + vec2(-s * 0.6, s),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        arrow,
        style::TEXT_WEAK,
        egui::Stroke::NONE,
    ));
    painter.text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        tr!("Chapters", "Главы"),
        FontId::proportional(13.0),
        style::TEXT,
    );
    painter.text(
        pos2(rect.right() - 6.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.0),
        style::TEXT_WEAK,
    );
    response.on_hover_text(if open {
        tr!("Hide the chapters", "Свернуть главы")
    } else {
        tr!("Show the chapters", "Развернуть главы")
    })
}

/// The chapters, filling `ui`: the number, the name, the start on the
/// right; the playing one in the playing entry's colour.
fn chapters_list(ui: &mut Ui, chapters: &[Chapter], time: Option<f64>, out: &mut Vec<Cmd>) {
    let current = time.and_then(|t| chapters::current(chapters, t));
    let long = chapters.last().is_some_and(|c| c.time >= 3600.0);
    let frame = egui::Frame::new()
        .fill(style::LIST_BG)
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(2));
    frame.show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        egui::ScrollArea::vertical()
            .id_salt("chapters")
            .auto_shrink([false, false])
            .show_rows(ui, ROW, chapters.len(), |ui, range| {
                for i in range {
                    let chapter = &chapters[i];
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 2.0, style::HOVER);
                    }
                    let color = if current == Some(i) {
                        style::current()
                    } else {
                        style::TEXT
                    };
                    let painter = ui.painter_at(rect.shrink2(vec2(4.0, 0.0)));
                    let font = FontId::proportional(13.0);
                    let n = painter.text(
                        rect.left_center() + vec2(6.0, 0.0),
                        Align2::LEFT_CENTER,
                        format!("{}.", i + 1),
                        font.clone(),
                        style::TEXT_WEAK,
                    );
                    let start = painter.text(
                        rect.right_center() - vec2(6.0, 0.0),
                        Align2::RIGHT_CENTER,
                        crate::format::time(Some(chapter.time), long),
                        FontId::monospace(11.5),
                        style::TEXT_WEAK,
                    );
                    let name = chapter.name(i + 1);
                    let text_left = n.right() + 6.0;
                    let galley = ui.fonts_mut(|f| {
                        let mut job =
                            egui::text::LayoutJob::simple_singleline(name.clone(), font, color);
                        job.wrap = egui::text::TextWrapping::truncate_at_width(
                            start.left() - 8.0 - text_left,
                        );
                        f.layout_job(job)
                    });
                    painter.galley(
                        pos2(text_left, rect.center().y - galley.size().y / 2.0),
                        galley,
                        color,
                    );
                    if response.on_hover_text(name).clicked() {
                        out.push(Cmd::Chapter(i));
                    }
                }
            });
    });
}
