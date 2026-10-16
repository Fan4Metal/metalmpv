//! The file's information window (`I`): a floating window over the player,
//! so that playback and the keys go on. The cover with the title, the
//! artist and the album at the top, then the file, its tags and its tracks
//! (`info.rs` words them), and a button that copies all of it as text. A
//! click on the cover shows it big.

use std::time::{Duration, Instant};

use egui::collapsing_header::CollapsingState;
use egui::{Align, Color32, Layout, Rect, RichText, Sense, TextureHandle, Ui, vec2};

use super::{icons, style, tool};
use crate::cover::{Cover, Source};
use crate::i18n::lang;
use crate::info::{self, ChipAction, Subject, Tone};
use crate::input::Cmd;
use crate::playlist::Channel;
use crate::tracks::{Kind, Track};

/// The side of the cover in the header.
const THUMB: f32 = 120.0;
/// The width of a button of the header (`ui::tool`).
const TOOL: f32 = 26.0;
/// The side of a channel's logo in the header.
const LOGO: f32 = 64.0;
/// The window's width (egui's, with the frame).
const WIDTH: f32 = 520.0;
/// The window's top right corner from the player's: under the title row,
/// a little in from the edge.
const CORNER: egui::Vec2 = vec2(-23.0, 46.0);

/// How long Copy all shows a tick after a click.
const COPIED_TICK: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct InfoWindow {
    pub open: bool,
    /// The cover is shown big over everything.
    zoom: bool,
    /// When Copy all was last clicked.
    copied: Option<Instant>,
    /// The file whose chips were unfolded ("+3" or the subtitles' chip
    /// clicked), and which; another file folds them again.
    unfolded: Option<(std::path::PathBuf, info::Unfolded)>,
}

/// What an IPTV list says about the entry playing, and its logo once
/// fetched (`logos.rs`).
pub struct ChannelView<'a> {
    pub channel: &'a Channel,
    pub logo: Option<TextureHandle>,
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

    /// Draw the window while it is open; the commands of its buttons. It
    /// stays at the player's top right corner (anchored, so it follows the
    /// corner as the player's window changes). The width is fixed; only the bottom edge is dragged, and
    /// egui's memory keeps the height.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        subject: &Subject,
        cover: Option<CoverView<'_>>,
        channel: Option<ChannelView<'_>>,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            return out;
        }
        let state = subject.state;
        let screen = ctx.content_rect();
        let mut open = true;
        let window = egui::Window::new(tr!("File information", "Сведения о файле"))
            .id(egui::Id::new("file_info"))
            .open(&mut open)
            .collapsible(false);
        window
            .anchor(egui::Align2::RIGHT_TOP, CORNER)
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
                let title = title_of(subject, channel.as_ref().map(|v| v.channel));
                out.extend(self.header(ui, subject, &title, cover.as_ref(), channel.as_ref()));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| body(ui, subject, &title, cover.as_ref(), &mut out));
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
        subject: &Subject,
        title: &str,
        cover: Option<&CoverView<'_>>,
        view: Option<&ChannelView<'_>>,
    ) -> Vec<Cmd> {
        let mut out = Vec::new();
        let state = subject.state;
        let channel = view.map(|v| v.channel);
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
                ui.horizontal_top(|ui| {
                    let buttons = if cover.is_some() { 2.0 } else { 1.0 };
                    let room = (ui.available_width() - buttons * TOOL - 8.0).max(60.0);
                    ui.allocate_ui_with_layout(
                        vec2(room, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(room);
                            // Down a little: the title's first line is
                            // centred on the buttons.
                            ui.add_space(1.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(title).size(17.0).color(style::TEXT_BRIGHT),
                                )
                                .wrap(),
                            );
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        self.tools(ui, subject, cover.is_some(), &mut out);
                    });
                });
                if let Some(artist) = artist {
                    ui.add(egui::Label::new(RichText::new(artist).size(14.0)).wrap());
                }
                if let Some(album) = album {
                    ui.add(egui::Label::new(RichText::new(album).color(style::TEXT_WEAK)).wrap());
                }
                let unfolded = match (&self.unfolded, subject.path) {
                    (Some((of, unfolded)), Some(path)) if of == path => *unfolded,
                    _ => info::Unfolded::default(),
                };
                let rows = info::chips_in(lang(), subject, unfolded);
                if !rows.is_empty() {
                    ui.add_space(6.0);
                }
                for row in rows {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for c in row {
                            if chip(ui, &c).clicked() {
                                match c.action {
                                    ChipAction::Audio(id) => out.push(Cmd::SelectAudio(id)),
                                    ChipAction::Sub(id) => out.push(Cmd::SelectSub(Some(id))),
                                    ChipAction::Unfold(kind) | ChipAction::Fold(kind) => {
                                        let on = matches!(c.action, ChipAction::Unfold(_));
                                        let mut now = unfolded;
                                        now.set(kind, on);
                                        self.unfolded =
                                            subject.path.map(|p| (p.to_path_buf(), now));
                                    }
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

    /// The window's buttons, from the right: Copy all (a tick for a moment
    /// after a click), then Save the cover… when there is one.
    fn tools(&mut self, ui: &mut Ui, subject: &Subject, cover: bool, out: &mut Vec<Cmd>) {
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
            ui.ctx().copy_text(info::report_in(lang(), subject));
            self.copied = Some(Instant::now());
        }
        if cover
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

/// The header's title: an IPTV list's channel by the list's name (a
/// stream's title is its file's, "index.m3u8"), else the file's title tag,
/// else mpv's title (the file's name).
fn title_of(subject: &Subject, channel: Option<&Channel>) -> String {
    channel
        .and_then(|c| c.title.clone())
        .or_else(|| info::tag(&subject.state.metadata, &["title"]).map(str::to_owned))
        .or_else(|| subject.state.title.clone())
        .unwrap_or_default()
}

/// The labels' column: as wide as the widest label of the window (folded
/// tracks' too, so that unfolding one moves nothing), within these.
const LABELS_MIN: f32 = 90.0;
const LABELS_MAX: f32 = 200.0;
/// A value longer than this many lines or characters is cut short, with
/// "more" to show it all (a tag's lyrics, a description).
const SHORT_LINES: usize = 3;
const SHORT_CHARS: usize = 240;

/// The file, the playback, the tags and the tracks, each a section that
/// folds (egui's memory keeps which) and copies on a right click.
fn body(
    ui: &mut Ui,
    subject: &Subject,
    title: &str,
    cover: Option<&CoverView<'_>>,
    out: &mut Vec<Cmd>,
) {
    let state = subject.state;
    let lang = lang();
    let name_label = tr!("Name", "Имя");
    let folder_label = tr!("Folder", "Папка");
    let mut file = info::file_rows_in(lang, subject);
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
        file.push((tr!("Cover", "Обложка").to_owned(), value));
    }
    let live = info::live_rows_in(lang, subject);
    let tags = info::tags_in(lang, &state.metadata);
    let tracks: Vec<Vec<(String, String)>> = state
        .tracks
        .iter()
        .map(|t| info::track_rows_in(lang, t, subject))
        .collect();

    let font = egui::TextStyle::Body.resolve(ui.style());
    let graph_label = tr!("Last minute", "За минуту");
    let widest = file
        .iter()
        .chain(&live)
        .chain(&tags)
        .chain(tracks.iter().flatten())
        .map(|(l, _)| l.as_str())
        .chain([graph_label])
        .map(|l| {
            ui.fonts_mut(|f| f.layout_no_wrap(l.to_owned(), font.clone(), Color32::WHITE))
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let labels = (widest + 4.0).clamp(LABELS_MIN, LABELS_MAX);

    let file_text = info::section_text(tr!("File", "Файл"), &file);
    section(ui, "file", tr!("File", "Файл"), file_text, |ui| {
        for (label, value) in &file {
            // The header says the name already.
            if label == name_label && value == title {
                continue;
            }
            if label == folder_label {
                row_with(ui, label, labels, |ui| {
                    let link = ui
                        .add(
                            egui::Label::new(RichText::new(value).color(style::accent_hover()))
                                .sense(Sense::click())
                                .wrap(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(tr!("Show in Explorer", "Показать в Проводнике"));
                    if link.clicked() {
                        out.push(Cmd::ShowInFolder);
                    }
                    copy_menu(&link, label, value);
                });
            } else {
                row(ui, label, value, labels);
            }
        }
    });

    if !live.is_empty() {
        let heading = tr!("Playback", "Воспроизведение");
        section(
            ui,
            "playback",
            heading,
            info::section_text(heading, &live),
            |ui| {
                for (label, value) in &live {
                    row(ui, label, value, labels);
                }
                if state.live.history.len() >= 2 {
                    row_with(ui, graph_label, labels, |ui| {
                        sparkline(ui, &state.live.history);
                    });
                }
            },
        );
    }

    if !tags.is_empty() {
        let heading = tr!("Tags", "Теги");
        section(
            ui,
            "tags",
            heading,
            info::section_text(heading, &tags),
            |ui| {
                for (label, value) in &tags {
                    long_row(ui, label, value, labels);
                }
            },
        );
    }

    if !state.tracks.is_empty() {
        let heading = tr!("Tracks", "Дорожки");
        let text = info::tracks_text_in(lang, subject);
        section(ui, "tracks", heading, text, |ui| {
            for kind in [Kind::Video, Kind::Audio, Kind::Sub] {
                let of_kind: Vec<(&Track, &Vec<(String, String)>)> = state
                    .tracks
                    .iter()
                    .zip(&tracks)
                    .filter(|(t, _)| t.kind == kind)
                    .collect();
                if of_kind.is_empty() {
                    continue;
                }
                let covers_only = of_kind.iter().all(|(t, _)| t.albumart);
                ui.add_space(4.0);
                ui.label(
                    RichText::new(info::kind_in(lang, kind, covers_only))
                        .small()
                        .strong()
                        .color(style::TEXT_WEAK),
                );
                for (track, rows) in of_kind {
                    track_entry(ui, track, rows, subject, labels);
                    ui.add_space(4.0);
                }
            }
        });
    }
}

/// A section: its heading, which folds it on a click (egui's memory keeps
/// it) and copies it as text on a right click, then its rows.
fn section(ui: &mut Ui, id: &str, title: &str, text: String, rows: impl FnOnce(&mut Ui)) {
    let id = ui.make_persistent_id(("info_section", id));
    let mut fold = CollapsingState::load_with_default_open(ui.ctx(), id, true);
    ui.add_space(8.0);
    let heading = ui
        .horizontal(|ui| {
            fold.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
            ui.add(
                egui::Label::new(
                    RichText::new(title)
                        .size(15.0)
                        .strong()
                        .color(style::TEXT_BRIGHT),
                )
                .sense(Sense::click()),
            )
        })
        .inner;
    if heading.clicked() {
        fold.toggle(ui);
    }
    heading.context_menu(|ui| {
        if ui
            .button(tr!("Copy the section", "Копировать раздел"))
            .clicked()
        {
            ui.ctx().copy_text(text);
            ui.close();
        }
    });
    fold.show_body_unindented(ui, |ui| {
        ui.add_space(2.0);
        rows(ui);
    });
}

/// A track: a dot in the accent colour when it plays, its number and name
/// over its main details; a click on them or on the arrow unfolds its
/// rows (egui's memory keeps it, per track; the picture on screen starts
/// unfolded).
fn track_entry(
    ui: &mut Ui,
    track: &Track,
    rows: &[(String, String)],
    subject: &Subject,
    labels: f32,
) {
    let state = subject.state;
    let lang = lang();
    let id = ui.make_persistent_id(("track_rows", track.kind, track.id));
    let on_screen = track.kind == Kind::Video && track.selected && !track.albumart;
    let mut fold = CollapsingState::load_with_default_open(ui.ctx(), id, on_screen);
    let header = ui.horizontal_top(|ui| {
        fold.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
        let (dot, _) = ui.allocate_exact_size(vec2(8.0, 18.0), Sense::hover());
        if track.selected {
            ui.painter()
                .circle_filled(dot.center(), 3.5, style::accent());
        }
        ui.vertical(|ui| {
            let n = track.number(&state.tracks);
            let heading = if track.albumart {
                info::kind_in(lang, track.kind, true).to_owned()
            } else {
                format!("{n}. {}", track.name_in(lang, n))
            };
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
            let details = info::track_details_in(lang, track, subject);
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
        for (label, value) in rows {
            row(ui, label, value, labels);
        }
    });
}

/// The bitrate of the last minute, a sample a second, the latest at the
/// right edge: a line over its area from zero, the highest value under it
/// and the one under the pointer in a tooltip.
fn sparkline(ui: &mut Ui, history: &[f32]) {
    let lang = lang();
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let max = history.iter().copied().fold(0.0, f32::max).max(1.0);
    let step = rect.width() / (crate::player::HISTORY - 1) as f32;
    let first = rect.right() - step * (history.len() - 1) as f32;
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .map(|(i, b)| {
            egui::pos2(
                first + step * i as f32,
                rect.bottom() - b / max * (rect.height() - 2.0),
            )
        })
        .collect();
    let painter = ui.painter_at(rect.expand(4.0));
    let base = egui::Stroke::new(1.0, Color32::from_white_alpha(28));
    painter.hline(rect.x_range(), rect.bottom(), base);
    let fill = style::accent().gamma_multiply(0.22);
    let mut area = egui::Mesh::default();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let i = area.vertices.len() as u32;
        area.colored_vertex(a, fill);
        area.colored_vertex(b, fill);
        area.colored_vertex(egui::pos2(b.x, rect.bottom()), fill);
        area.colored_vertex(egui::pos2(a.x, rect.bottom()), fill);
        area.add_triangle(i, i + 1, i + 2);
        area.add_triangle(i, i + 2, i + 3);
    }
    painter.add(area);
    painter.add(egui::Shape::line(
        points.clone(),
        egui::Stroke::new(2.0, style::accent()),
    ));
    if let Some(pointer) = response.hover_pos() {
        let i = ((pointer.x - first) / step).round();
        if i >= 0.0 && (i as usize) < points.len() {
            let i = i as usize;
            let at = points[i];
            painter.vline(
                at.x,
                rect.y_range(),
                egui::Stroke::new(1.0, Color32::from_white_alpha(70)),
            );
            painter.circle_filled(at, 4.0, style::accent());
            let ago = history.len() - 1 - i;
            let when = match (ago, lang) {
                (0, crate::i18n::Lang::En) => "now".to_owned(),
                (0, crate::i18n::Lang::Ru) => "сейчас".to_owned(),
                (n, crate::i18n::Lang::En) => format!("{n} s ago"),
                (n, crate::i18n::Lang::Ru) => format!("{n} с назад"),
            };
            // On the graph beside the line, not in a tooltip: egui's waits
            // for the pointer to rest, and a pointer moving along the graph
            // never showed it.
            let text = format!("{} · {when}", info::bitrate_in(lang, f64::from(history[i])));
            let galley = painter.layout_no_wrap(
                text,
                egui::TextStyle::Small.resolve(ui.style()),
                style::TEXT_BRIGHT,
            );
            let pad = vec2(5.0, 2.0);
            let size = galley.size() + pad * 2.0;
            // Right of the line, else left of it where it would run out.
            let x = if at.x + 8.0 + size.x <= rect.right() {
                at.x + 8.0
            } else {
                (at.x - 8.0 - size.x).max(rect.left())
            };
            let label = Rect::from_min_size(egui::pos2(x, rect.top()), size);
            painter.rect_filled(label, 3.0, Color32::from_black_alpha(200));
            painter.galley(label.min + pad, galley, style::TEXT_BRIGHT);
        }
    }
    let most = info::bitrate_in(lang, f64::from(max));
    ui.label(
        RichText::new(tr!(format!("up to {most}"), format!("до {most}")))
            .small()
            .color(style::TEXT_WEAK),
    );
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
/// one that does something lit when hovered. The format chip is its file
/// icon's band with white letters, and the picture, the sound and the
/// subtitles each a tint of their own hue.
fn chip(ui: &mut Ui, chip: &info::Chip) -> egui::Response {
    let clickable = chip.action != ChipAction::None;
    let tone = chip.tone;
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

/// A label on the left in a column `labels` wide, the value wrapped on the
/// right, selectable, and copied by its menu.
fn row(ui: &mut Ui, label: &str, value: &str, labels: f32) {
    row_with(ui, label, labels, |ui| {
        let response = ui.add(egui::Label::new(value).wrap().selectable(true));
        copy_menu(&response, label, value);
    });
}

/// A row whose value may be long: cut to its first lines with "more" to
/// show it all (egui's memory keeps it, per label).
fn long_row(ui: &mut Ui, label: &str, value: &str, labels: f32) {
    let Some(short) = cut_short(value) else {
        row(ui, label, value, labels);
        return;
    };
    let id = ui.make_persistent_id(("long_row", label));
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    row_with(ui, label, labels, |ui| {
        let text = if open { value } else { short.as_str() };
        let response = ui.add(egui::Label::new(text).wrap().selectable(true));
        copy_menu(&response, label, value);
        let more = if open {
            tr!("less", "свернуть")
        } else {
            tr!("more", "ещё")
        };
        if ui.link(more).clicked() {
            ui.data_mut(|d| d.insert_temp(id, !open));
        }
    });
}

/// The start of a value longer than [`SHORT_LINES`] lines or
/// [`SHORT_CHARS`] characters, with "…"; `None` for a short one.
fn cut_short(value: &str) -> Option<String> {
    let lines: Vec<&str> = value.lines().collect();
    if lines.len() <= SHORT_LINES && value.chars().count() <= SHORT_CHARS {
        return None;
    }
    let start = lines[..lines.len().min(SHORT_LINES)].join("\n");
    let mut short: String = start.chars().take(SHORT_CHARS).collect();
    short = short.trim_end().to_owned();
    short.push('…');
    Some(short)
}

/// The menu of a value: copy it, or the row.
fn copy_menu(response: &egui::Response, label: &str, value: &str) {
    response.context_menu(|ui| {
        if ui.button(tr!("Copy", "Копировать")).clicked() {
            ui.ctx().copy_text(value.to_owned());
            ui.close();
        }
        if ui
            .button(tr!("Copy the row", "Копировать строку"))
            .clicked()
        {
            ui.ctx().copy_text(format!("{label}: {value}"));
            ui.close();
        }
    });
}

fn row_with(ui: &mut Ui, label: &str, labels: f32, value: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(labels, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(labels);
            ui.add(egui::Label::new(RichText::new(label).color(style::TEXT_WEAK)).wrap());
        });
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            value(ui);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_values_cut_short() {
        assert_eq!(cut_short("One line"), None);
        assert_eq!(cut_short("1\n2\n3\n4\n5").as_deref(), Some("1\n2\n3…"));
        let long = "word ".repeat(100);
        let short = cut_short(&long).unwrap();
        assert!(short.ends_with("word…"));
        assert!(short.chars().count() <= SHORT_CHARS + 1);
    }
}
