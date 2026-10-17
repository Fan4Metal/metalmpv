//! The mini player (`0`, `Cmd::ToggleMini`, the title row's button): the
//! player's own window made small and kept over the others, at first in the
//! bottom right corner of the work area, afterwards where it was left
//! (`settings.mini_rect`). Only the picture, its client area of the
//! picture's shape (a new file or a turn reshapes it, the corner it is
//! nearer staying put; `sizing::mini_fitted`), kept while the edges are
//! dragged (`win::Mini`), drawn to the work area's edges while it moves.
//! Over the picture while the pointer moves there or nothing plays:
//! Play, the speaker, the volume and the seek bar along its bottom, the
//! way back in its top right corner (`ui::controls::mini`). The window
//! before is put back as it was, a maximized one maximized; the way back
//! (`0`, the corner's button, a double click, the menu) also to fullscreen
//! when the mini player came from there, `Esc` to the window. Commands
//! that want the player's window (the playlist, fullscreen, the windows of
//! the bar) leave the mini player first (`leaves_mini`).

use super::*;

/// The mini player's width at first, in points.
const WIDTH: f32 = 384.0;
/// Its distance from the work area's edges at first, in points.
const MARGIN: f32 = 16.0;
/// The controls stay this long after the pointer last moved over the
/// picture, in seconds.
const CONTROLS_FOR: f32 = 2.0;

/// The player's window before the mini player, to come back to.
pub(super) struct Before {
    /// Its rectangle in pixels; the restored one of a maximized window.
    rect: [i32; 4],
    maximized: bool,
    /// The mini player came from fullscreen: the way back goes there.
    fullscreen: bool,
}

/// Commands that want the player's window: the mini player ends first.
pub(super) fn leaves_mini(cmd: &Cmd) -> bool {
    matches!(
        cmd,
        Cmd::ToggleFullscreen
            | Cmd::ToggleFill
            | Cmd::ToggleInterface
            | Cmd::CycleInterface
            | Cmd::SetInterface(_)
            | Cmd::TogglePlaylist
            | Cmd::TogglePlaylistWindow
            | Cmd::PlaylistSearch
            | Cmd::WindowScale(_)
            | Cmd::FitScreen
            | Cmd::FitWindow
            | Cmd::ToggleMaximize
            | Cmd::ToggleInfo
            | Cmd::ToggleVideoPanel
            | Cmd::ShowEqualizer
            | Cmd::ToggleTracks
            | Cmd::Settings
            | Cmd::About
            | Cmd::OpenUrl
            | Cmd::GoToTime
            | Cmd::SaveClip
    )
}

/// The picture's shape (width over height), none without a picture.
fn aspect(size: Option<[f64; 2]>) -> Option<f64> {
    size.filter(|[w, h]| *w > 0.0 && *h > 0.0)
        .map(|[w, h]| w / h)
}

impl App {
    pub(super) fn toggle_mini(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.mini.is_some() {
            // Back where it came from.
            if self.leave_mini(ctx) {
                self.set_fullscreen(ctx, frame, true);
            }
        } else if frameless::is_fullscreen(ctx) {
            // Fullscreen ends first, and the mini player comes in the
            // frame after (`follow_mini`).
            self.mini_pending = true;
            self.set_fullscreen(ctx, frame, false);
        } else {
            self.enter_mini(ctx, false);
        }
    }

    /// The mini player, from a window; `fullscreen`: one that was in
    /// fullscreen a frame ago.
    fn enter_mini(&mut self, ctx: &egui::Context, fullscreen: bool) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let maximized = frameless::is_maximized(ctx);
        let before = if maximized {
            win::normal_rect(hwnd)
        } else {
            win::window_rect(hwnd)
        };
        // Restores a maximized window: the frame around the client area.
        let (Some(before), Some((window, client))) = (before, win::restored_rects(hwnd)) else {
            return;
        };
        let border = [
            window[2] - window[0] - client[0],
            window[3] - window[1] - client[1],
        ];
        log::debug!(
            "mini player, the window was {before:?} (maximized: {maximized}, \
             fullscreen: {fullscreen})"
        );
        self.mini = Some(Before {
            rect: before,
            maximized,
            fullscreen,
        });
        self.mini_border = border;
        // Their places are the player's window's.
        self.tracks_window.open = false;
        self.video_panel.open = false;
        self.info.open = false;
        // Aero Snap and Win+Up would maximize it.
        ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
            close: true,
            minimized: true,
            maximize: false,
        });
        let ppp = ctx.pixels_per_point();
        let aspect = aspect(self.picture_size(&self.state()));
        let saved = self.settings.mini_rect.filter(|r| win::on_a_monitor(*r));
        let start = saved.unwrap_or_else(|| {
            let width = (WIDTH * ppp).round() as i32;
            let height = (f64::from(width) / aspect.unwrap_or(16.0 / 9.0)).round() as i32;
            let work = win::work_area(hwnd).unwrap_or([0, 0, width, height]);
            sizing::mini_corner(
                [width + border[0], height + border[1]],
                work,
                (MARGIN * ppp).round() as i32,
            )
        });
        self.fit_mini(ppp, aspect, Some(start));
    }

    /// The player's window again, as it was; whether it was in fullscreen
    /// before the mini player (the caller goes back there or not).
    pub(super) fn leave_mini(&mut self, ctx: &egui::Context) -> bool {
        self.mini_pending = false;
        let Some(before) = self.mini.take() else {
            return false;
        };
        win::set_mini(None);
        ctx.send_viewport_cmd(egui::ViewportCommand::EnableButtons {
            close: true,
            minimized: true,
            maximize: true,
        });
        let Some(hwnd) = self.hwnd else {
            return before.fullscreen;
        };
        if !frameless::is_maximized(ctx)
            && let Some(rect) = win::window_rect(hwnd)
        {
            self.settings.mini_rect = Some(rect);
        }
        log::debug!("the player's window again: {:?}", before.rect);
        if before.maximized {
            win::show_maximized(hwnd, Some(before.rect), crate::DEFAULT_SIZE);
        } else {
            win::set_rect(hwnd, before.rect);
        }
        before.fullscreen
    }

    /// The window at `from` (else where it is) made for the picture's
    /// `aspect` (none: sound alone, the size as it is), with its least size
    /// for it; on the work area of the monitor it is to be on.
    fn fit_mini(&mut self, ppp: f32, aspect: Option<f64>, from: Option<[i32; 4]>) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let border = self.mini_border;
        let least = sizing::mini_least(aspect);
        let least = [
            (least[0] * f64::from(ppp)).round() as i32 + border[0],
            (least[1] * f64::from(ppp)).round() as i32 + border[1],
        ];
        // Before the window is made smaller: winit's least size is the
        // player's.
        win::set_mini(Some(win::Mini { least, aspect }));
        self.mini_aspect = aspect;
        let Some(rect) = from.or_else(|| win::window_rect(hwnd)) else {
            return;
        };
        let fitted = match (aspect, win::work_area_at(rect)) {
            (Some(a), Some(work)) => sizing::mini_fitted(rect, a, border, least, work),
            _ => rect,
        };
        if from.is_some() || fitted != rect {
            log::debug!("mini player for a picture of {aspect:?}: {fitted:?}");
            win::set_rect(hwnd, fitted);
        }
    }

    /// Each frame: the mini player asked for in fullscreen comes once that
    /// has ended; the mini player follows the picture's shape. Maximized
    /// or minimised it waits.
    pub(super) fn follow_mini(&mut self, ctx: &egui::Context, state: &State) {
        if self.mini_pending && !frameless::is_fullscreen(ctx) {
            self.mini_pending = false;
            self.enter_mini(ctx, true);
            return;
        }
        if self.mini.is_none()
            || frameless::is_maximized(ctx)
            || ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            return;
        }
        // A turn changes it too; the same shape within half a percent.
        let aspect = aspect(self.picture_size(state));
        let same = match (aspect, self.mini_aspect) {
            (Some(a), Some(b)) => (a / b - 1.0).abs() < 0.005,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.fit_mini(ctx.pixels_per_point(), aspect, None);
        }
    }

    /// The mini player's controls over the picture in `rect`: while the
    /// pointer moves over it (or rests on them), a seek or the volume is
    /// dragged, a menu is open, or nothing plays; and the pointer is in the
    /// window.
    pub(super) fn mini_controls(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        state: &State,
        cmds: &mut Vec<Cmd>,
    ) {
        let ctx = ui.ctx().clone();
        let (hover, still, down) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.time_since_last_movement(),
                i.pointer.any_down(),
            )
        });
        let playing = !state.idle && !state.paused && !state.eof;
        let on_controls = hover.is_some_and(|p| {
            p.y >= rect.bottom() - ui::controls::MINI_ROW - 4.0
                || (p.y <= rect.top() + 40.0 && p.x >= rect.right() - 40.0)
        });
        let shown = self.seek_drag.is_some()
            || egui::Popup::is_any_open(&ctx)
            || (hover.is_some_and(|p| rect.contains(p))
                && (!playing || on_controls || down || still < CONTROLS_FOR));
        if !shown {
            return;
        }
        if hover.is_some() && playing && !on_controls && !down {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(
                (CONTROLS_FOR - still).max(0.05),
            ));
        }
        cmds.extend(ui::controls::mini(ui, rect, state, &mut self.seek_drag));
    }

    /// The player's window as eframe is to restore it at the next start (it
    /// saves the mini player's), in its `WindowSettings`' RON; and the
    /// restored rectangle the app keeps for a maximized one.
    pub(super) fn save_mini(&mut self, storage: &mut dyn eframe::Storage) {
        let Some(before) = &self.mini else {
            return;
        };
        if let Some(rect) = self.hwnd.and_then(win::window_rect) {
            self.settings.mini_rect = Some(rect);
        }
        self.settings.window_normal = Some(before.rect);
        storage.set_string(
            "window",
            window_settings(before.rect, self.mini_border, before.maximized, self.ppp),
        );
    }
}

/// eframe's `WindowSettings` (egui-winit's, saved under `window`) of a
/// window at `rect` in pixels with `border` pixels of frame, in points of
/// `ppp` pixels.
fn window_settings(rect: [i32; 4], border: [i32; 2], maximized: bool, ppp: f32) -> String {
    let width = (rect[2] - rect[0] - border[0]) as f32 / ppp;
    let height = (rect[3] - rect[1] - border[1]) as f32 / ppp;
    let (x, y) = (rect[0] as f32, rect[1] as f32);
    let (ix, iy) = (x + (border[0] / 2) as f32, y);
    format!(
        "(inner_position_pixels:Some((x:{ix:.1},y:{iy:.1})),\
         outer_position_pixels:Some((x:{x:.1},y:{y:.1})),\
         fullscreen:false,maximized:{maximized},\
         inner_size_points:Some((x:{width:.1},y:{height:.1})))"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_comes_back_as_before_the_mini_player() {
        // As eframe writes it (a settings file's).
        assert_eq!(
            window_settings([100, 50, 1380, 770], [0, 0], false, 1.25),
            "(inner_position_pixels:Some((x:100.0,y:50.0)),\
             outer_position_pixels:Some((x:100.0,y:50.0)),\
             fullscreen:false,maximized:false,\
             inner_size_points:Some((x:1024.0,y:576.0)))"
        );
        let text = window_settings([-1500, 20, -300, 700], [16, 8], true, 1.0);
        assert!(ron::from_str::<ron::Value>(&text).is_ok(), "{text}");
        assert!(text.contains("maximized:true"));
    }

    #[test]
    fn the_playlist_and_the_windows_leave_the_mini_player() {
        assert!(leaves_mini(&Cmd::TogglePlaylist));
        assert!(leaves_mini(&Cmd::ToggleFullscreen));
        assert!(!leaves_mini(&Cmd::TogglePause));
        assert!(!leaves_mini(&Cmd::Volume(5.0)));
        assert!(!leaves_mini(&Cmd::ToggleMini));
    }
}
