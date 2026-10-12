//! The control bar under the picture: the seek bar across the whole
//! width, below it the time on the left, the buttons
//! in the middle with Play in their centre, the volume on the right.

use egui::{Align2, Color32, FontId, Rect, Response, Sense, Ui, pos2, vec2};

use super::{icons, style};
use crate::input::Cmd;
use crate::player::State;

/// What the bar shows besides the player's state.
pub struct Bar<'a> {
    pub state: &'a State,
    pub playlist_visible: bool,
    pub fullscreen: bool,
    pub has_prev: bool,
    pub has_next: bool,
    /// The seek bar is being dragged: the position under the pointer.
    pub drag: &'a mut Option<f64>,
}

/// Draw the bar in `ui` (its full width); the commands of its buttons.
pub fn bar(ui: &mut Ui, bar: Bar<'_>) -> Vec<Cmd> {
    let mut out = Vec::new();
    let width = ui.available_width();
    let (seek_rect, _) = ui.allocate_exact_size(vec2(width, style::SEEK_HEIGHT), Sense::hover());
    if let Some(cmd) = seek_bar(ui, seek_rect.shrink2(vec2(10.0, 0.0)), bar.state, bar.drag) {
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
        tr!("Open a file (Ctrl+O)", "Открыть файл (Ctrl+O)"),
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
        tr!("Stop (S)", "Стоп (S)"),
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
        tr!("Previous file (PgUp)", "Предыдущий файл (PgUp)"),
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
        tr!("Next file (PgDn)", "Следующий файл (PgDn)"),
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
        tr!("Settings (F5)", "Настройки (F5)"),
        true,
        |p, o, c, fill| icons::gear(p, o, s, c, fill),
    )
    .clicked()
    {
        out.push(Cmd::Settings);
    }
    let list_tip = tr!("Playlist (L)", "Список воспроизведения (L)");
    let list = flat(ui, slot(&mut x, SMALL), list_tip, true, |p, o, c, _| {
        icons::list(p, o, s, c)
    });
    if bar.playlist_visible {
        let r = list.rect;
        ui.painter().line_segment(
            [
                pos2(r.center().x - 6.0, r.bottom() - 3.0),
                pos2(r.center().x + 6.0, r.bottom() - 3.0),
            ],
            egui::Stroke::new(2.0, style::ACCENT),
        );
    }
    if list.clicked() {
        out.push(Cmd::TogglePlaylist);
    }

    // The time, left of the buttons (left out when there is no room).
    let time_rect = Rect::from_min_max(
        pos2(row.left() + 12.0, row.top()),
        pos2(group.left() - 8.0, row.bottom()),
    );
    if time_rect.width() > 60.0 {
        let text = crate::format::position(state.time, state.duration);
        ui.painter_at(time_rect).text(
            time_rect.left_center(),
            Align2::LEFT_CENTER,
            text,
            FontId::monospace(12.0),
            style::TEXT_WEAK,
        );
    }

    // The volume and fullscreen, right of the buttons.
    let right = row.right() - 10.0;
    let fs = Rect::from_center_size(
        pos2(right - SMALL / 2.0, row.center().y),
        vec2(SMALL, SMALL),
    );
    let fs_tip = if bar.fullscreen {
        tr!(
            "Leave fullscreen (Enter, Esc)",
            "Выйти из полноэкранного режима (Enter, Esc)"
        )
    } else {
        tr!("Fullscreen (Enter)", "Полный экран (Enter)")
    };
    if flat(ui, fs, fs_tip, true, |p, o, c, _| {
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
            state.volume / 100.0
        };
        let tip = if state.mute {
            tr!("Sound on (M)", "Включить звук (M)")
        } else {
            tr!("Mute (M)", "Без звука (M)")
        };
        if flat(ui, speaker, tip, true, |p, o, c, _| {
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
    out
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
        style::ACCENT
    } else if response.hovered() {
        style::ACCENT_HOVER
    } else {
        style::ACCENT
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
        tr!("Pause (Space)", "Пауза (Пробел)")
    } else {
        tr!("Play (Space)", "Воспроизвести (Пробел)")
    };
    response.on_hover_text(tip)
}

/// The seek bar: a click or a drag moves there (on a keyframe; the exact
/// frame when the button is let go). The position under the pointer is
/// shown while hovering.
fn seek_bar(ui: &mut Ui, rect: Rect, state: &State, drag: &mut Option<f64>) -> Option<Cmd> {
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
        painter.rect_filled(done, 3.0, style::ACCENT);
    }
    if duration.is_some() && (response.hovered() || drag.is_some()) {
        let x = track.left() + track.width() * fraction;
        painter.circle_filled(pos2(x, track.center().y), 6.0, style::TEXT_BRIGHT);
        if let (Some(d), Some(f)) = (duration, pointer_fraction) {
            response.on_hover_text_at_pointer(crate::format::time(Some(f * d), d >= 3600.0));
        }
    }
    cmd
}

/// The volume: a short bar set by a click or a drag, or by the wheel.
fn volume_slider(ui: &mut Ui, rect: Rect, state: &State) -> Option<Cmd> {
    let response = ui.interact(rect, ui.id().with("volume"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    let painter = ui.painter();
    painter.rect_filled(track, 2.0, style::TRACK);
    let level = if state.mute {
        0.0
    } else {
        (state.volume / 100.0) as f32
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
            style::ACCENT
        },
    );
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
        let v = (((p.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0).round() as f64;
        cmd = Some(Cmd::SetVolume(v));
    }
    if response.hovered() {
        let notches = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::MouseWheel { delta, .. } => Some(delta.y.signum()),
                    _ => None,
                })
                .sum::<f32>()
        });
        if notches != 0.0 {
            cmd = Some(Cmd::SetVolume(
                (state.volume + notches as f64 * crate::input::VOLUME_STEP).clamp(0.0, 100.0),
            ));
        }
    }
    response.on_hover_text(tr!(
        format!("Volume {:.0}%", state.volume),
        format!("Громкость {:.0}%", state.volume)
    ));
    cmd
}
