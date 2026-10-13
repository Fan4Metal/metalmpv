//! The control bar under the picture: the seek bar across the whole
//! width (with marks at the chapters' starts), below it the time on the left, the buttons
//! in the middle with Play in their centre, the volume on the right.

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
    pub fullscreen: bool,
    pub has_prev: bool,
    pub has_next: bool,
    /// Marks at the chapters' starts on the seek bar.
    pub chapter_marks: bool,
    /// What the time left of the buttons shows.
    pub time_mode: crate::format::TimeMode,
    /// The seek bar is being dragged: the position under the pointer.
    pub drag: &'a mut Option<f64>,
}

/// Draw the bar in `ui` (its full width); the commands of its buttons.
pub fn bar(ui: &mut Ui, bar: Bar<'_>) -> Vec<Cmd> {
    let mut out = Vec::new();
    // The free space between the controls moves the window.
    let free = ui.max_rect();
    crate::frameless::drag_area(ui, free, ui.id().with("bar_drag"), Sense::hover());
    let width = ui.available_width();
    let (seek_rect, _) = ui.allocate_exact_size(vec2(width, style::SEEK_HEIGHT), Sense::hover());
    if let Some(cmd) = seek_bar(
        ui,
        seek_rect.shrink2(vec2(10.0, 0.0)),
        bar.state,
        bar.chapter_marks,
        bar.drag,
    ) {
        out.push(cmd);
    }
    let (row, _) = ui.allocate_exact_size(vec2(width, style::BUTTONS_HEIGHT), Sense::hover());

    // The buttons, centred on the row.
    const SMALL: f32 = 34.0;
    const PLAY: f32 = 42.0;
    const GAP: f32 = 2.0;
    let group_width = 6.0 * SMALL + PLAY + 6.0 * GAP + 16.0;
    let group = Rect::from_center_size(row.center(), vec2(group_width, PLAY));
    let mut x = group.left();
    let cy = row.center().y;
    let slot = |x: &mut f32, w: f32| {
        let r = Rect::from_min_size(pos2(*x, cy - w / 2.0), vec2(w, w));
        *x += w + GAP;
        r
    };
    let s = 7.0;
    let state = bar.state;
    if flat(
        ui,
        slot(&mut x, SMALL),
        &with_key(tr!("Open a file", "Открыть файл"), Cmd::Open),
        true,
        |p, o, c, _| icons::eject(p, o, s, c),
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
        |p, o, c, _| icons::stop(p, o, s * 0.85, c),
    )
    .clicked()
    {
        out.push(Cmd::Stop);
    }
    x += 8.0;
    if flat(
        ui,
        slot(&mut x, SMALL),
        &with_key(tr!("Previous file", "Предыдущий файл"), Cmd::Prev),
        bar.has_prev,
        |p, o, c, _| icons::skip(p, o, s, c, false),
    )
    .clicked()
    {
        out.push(Cmd::Prev);
    }
    if play_button(ui, slot(&mut x, PLAY), state).clicked() {
        out.push(Cmd::TogglePause);
    }
    if flat(
        ui,
        slot(&mut x, SMALL),
        &with_key(tr!("Next file", "Следующий файл"), Cmd::Next),
        bar.has_next,
        |p, o, c, _| icons::skip(p, o, s, c, true),
    )
    .clicked()
    {
        out.push(Cmd::Next);
    }
    x += 8.0;
    if flat(
        ui,
        slot(&mut x, SMALL),
        &with_key(tr!("Settings", "Настройки"), Cmd::Settings),
        true,
        |p, o, c, fill| icons::gear(p, o, s, c, fill),
    )
    .clicked()
    {
        out.push(Cmd::Settings);
    }
    let list_tip = with_key(tr!("Playlist", "Плейлист"), Cmd::TogglePlaylist);
    let list = flat(ui, slot(&mut x, SMALL), &list_tip, true, |p, o, c, _| {
        icons::list(p, o, s, c)
    });
    if bar.playlist_visible {
        underline(ui, list.rect);
    }
    if list.clicked() {
        out.push(Cmd::TogglePlaylist);
    }

    // The time, left of the buttons (left out when there is no room); a
    // click on it switches between both times, the elapsed and the
    // remaining one.
    let time_rect = Rect::from_min_max(
        pos2(row.left() + 12.0, row.top()),
        pos2(group.left() - 8.0, row.bottom()),
    );
    if time_rect.width() > 60.0 {
        let text = crate::format::time_line(bar.time_mode, state.time, state.duration);
        // The colour is chosen once the pointer is known: painted with the galley.
        let galley =
            ui.painter()
                .layout_no_wrap(text, FontId::monospace(12.0), Color32::PLACEHOLDER);
        let text_rect = Align2::LEFT_CENTER.anchor_size(time_rect.left_center(), galley.size());
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
        ui.painter_at(time_rect)
            .galley(text_rect.min, galley, color);
        if response.clicked() {
            out.push(Cmd::CycleTime);
        }
    }

    // The volume and fullscreen, right of the buttons.
    let right = row.right() - 10.0;
    let fs = Rect::from_center_size(
        pos2(right - SMALL / 2.0, row.center().y),
        vec2(SMALL, SMALL),
    );
    let fs_tip = if bar.fullscreen {
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
    if flat(ui, fs, &fs_tip, true, |p, o, c, _| {
        icons::fullscreen(p, o, s, c, bar.fullscreen)
    })
    .clicked()
    {
        out.push(Cmd::ToggleFullscreen);
    }
    let slider = Rect::from_min_max(
        pos2(fs.left() - 8.0 - 80.0, row.center().y - 7.0),
        pos2(fs.left() - 8.0, row.center().y + 7.0),
    );
    let speaker = Rect::from_center_size(
        pos2(slider.left() - 4.0 - SMALL / 2.0, row.center().y),
        vec2(SMALL, SMALL),
    );
    if speaker.left() > group.right() + 4.0 {
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
        if flat(ui, speaker, &tip, true, |p, o, c, _| {
            icons::speaker(p, o, s, c, level, state.mute)
        })
        .clicked()
        {
            out.push(Cmd::ToggleMute);
        }
        if let Some(cmd) = volume_slider(ui, slider, state) {
            out.push(cmd);
        }
    }

    // Audio and subtitles, left of the volume: one button, both lists
    // side by side in the window over it. The Video, sound and subtitles panel's
    // button left of it.
    let tracks_rect = Rect::from_center_size(
        pos2(speaker.left() - 6.0 - SMALL / 2.0, row.center().y),
        vec2(SMALL, SMALL),
    );
    if tracks_rect.left() > group.right() + 4.0 {
        let any = !state.idle && !state.tracks.is_empty();
        let tip = super::tracks::summary(&state.tracks);
        let button = flat(ui, tracks_rect, &tip, any, |p, o, c, _| {
            icons::tracks(p, o, s, c)
        });
        egui::Popup::from_toggle_button_response(&button)
            .id(super::tracks::popup_id())
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align(egui::RectAlign::TOP_END)
            .gap(6.0)
            .show(|ui| out.extend(super::tracks::panel(ui, &state.tracks)));
    }
    let panel_rect = tracks_rect.translate(vec2(-(SMALL + GAP), 0.0));
    if panel_rect.left() > group.right() + 4.0 {
        let panel = flat(
            ui,
            panel_rect,
            &with_key(
                tr!("Video, sound and subtitles", "Видео, звук и субтитры"),
                Cmd::ToggleVideoPanel,
            ),
            true,
            |p, o, c, _| icons::sliders(p, o, s, c),
        );
        if bar.video_panel_open {
            underline(ui, panel.rect);
        }
        if panel.clicked() {
            out.push(Cmd::ToggleVideoPanel);
        }
    }
    out
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

/// The round Play/Pause button in the middle.
fn play_button(ui: &mut Ui, rect: Rect, state: &State) -> Response {
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
        icons::pause(painter, rect.center(), 9.0, style::TEXT_BRIGHT);
    } else {
        icons::play(painter, rect.center(), 9.0, style::TEXT_BRIGHT);
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
