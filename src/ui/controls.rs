//! The control bar under the picture. In full: the seek bar across the
//! whole width (with marks at the chapters' starts), below it the time on
//! the left, the buttons in the middle with Play in their centre, the
//! volume on the right. Compact: one row, Play, the time and the seek bar
//! from the left, the rest on the right.

use egui::{Align2, Color32, FontId, Rect, Response, Sense, Ui, pos2, vec2};

use super::{icons, style};
use crate::input::Cmd;
use crate::keymap::with_key;
use crate::player::State;

/// What the bar shows besides the player's state.
pub struct Bar<'a> {
    pub state: &'a State,
    pub playlist_visible: bool,
    /// The Video, sound and subtitles panel is open (its button underlined).
    pub video_panel_open: bool,
    /// The tracks window is open (its button underlined).
    pub tracks_open: bool,
    /// Where the tracks button is drawn, up to the bar's top: the window
    /// stands over it.
    pub tracks_button: &'a mut Option<Rect>,
    pub fullscreen: bool,
    /// One row instead of two (`Interface::Compact`).
    pub compact: bool,
    pub has_prev: bool,
    pub has_next: bool,
    /// Marks at the chapters' starts on the seek bar.
    pub chapter_marks: bool,
    /// What the time left of the buttons shows.
    pub time_mode: crate::format::TimeMode,
    /// The seek bar is being dragged: the position under the pointer.
    pub drag: &'a mut Option<f64>,
    /// Where the Video, sound and subtitles panel's button is drawn: the
    /// panel stands over it.
    pub panel_button: &'a mut Option<Rect>,
}

/// The size of the small buttons.
const SMALL: f32 = 34.0;
/// The size of the round Play in the full bar.
const PLAY: f32 = 42.0;
const GAP: f32 = 2.0;
/// The size of the compact bar's buttons, and of its row.
const COMPACT: f32 = 30.0;
const COMPACT_ROW: f32 = 36.0;
/// The space between the seek bar and the row of buttons in the full bar.
const ROWS_GAP: f32 = 3.0;
/// The size of the icons in the buttons.
const ICON: f32 = 7.0;

/// The frame of the bar under the picture: a line along its edges, a little
/// room above and below the full bar's rows.
pub fn frame(compact: bool) -> egui::Frame {
    let (top, bottom) = if compact { (0, 0) } else { (4, 2) };
    egui::Frame::NONE
        .fill(style::BAR_BG)
        .stroke(egui::Stroke::new(1.0, style::LINE))
        .inner_margin(egui::Margin {
            left: 0,
            right: 0,
            top,
            bottom,
        })
}

/// The height of the bar under the picture in points, its frame
/// included: its panel is given it exactly, as the window grows or shrinks
/// by it when the interface changes.
pub fn height(compact: bool) -> f32 {
    let rows = if compact {
        COMPACT_ROW
    } else {
        style::SEEK_HEIGHT + ROWS_GAP + style::BUTTONS_HEIGHT
    };
    rows + frame(compact).total_margin().sum().y
}

/// Draw the bar in `ui` (its full width); the commands of its buttons.
pub fn bar(ui: &mut Ui, bar: Bar<'_>) -> Vec<Cmd> {
    // The free space between the controls moves the window.
    let free = ui.max_rect();
    crate::frameless::drag_area(ui, free, ui.id().with("bar_drag"), Sense::hover());
    if bar.compact {
        compact(ui, bar)
    } else {
        full(ui, bar)
    }
}

fn full(ui: &mut Ui, mut bar: Bar<'_>) -> Vec<Cmd> {
    let mut out = Vec::new();
    let width = ui.available_width();
    let top = ui.max_rect().left_top();
    let seek_rect = Rect::from_min_size(top, vec2(width, style::SEEK_HEIGHT));
    let row = Rect::from_min_size(
        pos2(top.x, seek_rect.bottom() + ROWS_GAP),
        vec2(width, style::BUTTONS_HEIGHT),
    );
    ui.allocate_rect(seek_rect.union(row), Sense::hover());
    if let Some(cmd) = seek_bar(
        ui,
        seek_rect.shrink2(vec2(10.0, 0.0)),
        bar.state,
        bar.chapter_marks,
        bar.drag,
    ) {
        out.push(cmd);
    }

    // The buttons, centred on the row.
    let group_width = 6.0 * SMALL + PLAY + 6.0 * GAP + 16.0;
    let group = Rect::from_center_size(row.center(), vec2(group_width, PLAY));
    let mut x = group.left();
    let cy = row.center().y;
    let slot = |x: &mut f32, w: f32| {
        let r = Rect::from_min_size(pos2(*x, cy - w / 2.0), vec2(w, w));
        *x += w + GAP;
        r
    };
    let state = bar.state;
    if flat(
        ui,
        slot(&mut x, SMALL),
        &with_key(tr!("Open a file", "Открыть файл"), Cmd::Open),
        true,
        |p, o, c, _| icons::eject(p, o, ICON, c),
    )
    .clicked()
    {
        out.push(Cmd::Open);
    }
    if flat(
        ui,
        slot(&mut x, SMALL),
        tr!("Stop", "Стоп"),
        !state.idle,
        |p, o, c, _| icons::stop(p, o, ICON * 0.85, c),
    )
    .clicked()
    {
        out.push(Cmd::Stop);
    }
    x += 8.0;
    if skip_button(ui, slot(&mut x, SMALL), false, bar.has_prev) {
        out.push(Cmd::Prev);
    }
    if play_button(ui, slot(&mut x, PLAY), state, 9.0).clicked() {
        out.push(Cmd::TogglePause);
    }
    if skip_button(ui, slot(&mut x, SMALL), true, bar.has_next) {
        out.push(Cmd::Next);
    }
    x += 8.0;
    if settings_button(ui, slot(&mut x, SMALL)) {
        out.push(Cmd::Settings);
    }
    if playlist_button(ui, slot(&mut x, SMALL), bar.playlist_visible) {
        out.push(Cmd::TogglePlaylist);
    }

    // The time, left of the buttons (left out when there is no room).
    let time_rect = Rect::from_min_max(
        pos2(row.left() + 12.0, row.top()),
        pos2(group.left() - 8.0, row.bottom()),
    );
    if time_rect.width() > 60.0 {
        let galley = time_galley(ui, &bar);
        if time(ui, time_rect, galley) {
            out.push(Cmd::CycleTime);
        }
    }

    // The volume and fullscreen, right of the buttons.
    let right = row.right() - 10.0;
    let fs = Rect::from_center_size(pos2(right - SMALL / 2.0, cy), vec2(SMALL, SMALL));
    if fullscreen_button(ui, fs, bar.fullscreen) {
        out.push(Cmd::ToggleFullscreen);
    }
    let slider = Rect::from_min_max(
        pos2(fs.left() - 8.0 - 80.0, cy - 7.0),
        pos2(fs.left() - 8.0, cy + 7.0),
    );
    let speaker = Rect::from_center_size(
        pos2(slider.left() - 4.0 - SMALL / 2.0, cy),
        vec2(SMALL, SMALL),
    );
    if speaker.left() > group.right() + 4.0 {
        if speaker_button(ui, speaker, state) {
            out.push(Cmd::ToggleMute);
        }
        if let Some(cmd) = volume_slider(ui, slider, state) {
            out.push(cmd);
        }
    }

    // Audio and subtitles, left of the volume: one button, both lists
    // in the window over it. The Video, sound and subtitles panel's
    // button left of it.
    let tracks_rect = Rect::from_center_size(
        pos2(speaker.left() - 6.0 - SMALL / 2.0, cy),
        vec2(SMALL, SMALL),
    );
    if tracks_rect.left() > group.right() + 4.0 && tracks_button(ui, tracks_rect, &mut bar) {
        out.push(Cmd::ToggleTracks);
    }
    let panel_rect = tracks_rect.translate(vec2(-(SMALL + GAP), 0.0));
    if panel_rect.left() > group.right() + 4.0 && panel_button(ui, panel_rect, &mut bar) {
        out.push(Cmd::ToggleVideoPanel);
    }
    out
}

/// One row: Play, previous and next, the time, the seek bar taking what is
/// left, the volume, the panels' buttons, the playlist, the settings and
/// fullscreen. Open and Stop are left to the keys and the context menu; in
/// a narrow window the settings, the volume's bar, previous and next, the
/// panel, the tracks and the time go in turn, so that the seek bar keeps
/// [`MIN_SEEK`].
fn compact(ui: &mut Ui, mut bar: Bar<'_>) -> Vec<Cmd> {
    const MIN_SEEK: f32 = 120.0;
    const SLIDER: f32 = 64.0;
    const PAD: f32 = 6.0;
    let mut out = Vec::new();
    let row = Rect::from_min_size(
        ui.max_rect().left_top(),
        vec2(ui.available_width(), COMPACT_ROW),
    );
    ui.allocate_rect(row, Sense::hover());
    let cy = row.center().y;
    let state = bar.state;
    let galley = time_galley(ui, &bar);
    let time_width = galley.size().x + 12.0;

    // What may be left out, in that order, and the room each takes.
    let optional = [
        COMPACT + GAP,         // settings
        SLIDER + 4.0,          // the volume's bar
        2.0 * (COMPACT + GAP), // previous and next
        COMPACT + GAP,         // the panel
        COMPACT + GAP,         // the tracks
        time_width,            // the time
    ];
    // Play, the speaker, the playlist, fullscreen, the seek bar's margins.
    let fixed = 2.0 * PAD + 4.0 * (COMPACT + GAP) + 6.0 + 2.0 * 8.0;
    let mut shown = [true; 6];
    let mut need = fixed + optional.iter().sum::<f32>();
    for (i, room) in optional.iter().enumerate() {
        if need + MIN_SEEK <= row.width() {
            break;
        }
        need -= room;
        shown[i] = false;
    }
    let [settings, slider, skip, panel, tracks, time_shown] = shown;
    let square = |x: f32| Rect::from_min_size(pos2(x, cy - COMPACT / 2.0), vec2(COMPACT, COMPACT));

    // From the left.
    let mut x = row.left() + PAD;
    if play_button(ui, square(x).shrink(1.0), state, 7.0).clicked() {
        out.push(Cmd::TogglePause);
    }
    x += COMPACT + GAP;
    if skip {
        if skip_button(ui, square(x), false, bar.has_prev) {
            out.push(Cmd::Prev);
        }
        x += COMPACT + GAP;
        if skip_button(ui, square(x), true, bar.has_next) {
            out.push(Cmd::Next);
        }
        x += COMPACT + GAP;
    }
    if time_shown {
        let area = Rect::from_min_max(pos2(x + 6.0, row.top()), pos2(x + time_width, row.bottom()));
        if time(ui, area, galley) {
            out.push(Cmd::CycleTime);
        }
        x += time_width;
    }

    // From the right.
    let mut right = row.right() - PAD;
    let next = |right: &mut f32| {
        *right -= COMPACT;
        let r = square(*right);
        *right -= GAP;
        r
    };
    if fullscreen_button(ui, next(&mut right), bar.fullscreen) {
        out.push(Cmd::ToggleFullscreen);
    }
    if settings && settings_button(ui, next(&mut right)) {
        out.push(Cmd::Settings);
    }
    if playlist_button(ui, next(&mut right), bar.playlist_visible) {
        out.push(Cmd::TogglePlaylist);
    }
    if tracks && tracks_button(ui, next(&mut right), &mut bar) {
        out.push(Cmd::ToggleTracks);
    }
    if panel && panel_button(ui, next(&mut right), &mut bar) {
        out.push(Cmd::ToggleVideoPanel);
    }
    right -= 6.0;
    if slider {
        let r = Rect::from_min_max(pos2(right - SLIDER, cy - 7.0), pos2(right, cy + 7.0));
        if let Some(cmd) = volume_slider(ui, r, state) {
            out.push(cmd);
        }
        right -= SLIDER + 4.0;
    }
    if speaker_button(ui, next(&mut right), state) {
        out.push(Cmd::ToggleMute);
    }

    // The seek bar between them, as tall as the row (easier to hit).
    let seek = Rect::from_min_max(pos2(x + 8.0, row.top()), pos2(right - 8.0, row.bottom()));
    if seek.width() > 0.0
        && let Some(cmd) = seek_bar(ui, seek, state, bar.chapter_marks, bar.drag)
    {
        out.push(cmd);
    }
    out
}

/// The height of the mini player's row over the picture's bottom.
pub const MINI_ROW: f32 = 36.0;

/// The mini player's controls over the picture in `rect`: along its bottom
/// Play, the speaker and the volume, the seek bar where there is room; the
/// player's window again in the top right corner. The row's
/// free space moves the window, as the picture does.
pub fn mini(ui: &mut Ui, rect: Rect, state: &State, drag: &mut Option<f64>) -> Vec<Cmd> {
    const BUTTON: f32 = 28.0;
    const SLIDER: f32 = 56.0;
    const PAD: f32 = 4.0;
    const MIN_SEEK: f32 = 40.0;
    let mut out = Vec::new();
    let row = Rect::from_min_max(pos2(rect.left(), rect.bottom() - MINI_ROW), rect.max);
    ui.painter()
        .rect_filled(row, 0.0, style::BAR_BG.gamma_multiply_u8(220));
    crate::frameless::drag_area(ui, row, ui.id().with("mini_drag"), Sense::hover());
    let cy = row.center().y;
    let square = |x: f32| Rect::from_min_size(pos2(x, cy - BUTTON / 2.0), vec2(BUTTON, BUTTON));

    let mut x = row.left() + PAD;
    if play_button(ui, square(x).shrink(1.0), state, 6.5).clicked() {
        out.push(Cmd::TogglePause);
    }
    x += BUTTON + GAP;
    if speaker_button(ui, square(x), state) {
        out.push(Cmd::ToggleMute);
    }
    x += BUTTON + 2.0;
    let slider = Rect::from_min_max(pos2(x, cy - 7.0), pos2(x + SLIDER, cy + 7.0));
    if let Some(cmd) = volume_slider(ui, slider, state) {
        out.push(cmd);
    }
    x += SLIDER;

    let seek = Rect::from_min_max(
        pos2(x + 10.0, row.top()),
        pos2(row.right() - 12.0, row.bottom()),
    );
    if seek.width() >= MIN_SEEK
        && let Some(cmd) = seek_bar(ui, seek, state, false, drag)
    {
        out.push(cmd);
    }

    // The way back, on a ground of its own over the picture.
    let back = Rect::from_min_size(
        pos2(rect.right() - PAD - BUTTON, rect.top() + PAD),
        vec2(BUTTON, BUTTON),
    );
    ui.painter()
        .rect_filled(back.shrink(1.0), 5.0, style::BAR_BG.gamma_multiply_u8(220));
    let tip = with_key(tr!("The player's window", "Окно плеера"), Cmd::ToggleMini);
    if flat(ui, back, &tip, true, |p, o, c, _| {
        icons::mini(p, o, ICON, c)
    })
    .clicked()
    {
        out.push(Cmd::ToggleMini);
    }
    out
}

/// The time as the bar shows it; its colour is chosen once the pointer is
/// known, painted with the galley.
fn time_galley(ui: &Ui, bar: &Bar<'_>) -> std::sync::Arc<egui::Galley> {
    let text = crate::format::time_line(bar.time_mode, bar.state.time, bar.state.duration);
    ui.painter()
        .layout_no_wrap(text, FontId::monospace(12.0), Color32::PLACEHOLDER)
}

/// The time at the left of `area`, cut to it; a click on it switches
/// between both times, the elapsed and the remaining one (true).
fn time(ui: &mut Ui, area: Rect, galley: std::sync::Arc<egui::Galley>) -> bool {
    let text_rect = Align2::LEFT_CENTER.anchor_size(area.left_center(), galley.size());
    let response = ui
        .interact(
            text_rect.expand2(vec2(6.0, 4.0)),
            ui.id().with("time"),
            Sense::CLICK,
        )
        .on_hover_text(tr!(
            "Elapsed / total, elapsed or remaining time (click)",
            "Прошло / всего, прошло или осталось (щелчок)"
        ));
    let color = if response.hovered() {
        style::TEXT_BRIGHT
    } else {
        style::TEXT_WEAK
    };
    ui.painter_at(area).galley(text_rect.min, galley, color);
    response.clicked()
}

/// Previous (`next` false) or next file.
fn skip_button(ui: &mut Ui, rect: Rect, next: bool, enabled: bool) -> bool {
    let tip = if next {
        with_key(tr!("Next file", "Следующий файл"), Cmd::Next)
    } else {
        with_key(tr!("Previous file", "Предыдущий файл"), Cmd::Prev)
    };
    flat(ui, rect, &tip, enabled, |p, o, c, _| {
        icons::skip(p, o, ICON, c, next)
    })
    .clicked()
}

fn settings_button(ui: &mut Ui, rect: Rect) -> bool {
    flat(
        ui,
        rect,
        &with_key(tr!("Settings", "Настройки"), Cmd::Settings),
        true,
        |p, o, c, fill| icons::gear(p, o, ICON, c, fill),
    )
    .clicked()
}

fn playlist_button(ui: &mut Ui, rect: Rect, visible: bool) -> bool {
    let tip = with_key(tr!("Playlist", "Плейлист"), Cmd::TogglePlaylist);
    let list = flat(ui, rect, &tip, true, |p, o, c, _| {
        icons::list(p, o, ICON, c)
    });
    if visible {
        underline(ui, list.rect);
    }
    list.clicked()
}

fn fullscreen_button(ui: &mut Ui, rect: Rect, fullscreen: bool) -> bool {
    let tip = if fullscreen {
        // The keys as they are now: either may have been changed.
        let keys: Vec<String> = [Cmd::ToggleFullscreen, Cmd::Escape]
            .into_iter()
            .map(crate::keymap::shortcut)
            .filter(|k| !k.is_empty())
            .collect();
        let text = tr!("Leave fullscreen", "Выйти из полноэкранного режима");
        if keys.is_empty() {
            text.to_owned()
        } else {
            format!("{text} ({})", keys.join(", "))
        }
    } else {
        with_key(tr!("Fullscreen", "Полный экран"), Cmd::ToggleFullscreen)
    };
    flat(ui, rect, &tip, true, |p, o, c, _| {
        icons::fullscreen(p, o, ICON, c, fullscreen)
    })
    .clicked()
}

/// Mute, or the sound on again.
fn speaker_button(ui: &mut Ui, rect: Rect, state: &State) -> bool {
    let level = if state.mute {
        0.0
    } else {
        (state.volume / 100.0).min(1.0)
    };
    let tip = if state.mute {
        tr!("Sound on", "Включить звук")
    } else {
        tr!("Mute", "Без звука")
    };
    let tip = with_key(tip, Cmd::ToggleMute);
    flat(ui, rect, &tip, true, |p, o, c, _| {
        icons::speaker(p, o, ICON, c, level, state.mute)
    })
    .clicked()
}

/// Where a window over a button of the bar stands: the button's column up
/// to the bar's top, so that the window leaves the seek bar of the full
/// bar (over the buttons' row) uncovered.
fn anchor(ui: &Ui, button: Rect) -> Rect {
    button.with_min_y(ui.max_rect().top().min(button.top()))
}

/// Audio and subtitles: one button, both lists in the window over it.
fn tracks_button(ui: &mut Ui, rect: Rect, bar: &mut Bar<'_>) -> bool {
    let state = bar.state;
    let any = !state.idle && !state.tracks.is_empty();
    let tip = super::tracks::summary(&state.tracks);
    let button = flat(ui, rect, &tip, any, |p, o, c, _| {
        icons::tracks(p, o, ICON, c)
    });
    if bar.tracks_open {
        underline(ui, button.rect);
    }
    *bar.tracks_button = Some(anchor(ui, button.rect));
    button.clicked()
}

/// The Video, sound and subtitles panel.
fn panel_button(ui: &mut Ui, rect: Rect, bar: &mut Bar<'_>) -> bool {
    let panel = flat(
        ui,
        rect,
        &with_key(
            tr!("Video, sound and subtitles", "Видео, звук и субтитры"),
            Cmd::ToggleVideoPanel,
        ),
        true,
        |p, o, c, _| icons::sliders(p, o, ICON, c),
    );
    if bar.video_panel_open {
        underline(ui, panel.rect);
    }
    *bar.panel_button = Some(anchor(ui, panel.rect));
    panel.clicked()
}

/// The accent line under a button whose panel is shown.
fn underline(ui: &Ui, r: Rect) {
    ui.painter().line_segment(
        [
            pos2(r.center().x - 6.0, r.bottom() - 3.0),
            pos2(r.center().x + 6.0, r.bottom() - 3.0),
        ],
        egui::Stroke::new(2.0, style::accent()),
    );
}

/// A button with no frame: an icon painted by `paint(painter, centre,
/// colour, background)`, a background while hovered. Not focusable, so
/// Space and Enter stay the player's.
fn flat(
    ui: &mut Ui,
    rect: Rect,
    tip: &str,
    enabled: bool,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, Color32, Color32),
) -> Response {
    let sense = if enabled {
        Sense::CLICK
    } else {
        Sense::hover()
    };
    let response = ui.allocate_rect(rect, sense);
    let (fill, color) = if !enabled {
        (style::BAR_BG, style::TEXT_WEAK.gamma_multiply(0.5))
    } else if response.is_pointer_button_down_on() {
        (style::PRESSED, style::TEXT_BRIGHT)
    } else if response.hovered() {
        (style::HOVER, style::TEXT_BRIGHT)
    } else {
        (style::BAR_BG, style::TEXT)
    };
    if fill != style::BAR_BG {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, fill);
    }
    paint(ui.painter(), rect.center(), color, fill);
    response.on_hover_text(tip)
}

/// The round Play/Pause button, its icon of the size `icon`.
fn play_button(ui: &mut Ui, rect: Rect, state: &State, icon: f32) -> Response {
    let response = ui.allocate_rect(rect, Sense::CLICK);
    let fill = if response.is_pointer_button_down_on() {
        style::accent()
    } else if response.hovered() {
        style::accent_hover()
    } else {
        style::accent()
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    painter.circle_stroke(
        rect.center(),
        rect.width() / 2.0 - 0.5,
        egui::Stroke::new(1.0, Color32::from_white_alpha(40)),
    );
    let playing = !state.idle && !state.paused && !state.eof;
    if playing {
        icons::pause(painter, rect.center(), icon, style::TEXT_BRIGHT);
    } else {
        icons::play(painter, rect.center(), icon, style::TEXT_BRIGHT);
    }
    let tip = if playing {
        tr!("Pause", "Пауза")
    } else {
        tr!("Play", "Воспроизвести")
    };
    response.on_hover_text(with_key(tip, Cmd::TogglePause))
}

/// The seek bar: a click or a drag moves there (on a keyframe; the exact
/// frame when the button is let go). The position under the pointer and
/// its chapter are shown while hovering; `marks` cuts the bar at the
/// chapters' starts.
fn seek_bar(
    ui: &mut Ui,
    rect: Rect,
    state: &State,
    marks: bool,
    drag: &mut Option<f64>,
) -> Option<Cmd> {
    let duration = state.duration.filter(|_| !state.idle);
    let sense = if duration.is_some() {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, ui.id().with("seek_bar"), sense);
    let track = Rect::from_center_size(
        rect.center(),
        vec2(rect.width(), if response.hovered() { 6.0 } else { 4.0 }),
    );
    let painter = ui.painter();
    painter.rect_filled(track, 3.0, style::TRACK);
    let pointer_fraction = response
        .hover_pos()
        .or(response.interact_pointer_pos())
        .map(|p| ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64);
    let mut cmd = None;
    if let Some(d) = duration {
        if response.dragged() || response.drag_started() {
            if let Some(f) = pointer_fraction {
                if drag.is_none_or(|old| (old - f).abs() * d > 0.25) {
                    cmd = Some(Cmd::SeekTo {
                        seconds: f * d,
                        exact: false,
                    });
                }
                *drag = Some(f);
            }
        } else if response.drag_stopped() {
            if let Some(f) = drag.take() {
                cmd = Some(Cmd::SeekTo {
                    seconds: f * d,
                    exact: true,
                });
            }
        } else if response.clicked()
            && let Some(f) = pointer_fraction
        {
            cmd = Some(Cmd::SeekTo {
                seconds: f * d,
                exact: false,
            });
        }
    } else {
        *drag = None;
    }
    let fraction = drag.or_else(|| state.fraction()).unwrap_or(0.0) as f32;
    if fraction > 0.0 {
        let done = Rect::from_min_max(
            track.left_top(),
            pos2(track.left() + track.width() * fraction, track.bottom()),
        );
        painter.rect_filled(done, 3.0, style::accent());
    }
    // The A-B repeat: the stretch between its points lighter, the points
    // as ticks across the bar.
    if let Some(d) = duration.filter(|d| *d > 0.0) {
        let x = |t: f64| track.left() + track.width() * (t / d).clamp(0.0, 1.0) as f32;
        if let (Some(a), Some(b)) = (state.loop_a, state.loop_b) {
            let stretch = Rect::from_min_max(pos2(x(a), track.top()), pos2(x(b), track.bottom()));
            painter.rect_filled(stretch, 0.0, style::LOOP.gamma_multiply(0.45));
        }
        for t in [state.loop_a, state.loop_b].into_iter().flatten() {
            let tick = Rect::from_center_size(
                pos2(x(t), track.center().y),
                vec2(2.0, track.height() + 8.0),
            );
            painter.rect_filled(tick, 1.0, style::LOOP);
        }
    }
    // The chapters' starts as gaps in the bar (not the first, at the
    // start, nor one at the very end).
    if marks && let Some(d) = duration {
        for c in state
            .chapters
            .iter()
            .filter(|c| c.time > 0.5 && c.time < d - 0.5)
        {
            let x = track.left() + track.width() * (c.time / d) as f32;
            let gap = Rect::from_center_size(pos2(x, track.center().y), vec2(2.0, track.height()));
            painter.rect_filled(gap, 0.0, style::BAR_BG);
        }
    }
    if duration.is_some() && (response.hovered() || drag.is_some()) {
        let x = track.left() + track.width() * fraction;
        painter.circle_filled(pos2(x, track.center().y), 6.0, style::TEXT_BRIGHT);
        if let (Some(d), Some(f)) = (duration, pointer_fraction) {
            let mut text = crate::format::time(Some(f * d), d >= 3600.0);
            if let Some(i) = crate::chapters::current(&state.chapters, f * d) {
                text = format!("{text} · {}", state.chapters[i].name(i + 1));
            }
            // At once, without the tooltip delay: it follows the pointer
            // along the bar, as a time readout.
            egui::Tooltip::for_widget(&response)
                .at_pointer()
                .show(|ui| ui.add(egui::Label::new(text).extend()));
        }
    }
    cmd
}

/// The volume: a short bar set by a click or a drag, or by the wheel; it
/// reaches `volume_max`, with a tick at 100 % when that is boosted.
fn volume_slider(ui: &mut Ui, rect: Rect, state: &State) -> Option<Cmd> {
    let response = ui.interact(rect, ui.id().with("volume"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    let painter = ui.painter();
    painter.rect_filled(track, 2.0, style::TRACK);
    let max = state.volume_max.max(100.0);
    let level = if state.mute {
        0.0
    } else {
        (state.volume / max) as f32
    };
    let done = Rect::from_min_max(
        track.left_top(),
        pos2(track.left() + track.width() * level, track.bottom()),
    );
    painter.rect_filled(
        done,
        2.0,
        if state.mute {
            style::TEXT_WEAK
        } else {
            style::accent()
        },
    );
    if max > 100.0 {
        let x = track.left() + track.width() * (100.0 / max) as f32;
        painter.line_segment(
            [pos2(x, track.top() - 2.0), pos2(x, track.bottom() + 2.0)],
            egui::Stroke::new(1.0, style::TEXT_WEAK),
        );
    }
    if response.hovered() || response.dragged() {
        painter.circle_filled(
            pos2(done.right(), track.center().y),
            5.0,
            style::TEXT_BRIGHT,
        );
    }
    let mut cmd = None;
    if (response.dragged() || response.clicked())
        && let Some(p) = response.interact_pointer_pos()
    {
        let v = (((p.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * max).round();
        cmd = Some(Cmd::SetVolume(v));
    }
    if response.hovered() {
        let notches = super::wheel_notches(ui);
        if notches != 0 {
            cmd = Some(Cmd::SetVolume(
                (state.volume + notches as f64 * crate::input::VOLUME_STEP).clamp(0.0, max),
            ));
        }
    }
    response.on_hover_text(tr!(
        format!("Volume {:.0}%", state.volume),
        format!("Громкость {:.0}%", state.volume)
    ));
    cmd
}
