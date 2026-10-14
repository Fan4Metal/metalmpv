//! The tracks window over the control bar's tracks button (`T`), as the
//! Video, sound and subtitles panel is (`popover.rs`: the same frame and
//! place, a press outside closes it, the keys go on): the audio tracks and
//! the subtitles side by side, each in a column of its own that scrolls
//! when long. A choice applies at once and the window stays open, so both
//! can be changed in one go; `T` or `Esc` closes it too. The same lists are
//! submenus of the picture's context menu.

use egui::{Align2, Color32, FontId, Rect, Response, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{icons, popover, style};
use crate::i18n::lang;
use crate::input::Cmd;
use crate::tracks::{self, Kind, Track};

/// The window, while open.
#[derive(Default)]
pub struct TracksWindow {
    pub open: bool,
    /// Where it was drawn last, for the presses outside it.
    rect: Option<Rect>,
}

impl TracksWindow {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Draw the window over `button` (the control bar's, where it was drawn
    /// this frame; else the player's bottom right corner) while it is open;
    /// the choices made.
    pub fn show(&mut self, ctx: &egui::Context, button: Option<Rect>, all: &[Track]) -> Vec<Cmd> {
        let mut out = Vec::new();
        if !self.open {
            self.rect = None;
            return out;
        }
        if popover::pressed_outside(ctx, self.rect, button) {
            self.open = false;
            self.rect = None;
            return out;
        }
        let corner = popover::corner(ctx, button);
        let shown = popover::show(ctx, "tracks_window", corner, WIDTH, |ui| {
            out = panel(ui, all);
        });
        self.rect = Some(shown);
        out
    }
}

/// The width of a column.
const COLUMN: f32 = 270.0;
/// The width of the window's content: the columns and the line between.
const WIDTH: f32 = 2.0 * COLUMN + 13.0;
/// The height of a row with a name only ("Off").
const ROW: f32 = 30.0;
/// The height of a track's row: the name, the badges under it.
const ROW_TWO: f32 = 44.0;
/// A column scrolls past this height (Blu-ray rips may carry 30 subtitles).
const MAX_LIST: f32 = 300.0;

/// The contents of the window.
pub fn panel(ui: &mut Ui, all: &[Track]) -> Vec<Cmd> {
    let mut out = Vec::new();
    ui.set_width(WIDTH);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            column(ui, all, Kind::Audio, &mut out);
        });
        let (line, _) =
            ui.allocate_exact_size(vec2(1.0, ui.min_rect().height().max(60.0)), Sense::hover());
        ui.painter().vline(
            line.center().x,
            line.y_range(),
            Stroke::new(1.0, style::LINE),
        );
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            column(ui, all, Kind::Sub, &mut out);
        });
    });
    out
}

fn column(ui: &mut Ui, all: &[Track], kind: Kind, out: &mut Vec<Cmd>) {
    let audio = kind == Kind::Audio;
    let title = if audio {
        tr!("Audio", "Звук")
    } else {
        tr!("Subtitles", "Субтитры")
    };
    // As the panel's groups: small and muted.
    ui.label(
        RichText::new(title)
            .small()
            .strong()
            .color(style::TEXT_WEAK),
    );
    ui.add_space(4.0);
    let list = tracks::of_kind(all, kind);
    egui::ScrollArea::vertical()
        .id_salt(("tracks", audio))
        .max_height(MAX_LIST)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if !audio {
                let off = tracks::selected(all, Kind::Sub).is_none();
                if row(ui, off, tr!("Off", "Выключены"), &[], None).clicked() && !off {
                    out.push(Cmd::SelectSub(None));
                }
            }
            if list.is_empty() {
                ui.add_space(4.0);
                ui.weak(if audio {
                    tr!("No audio", "Звука нет")
                } else {
                    tr!("No subtitles in the file", "В файле нет субтитров")
                });
            }
            for (i, t) in list.iter().enumerate() {
                let name = format!("{}. {}", i + 1, t.name_in(lang(), i + 1));
                let badges = t.badges_in(lang());
                let tip = if badges.is_empty() {
                    name.clone()
                } else {
                    format!("{name}\n{}", badges.join(" · "))
                };
                if row(ui, t.selected, &name, &badges, Some(&tip)).clicked() && !t.selected {
                    out.push(if audio {
                        Cmd::SelectAudio(t.id)
                    } else {
                        Cmd::SelectSub(Some(t.id))
                    });
                }
            }
        });
    if !audio {
        ui.add_space(6.0);
        let text = tr!("Load from a file…", "Загрузить из файла…");
        if super::picture::icon_button(ui, icons::folder, text, None, |b| b)
            .on_hover_text(tr!(
                "Subtitles from a file of their own (SRT, ASS, …)",
                "Субтитры из отдельного файла (SRT, ASS, …)"
            ))
            .clicked()
        {
            out.push(Cmd::LoadSubtitles);
        }
    }
}

/// A row: a radio mark, the name (the language first) and under it the
/// badges (format, channels, marks), plain text on a light ground, so that
/// they read as clearly as the name. A long name is cut; badges past the
/// column are left out; `tip` holds the whole text.
fn row(ui: &mut Ui, selected: bool, name: &str, badges: &[String], tip: Option<&str>) -> Response {
    let height = if badges.is_empty() { ROW } else { ROW_TWO };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::CLICK);
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 4.0, style::selected_bg());
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, style::HOVER);
    }
    let text_color = if selected {
        style::TEXT_BRIGHT
    } else {
        style::TEXT
    };
    let left = rect.left() + 24.0;
    let right = rect.right() - 6.0;
    let name_y = if badges.is_empty() {
        rect.center().y
    } else {
        rect.top() + 13.0
    };

    let mark = pos2(rect.left() + 12.0, name_y);
    let ring = if selected {
        style::accent_hover()
    } else {
        style::TEXT_WEAK
    };
    painter.circle_stroke(mark, 5.5, Stroke::new(1.3, ring));
    if selected {
        painter.circle_filled(mark, 2.8, style::accent_hover());
    }

    let name_galley = ui.fonts_mut(|f| {
        let mut job = egui::text::LayoutJob::simple_singleline(
            name.to_owned(),
            FontId::proportional(14.5),
            text_color,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width((right - left).max(20.0));
        f.layout_job(job)
    });
    painter.galley(
        pos2(left, name_y - name_galley.size().y / 2.0),
        name_galley,
        text_color,
    );

    let mut x = left;
    let badge_y = rect.bottom() - 12.0;
    let ground = if selected {
        Color32::from_white_alpha(28)
    } else {
        Color32::from_white_alpha(18)
    };
    for badge in badges {
        let galley = ui
            .fonts_mut(|f| f.layout_no_wrap(badge.clone(), FontId::proportional(12.0), text_color));
        let size = vec2(galley.size().x + 10.0, 17.0);
        if x + size.x > right {
            break;
        }
        let r = Rect::from_min_size(pos2(x, badge_y - size.y / 2.0), size);
        painter.rect_filled(r, 3.0, ground);
        painter.galley(
            Align2::CENTER_CENTER
                .anchor_size(r.center(), galley.size())
                .min,
            galley,
            text_color,
        );
        x += size.x + 4.0;
    }
    match tip {
        Some(tip) => response.on_hover_text(tip),
        None => response,
    }
}

/// The same lists as submenus of the picture's context menu.
pub fn menus(ui: &mut Ui, all: &[Track], out: &mut Vec<Cmd>) {
    for (kind, title) in [
        (Kind::Audio, tr!("Audio", "Звук")),
        (Kind::Sub, tr!("Subtitles", "Субтитры")),
    ] {
        let list = tracks::of_kind(all, kind);
        ui.add_enabled_ui(kind == Kind::Sub || !list.is_empty(), |ui| {
            ui.menu_button(title, |ui| {
                if kind == Kind::Sub {
                    let off = tracks::selected(all, Kind::Sub).is_none();
                    if ui.radio(off, tr!("Off", "Выключены")).clicked() {
                        out.push(Cmd::SelectSub(None));
                        ui.close();
                    }
                }
                for (i, t) in list.iter().enumerate() {
                    let details = t.details_in(lang());
                    let mut text = format!("{}. {}", i + 1, t.name_in(lang(), i + 1));
                    if !details.is_empty() {
                        text = format!("{text}  ({details})");
                    }
                    if ui.radio(t.selected, text).clicked() {
                        out.push(if kind == Kind::Audio {
                            Cmd::SelectAudio(t.id)
                        } else {
                            Cmd::SelectSub(Some(t.id))
                        });
                        ui.close();
                    }
                }
                if kind == Kind::Sub {
                    ui.separator();
                    if ui
                        .button(tr!("Load from a file…", "Загрузить из файла…"))
                        .clicked()
                    {
                        out.push(Cmd::LoadSubtitles);
                        ui.close();
                    }
                }
            });
        });
    }
}

/// The tracks button's tooltip: what plays now.
pub fn summary(all: &[Track]) -> String {
    let l = lang();
    let audio = tracks::selected(all, Kind::Audio)
        .map(|t| t.name_in(l, t.number(all)))
        .unwrap_or_else(|| tr!("none", "нет").into());
    let sub = tracks::selected(all, Kind::Sub)
        .map(|t| t.name_in(l, t.number(all)))
        .unwrap_or_else(|| tr!("off", "выкл.").into());
    let title = crate::keymap::with_key(
        tr!("Audio and subtitles", "Звук и субтитры"),
        Cmd::ToggleTracks,
    );
    tr!(
        format!("{title}\nAudio: {audio} · Subtitles: {sub}"),
        format!("{title}\nЗвук: {audio} · Субтитры: {sub}")
    )
}
