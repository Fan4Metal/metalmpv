//! The file's information window (`I`): a floating window over the player,
//! so that playback and the keys go on. The cover with the title, the
//! artist and the album at the top, then the file, its tags and its tracks
//! (`info.rs` words them), and a button that copies all of it as text. A
//! click on the cover shows it big.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::collapsing_header::CollapsingState;
use egui::{Align, Color32, Layout, Rect, RichText, Sense, TextureHandle, Ui, vec2};

use super::{icons, style, tool};
use crate::cover::{Cover, Source};
use crate::i18n::lang;
use crate::info::{self, ChipAction, Tone};
use crate::input::Cmd;
use crate::player::State;
use crate::playlist::Channel;
use crate::tracks::Track;

/// The width of the labels' column.
const LABEL_WIDTH: f32 = 170.0;
/// The side of the cover in the header.
const THUMB: f32 = 120.0;
/// The side of a channel's logo in the header.
const LOGO: f32 = 64.0;
/// The window's width (egui's, with the frame).
pub const WIDTH: f32 = 520.0;
/// The window's top right corner from the player's: under the title row,
/// a little in from the edge. The Video, sound and subtitles panel's too.
pub const CORNER: egui::Vec2 = vec2(-23.0, 46.0);

/// How long Copy all shows a tick after a click.
const COPIED_TICK: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct InfoWindow {
    pub open: bool,
    /// The cover is shown big over everything.
    zoom: bool,
    /// When Copy all was last clicked.
    copied: Option<Instant>,
}

/// What an IPTV list says about the entry playing, and its logo once
/// fetched (`logos.rs`).
pub struct ChannelView<'a> {
    pub channel: &'a Channel,
    pub logo: Option<TextureHandle>,
}

/// How the window is shown, from the settings.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// At the player's top right corner, else where it was left.
    pub pinned: bool,
    /// The chips in colour (`settings.info_colours`).
    pub colours: bool,
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

    /// Draw the window while it is open; the commands of its buttons. A
    /// pinned window stays at the player's top right corner (anchored,
    /// so it follows the corner as the player's window changes); else it
    /// starts there, is moved by its title and egui's memory keeps its
    /// place. The width is fixed; only the bottom edge is dragged, and
    /// egui's memory keeps the height.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        path: Option<&Path>,
        state: &State,
        cover: Option<CoverView<'_>>,
        look: Look,
        channel: Option<ChannelView<'_>>,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            return out;
        }
        let screen = ctx.content_rect();
        let mut open = true;
        let window = egui::Window::new(tr!("File information", "Сведения о файле"))
            .id(egui::Id::new("file_info"))
            .open(&mut open)
            .collapsible(false);
        let window = if look.pinned {
            window.anchor(egui::Align2::RIGHT_TOP, CORNER)
        } else {
            // The same pivot as the anchor's: unpinning leaves it in place.
            window
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos(screen.right_top() + CORNER)
        };
        window
            .resizable([false, true])
            .default_height(620.0)
            .min_height(200.0)
            .min_width(WIDTH)
            .max_width(WIDTH)
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
                out.extend(self.header(
                    ui,
                    path,
                    state,
                    cover.as_ref(),
                    channel.as_ref(),
                    look.colours,
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let s = 6.0;
                    let copied = self
                        .copied
                        .map(|t| t.elapsed())
                        .filter(|e| *e < COPIED_TICK);
                    if let Some(e) = copied {
                        ui.ctx().request_repaint_after(COPIED_TICK - e);
                    }
                    let copy = tool(
                        ui,
                        tr!("Copy all", "Копировать всё"),
                        true,
                        |p, o, c| {
                            if copied.is_some() {
                                icons::check(p, o, s, c);
                            } else {
                                icons::copy(p, o, s, c);
                            }
                        },
                    );
                    if copy.clicked() {
                        ui.ctx().copy_text(info::report_in(lang(), path, state));
                        self.copied = Some(Instant::now());
                    }
                    if cover.is_some()
                        && tool(
                            ui,
                            tr!("Save the cover…", "Сохранить обложку…"),
                            true,
                            |p, o, c| icons::save(p, o, s, c),
                        )
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

    /// The cover and, beside it, the title, the artist and the album, then
    /// the chips: the file's main facts at a glance ([`info::chips_in`]);
    /// the commands of the chips clicked.
    fn header(
        &mut self,
        ui: &mut Ui,
        path: Option<&Path>,
        state: &State,
        cover: Option<&CoverView<'_>>,
        view: Option<&ChannelView<'_>>,
        colours: bool,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        let channel = view.map(|v| v.channel);
        // An IPTV list's channel by the list's name (a stream's title is
        // its file's, "index.m3u8"), with its group in place of an album.
        let title = channel
            .and_then(|c| c.title.clone())
            .or_else(|| info::tag(&state.metadata, &["title"]).map(str::to_owned))
            .or_else(|| state.title.clone())
            .unwrap_or_default();
        let artist = info::artist(&state.metadata);
        let album = info::album(&state.metadata).or_else(|| channel?.group.clone());
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
                        egui::Stroke::new(1.0, style::accent()),
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
            } else if let Some(logo) = view.and_then(|v| v.logo.as_ref()) {
                // The channel's logo in place of a cover, no larger than
                // fetched (`logos::THUMB`).
                let side = LOGO;
                let size = fit(logo.size_vec2(), vec2(side, side));
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                ui.painter().image(
                    logo.id(),
                    Rect::from_center_size(rect.center(), size),
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
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
                let rows = info::chips_in(lang(), path, state);
                if !rows.is_empty() {
                    ui.add_space(6.0);
                }
                for row in rows {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for c in row {
                            if chip(ui, &c, colours).clicked() {
                                match c.action {
                                    ChipAction::Audio(id) => out.push(Cmd::SelectAudio(id)),
                                    ChipAction::None => {}
                                }
                            }
                        }
                    });
                }
            });
        });
        out
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
                        egui::Label::new(RichText::new(&value).color(style::accent_hover()))
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
        for track in &state.tracks {
            track_entry(ui, track, state);
            ui.add_space(4.0);
        }
    }
}

/// A track: its name and main details in one line, the playing ones
/// brighter; a click on them or on the arrow unfolds all its rows (the open
/// state is egui's memory, per track).
fn track_entry(ui: &mut Ui, track: &Track, state: &State) {
    let lang = lang();
    let id = ui.make_persistent_id(("track_rows", track.kind, track.id));
    let mut fold = CollapsingState::load_with_default_open(ui.ctx(), id, false);
    let header = ui.horizontal_top(|ui| {
        fold.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
        ui.vertical(|ui| {
            let heading = info::track_heading_in(lang, track, &state.tracks);
            let color = if track.selected {
                style::TEXT_BRIGHT
            } else {
                style::TEXT
            };
            let mut response = ui
                .add(
                    egui::Label::new(RichText::new(heading).color(color))
                        .sense(Sense::click())
                        .wrap(),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if let Some(desc) = &track.codec_desc {
                response = response.on_hover_text(desc);
            }
            let mut clicked = response.clicked();
            let details = info::track_details_in(lang, track, state);
            if !details.is_empty() {
                clicked |= ui
                    .add(
                        egui::Label::new(RichText::new(details).color(style::TEXT_WEAK))
                            .sense(Sense::click())
                            .wrap(),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked();
            }
            clicked
        })
        .inner
    });
    if header.inner {
        fold.toggle(ui);
    }
    fold.show_body_unindented(ui, |ui| {
        ui.add_space(4.0);
        for (label, value) in info::track_rows_in(lang, track, state) {
            row(ui, &label, &value);
        }
    });
}

/// The hue of a kind of chip: the picture, the sound, the subtitles.
fn hue(tone: Tone) -> Option<Color32> {
    match tone {
        Tone::Picture => Some(Color32::from_rgb(0x5b, 0x9d, 0xe0)),
        Tone::Sound => Some(Color32::from_rgb(0x5e, 0xc2, 0x7f)),
        Tone::Subtitles => Some(Color32::from_rgb(0xe6, 0xb4, 0x4c)),
        Tone::Plain | Tone::Format(_) => None,
    }
}

/// A chip of the header: plain text on a light ground, as the badges of
/// the tracks window; the playing track's outlined in the accent colour,
/// one that does something lit when hovered. With `colours`, the format
/// chip is its file icon's band with white letters, and the picture, the
/// sound and the subtitles each a tint of their own hue.
fn chip(ui: &mut Ui, chip: &info::Chip, colours: bool) -> egui::Response {
    let clickable = chip.action != ChipAction::None;
    let tone = if colours { chip.tone } else { Tone::Plain };
    let (ground, lit, color) = match tone {
        Tone::Format(c) => {
            let band = Color32::from_rgb(c.band[0], c.band[1], c.band[2]);
            (band, band, style::TEXT_BRIGHT)
        }
        _ => match hue(tone) {
            Some(h) => (
                h.gamma_multiply(0.22),
                h.gamma_multiply(0.36),
                if chip.selected {
                    style::TEXT_BRIGHT
                } else {
                    h.lerp_to_gamma(Color32::WHITE, 0.55)
                },
            ),
            None => (
                Color32::from_white_alpha(18),
                Color32::from_white_alpha(34),
                if chip.selected {
                    style::TEXT_BRIGHT
                } else {
                    style::TEXT
                },
            ),
        },
    };
    let galley = ui.fonts_mut(|f| {
        f.layout_no_wrap(chip.text.clone(), egui::FontId::proportional(12.0), color)
    });
    let size = vec2(galley.size().x + 12.0, 19.0);
    let sense = if clickable {
        Sense::CLICK
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let painter = ui.painter();
    let ground = if clickable && response.hovered() {
        lit
    } else {
        ground
    };
    painter.rect_filled(rect, 4.0, ground);
    if chip.selected {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, style::accent_hover()),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(
        egui::Align2::CENTER_CENTER
            .anchor_size(rect.center(), galley.size())
            .min,
        galley,
        color,
    );
    let response = if clickable {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    };
    match &chip.tip {
        Some(tip) => response.on_hover_text(tip),
        None => response,
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
