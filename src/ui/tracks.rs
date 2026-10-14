//! The tracks window over the control bar's tracks button (`T`), as the
//! Video, sound and subtitles panel is (`popover.rs`: the same frame and
//! place, a press outside closes it, the keys go on): the audio tracks and
//! the subtitles as tabs, or side by side in two columns (the button at the
//! right of the tabs, `settings.tracks_wide`), each list scrolling when
//! long. A choice applies at once and the window stays open, so both
//! can be changed in one go; `T` or `Esc` closes it too. The same lists are
//! submenus of the picture's context menu.

use egui::{Align2, Color32, FontId, Rect, Response, Sense, Stroke, Ui, pos2, vec2};

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
    /// The tab shown: 0 the sound, 1 the subtitles.
    tab: usize,
}

impl TracksWindow {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Draw the window over `button` (the control bar's, where it was drawn
    /// this frame; else the player's bottom right corner) while it is open,
    /// as tabs or, with `wide`, two columns (its button turns it); the
    /// choices made.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        button: Option<Rect>,
        all: &[Track],
        wide: &mut bool,
    ) -> Vec<Cmd> {
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
        let width = if *wide { WIDTH } else { COLUMN };
        // The height there is over the button: the lists are cut to it.
        let margins = popover::frame(ctx).total_margin().sum().y;
        let limit = corner.y - ctx.content_rect().top() - popover::EDGE - margins;
        let tab = &mut self.tab;
        let shown = popover::show(ctx, "tracks_window", corner, width, |ui| {
            let top = ui.cursor().top();
            out = if *wide {
                columns(ui, all, wide, |ui| limit - (ui.cursor().top() - top))
            } else {
                tabbed(ui, all, tab, wide, |ui| limit - (ui.cursor().top() - top))
            };
        });
        self.rect = Some(shown);
        out
    }
}

/// The width of a column, and of the window as tabs.
const COLUMN: f32 = 270.0;
/// The width of a button of the head (`ui::tool`).
const TOOL: f32 = 26.0;
/// The width of the window's content: the columns and the line between.
const WIDTH: f32 = 2.0 * COLUMN + 13.0;
/// The height of a row with a name only ("Off").
const ROW: f32 = 30.0;
/// The height of a track's row: the name, the badges under it.
const ROW_TWO: f32 = 44.0;
/// A column scrolls past this height (Blu-ray rips may carry 30 subtitles),
/// or past the room the window has over its button, never under `MIN_LIST`.
const MAX_LIST: f32 = 300.0;
const MIN_LIST: f32 = 90.0;
/// The height under the subtitles' list: Load from a file.
const LOAD_ROW: f32 = 36.0;

/// The lists as tabs, the button for two columns at their right.
fn tabbed(
    ui: &mut Ui,
    all: &[Track],
    tab: &mut usize,
    wide: &mut bool,
    room: impl Fn(&Ui) -> f32,
) -> Vec<Cmd> {
    let mut out = Vec::new();
    ui.set_width(COLUMN);
    ui.horizontal(|ui| {
        let labels = [tr!("Audio", "Звук"), tr!("Subtitles", "Субтитры")];
        popover::tabs(ui, &labels, tab, COLUMN - TOOL - 8.0, "tracks_tab");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tip = tr!(
                "Two columns: the sound and the subtitles side by side",
                "Две колонки: звук и субтитры рядом"
            );
            if super::tool(ui, tip, true, |p, o, c| icons::columns(p, o, 6.5, c, true)).clicked() {
                *wide = true;
            }
        });
    });
    ui.add_space(6.0);
    let room = room(ui);
    // Both lists laid out unseen (an invisible Ui paints nothing and takes
    // no input) for the taller: the window keeps that height whichever tab
    // is shown, as the panel does; held by its bottom edge, it jumped.
    let mut tallest = 0.0_f32;
    for kind in [Kind::Audio, Kind::Sub] {
        let rect = ui.available_rect_before_wrap();
        let mut unseen = ui.new_child(egui::UiBuilder::new().max_rect(rect).invisible());
        column(&mut unseen, all, kind, room, &mut Vec::new());
        tallest = tallest.max(unseen.min_rect().height());
    }
    let kind = if *tab == 0 { Kind::Audio } else { Kind::Sub };
    ui.scope(|ui| {
        ui.set_min_height(tallest);
        column(ui, all, kind, room, &mut out);
    });
    out
}

/// The lists side by side, under titles drawn as tabs shown, the button
/// back to tabs at the right of the second.
fn columns(ui: &mut Ui, all: &[Track], wide: &mut bool, room: impl Fn(&Ui) -> f32) -> Vec<Cmd> {
    let mut out = Vec::new();
    ui.set_width(WIDTH);
    ui.horizontal(|ui| {
        title_as_tab(ui, tr!("Audio", "Звук"), COLUMN);
        ui.add_space(13.0 - ui.spacing().item_spacing.x);
        title_as_tab(ui, tr!("Subtitles", "Субтитры"), COLUMN - TOOL - 8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tip = tr!("Back to tabs", "Обратно во вкладки");
            if super::tool(ui, tip, true, |p, o, c| icons::columns(p, o, 6.5, c, false)).clicked() {
                *wide = false;
            }
        });
    });
    ui.add_space(6.0);
    let room = room(ui);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            column(ui, all, Kind::Audio, room, &mut out);
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
            column(ui, all, Kind::Sub, room, &mut out);
        });
    });
    out
}

/// A list of `kind`, the column `room` high at most.
fn column(ui: &mut Ui, all: &[Track], kind: Kind, room: f32, out: &mut Vec<Cmd>) {
    let audio = kind == Kind::Audio;
    let list = tracks::of_kind(all, kind);
    let below = if audio { 0.0 } else { LOAD_ROW };
    egui::ScrollArea::vertical()
        .id_salt(("tracks", audio))
        .max_height((room - below).clamp(MIN_LIST, MAX_LIST))
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

/// A column's title drawn as a tab shown, `width` wide: centred over a
/// line, underlined in the accent colour.
fn title_as_tab(ui: &mut Ui, title: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    let painter = ui.painter();
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, style::LINE),
    );
    painter.text(
        rect.center() - vec2(0.0, 1.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(14.0),
        style::TEXT_BRIGHT,
    );
    // As a tab's: the same width of underline whatever the title's.
    let tab = Rect::from_center_size(rect.center(), vec2(rect.width() / 2.0, rect.height()));
    popover::underline(painter, tab);
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
