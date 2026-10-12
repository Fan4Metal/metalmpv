//! The file's information window (`I`): a floating window over the player,
//! so that playback and the keys go on. The cover with the title, the
//! artist and the album at the top, then the file, its tags and its tracks
//! (`info.rs` words them), and a button that copies all of it as text. A
//! click on the cover shows it big.

use std::path::Path;

use egui::{Align, Color32, Layout, Rect, RichText, Sense, TextureHandle, Ui, vec2};

use super::style;
use crate::cover::{Cover, Source};
use crate::i18n::lang;
use crate::info;
use crate::input::Cmd;
use crate::player::State;
use crate::tracks::Kind;

/// The width of the labels' column.
const LABEL_WIDTH: f32 = 130.0;
/// The side of the cover in the header.
const THUMB: f32 = 120.0;

#[derive(Default)]
pub struct InfoWindow {
    pub open: bool,
    /// The cover is shown big over everything.
    zoom: bool,
}

/// The cover as the window gets it: the picture and its texture.
pub struct CoverView<'a> {
    pub cover: &'a Cover,
    pub texture: &'a TextureHandle,
}

impl InfoWindow {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.zoom = false;
    }

    /// Close the big cover, else the window; whether there was anything to
    /// close (`Esc`).
    pub fn close(&mut self) -> bool {
        if self.zoom {
            self.zoom = false;
        } else if self.open {
            self.open = false;
        } else {
            return false;
        }
        true
    }

    /// Draw the window while it is open; the commands of its buttons.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        path: Option<&Path>,
        state: &State,
        cover: Option<CoverView<'_>>,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            return out;
        }
        let screen = ctx.content_rect();
        let mut open = true;
        egui::Window::new(tr!("File information", "Сведения о файле"))
            .id(egui::Id::new("file_info"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(480.0, 560.0))
            .min_width(340.0)
            .default_pos(screen.right_top() + vec2(-500.0, 50.0))
            .constrain_to(screen)
            .frame(
                egui::Frame::window(&ctx.style_of(egui::Theme::Dark))
                    .fill(style::PANEL_BG)
                    .corner_radius(8)
                    .inner_margin(12),
            )
            .show(ctx, |ui| {
                if state.idle {
                    ui.weak(tr!("No file is open.", "Файл не открыт."));
                    return;
                }
                self.header(ui, state, cover.as_ref());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr!("Copy all", "Копировать всё")).clicked() {
                        ui.ctx().copy_text(info::report_in(lang(), path, state));
                    }
                    if cover.is_some()
                        && ui
                            .button(tr!("Save the cover…", "Сохранить обложку…"))
                            .clicked()
                    {
                        out.push(Cmd::SaveCover);
                    }
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, path, state, cover.as_ref(), &mut out));
            });
        if !open {
            self.open = false;
        }
        if self.zoom {
            match &cover {
                Some(c) => self.zoomed(ctx, c.texture),
                None => self.zoom = false,
            }
        }
        out
    }

    /// The cover and the title, the artist and the album beside it.
    fn header(&mut self, ui: &mut Ui, state: &State, cover: Option<&CoverView<'_>>) {
        let title = info::tag(&state.metadata, &["title"])
            .map(str::to_owned)
            .or_else(|| state.title.clone())
            .unwrap_or_default();
        let artist = info::artist(&state.metadata);
        let album = info::album(&state.metadata);
        ui.horizontal_top(|ui| {
            if let Some(c) = cover {
                let size = fit(c.texture.size_vec2(), vec2(THUMB, THUMB));
                let (rect, response) = ui.allocate_exact_size(vec2(THUMB, THUMB), Sense::CLICK);
                let image = Rect::from_center_size(rect.center(), size);
                ui.painter().image(
                    c.texture.id(),
                    image,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                if response.hovered() {
                    ui.painter().rect_stroke(
                        image,
                        0.0,
                        egui::Stroke::new(1.0, style::ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
                if response
                    .on_hover_text(tr!("Show it big", "Показать крупно"))
                    .clicked()
                {
                    self.zoom = true;
                }
                ui.add_space(10.0);
            }
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.add(
                    egui::Label::new(RichText::new(title).size(17.0).color(style::TEXT_BRIGHT))
                        .wrap(),
                );
                if let Some(artist) = artist {
                    ui.add(egui::Label::new(RichText::new(artist).size(14.0)).wrap());
                }
                if let Some(album) = album {
                    ui.add(egui::Label::new(RichText::new(album).color(style::TEXT_WEAK)).wrap());
                }
            });
        });
    }

    /// The cover over the whole window, as big as it fits; a click or `Esc`
    /// closes it.
    fn zoomed(&mut self, ctx: &egui::Context, texture: &TextureHandle) {
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("cover_zoom"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::CLICK);
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(220));
                // Not past the picture's own size times two, nor the screen.
                let natural = texture.size_vec2() / ui.ctx().pixels_per_point() * 2.0;
                let room = (rect.size() - vec2(48.0, 48.0)).min(natural);
                let size = fit(texture.size_vec2(), room);
                ui.painter().image(
                    texture.id(),
                    Rect::from_center_size(rect.center(), size),
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                if response.clicked() {
                    self.zoom = false;
                }
            });
    }
}

/// `size` scaled to fit in `room`, its proportions kept.
fn fit(size: egui::Vec2, room: egui::Vec2) -> egui::Vec2 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return room;
    }
    let scale = (room.x / size.x).min(room.y / size.y);
    size * scale
}

/// The file, the tags and the tracks.
fn body(
    ui: &mut Ui,
    path: Option<&Path>,
    state: &State,
    cover: Option<&CoverView<'_>>,
    out: &mut Vec<Cmd>,
) {
    let lang = lang();
    heading(ui, tr!("File", "Файл"));
    let folder_label = tr!("Folder", "Папка");
    for (label, value) in info::file_rows_in(lang, path, state) {
        if label == folder_label {
            row_with(ui, &label, |ui| {
                if ui
                    .add(
                        egui::Label::new(RichText::new(&value).color(style::ACCENT_HOVER))
                            .sense(Sense::click())
                            .wrap(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(tr!("Show in Explorer", "Показать в Проводнике"))
                    .clicked()
                {
                    out.push(Cmd::ShowInFolder);
                }
            });
        } else {
            row(ui, &label, &value);
        }
    }
    if let Some(c) = cover {
        let [w, h] = c.cover.size;
        let kind = c.cover.extension.to_uppercase();
        let value = match &c.cover.source {
            Source::Embedded => tr!(
                format!("in the file, {w}×{h} {kind}"),
                format!("в файле, {w}×{h} {kind}")
            ),
            Source::Folder(file) => {
                let name = file.file_name().unwrap_or_default().to_string_lossy();
                tr!(
                    format!("{name} next to the file, {w}×{h}"),
                    format!("{name} рядом с файлом, {w}×{h}")
                )
            }
        };
        row(ui, tr!("Cover", "Обложка"), &value);
    }

    let tags = info::tags_in(lang, &state.metadata);
    if !tags.is_empty() {
        heading(ui, tr!("Tags", "Теги"));
        for (label, value) in tags {
            row(ui, &label, &value);
        }
    }

    if !state.tracks.is_empty() {
        heading(ui, tr!("Tracks", "Дорожки"));
        let selected_video = crate::tracks::selected(&state.tracks, Kind::Video).map(|t| t.id);
        for track in &state.tracks {
            let params = state
                .video_params
                .as_ref()
                .filter(|_| track.kind == Kind::Video && Some(track.id) == selected_video);
            let heading = info::track_heading_in(lang, track, &state.tracks);
            let details = info::track_details_in(lang, track, params);
            let response = ui.add(
                egui::Label::new(RichText::new(heading).color(if track.selected {
                    style::TEXT_BRIGHT
                } else {
                    style::TEXT
                }))
                .wrap(),
            );
            if let Some(desc) = &track.codec_desc {
                response.on_hover_text(desc);
            }
            if !details.is_empty() {
                ui.add(egui::Label::new(RichText::new(details).color(style::TEXT_WEAK)).wrap());
            }
            ui.add_space(4.0);
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(
        RichText::new(text)
            .size(15.0)
            .strong()
            .color(style::TEXT_BRIGHT),
    );
    ui.add_space(2.0);
}

/// A label on the left, the value wrapped on the right.
fn row(ui: &mut Ui, label: &str, value: &str) {
    row_with(ui, label, |ui| {
        ui.add(egui::Label::new(value).wrap());
    });
}

fn row_with(ui: &mut Ui, label: &str, value: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(LABEL_WIDTH, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(LABEL_WIDTH);
            ui.add(egui::Label::new(RichText::new(label).color(style::TEXT_WEAK)).wrap());
        });
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            value(ui);
        });
    });
}
