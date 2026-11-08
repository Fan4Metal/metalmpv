//! The window's layout: the interface shown, its insets, the window sized
//! for the picture and for the interface, the control bar's view, the
//! seek bar's frames.

use super::*;

impl App {
    /// The interface shown.
    pub(super) fn interface(&self) -> Interface {
        self.settings.interface
    }

    /// The playlist is shown: over the picture in fullscreen, else docked
    /// or in its window (not in only the picture).
    pub(super) fn playlist_shown(&self, ctx: &egui::Context) -> bool {
        if frameless::is_fullscreen(ctx) {
            self.fullscreen_playlist
        } else {
            self.settings.playlist_visible
                && self.interface() != Interface::Bare
                && self.mini.is_none()
        }
    }

    /// What `interface` takes around the picture in a window, in points
    /// (left, top, right, bottom).
    pub(super) fn insets(&self, interface: Interface) -> [f32; 4] {
        let top = if interface.title_row() {
            style::TITLE_HEIGHT
        } else {
            0.0
        };
        let (right, bottom) = match interface {
            Interface::Bare => (0.0, 0.0),
            Interface::Compact => (self.playlist_inset, ui::controls::height(true)),
            Interface::Full | Interface::NoTitle => {
                (self.playlist_inset, ui::controls::height(false))
            }
        };
        [0.0, top, right, bottom]
    }

    /// The window was made for another interface than the one shown: it
    /// grows or shrinks by the difference, the picture keeping its place
    /// and size on the screen, and is kept on the work area. Not while
    /// maximized, in fullscreen or minimised: once it is a window again.
    pub(super) fn fit_interface(&mut self, ctx: &egui::Context) {
        let now = self.interface();
        if self.sized_for == now
            || self.mini.is_some()
            || frameless::is_fullscreen(ctx)
            || frameless::is_maximized(ctx)
            || ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            return;
        }
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let Some(window) = win::window_rect(hwnd) else {
            return;
        };
        let mut slack = match self.fit_slack {
            Some((left, slack)) if left == window => slack,
            _ => [0, 0],
        };
        let rect = self.refit(
            window,
            self.sized_for,
            now,
            ctx.pixels_per_point(),
            &mut slack,
        );
        self.sized_for = now;
        let rect = match win::work_area(hwnd) {
            Some(work) => sizing::placed(rect, [rect[2] - rect[0], rect[3] - rect[1]], work),
            None => rect,
        };
        self.fit_slack = (slack != [0, 0]).then_some((rect, slack));
        win::set_rect(hwnd, rect);
    }

    /// `window` (in pixels) made for interface `from`, grown or shrunk for
    /// `to` by the difference of their insets, the picture keeping its
    /// place. Not under the window's least size: what a shrink could not
    /// take off is added to `slack`, and a growth takes it off first.
    pub(super) fn refit(
        &self,
        window: [i32; 4],
        from: Interface,
        to: Interface,
        ppp: f32,
        slack: &mut [i32; 2],
    ) -> [i32; 4] {
        let least = crate::MIN_SIZE.map(|v| (v * ppp).round() as i32);
        sizing::refit(
            window,
            self.insets(from),
            self.insets(to),
            ppp,
            least,
            slack,
        )
    }

    /// The bar's layout over the picture or in fullscreen: compact in the
    /// compact interface, and with only the picture when that came from
    /// the compact one.
    pub(super) fn compact_bar(&self) -> bool {
        match self.interface() {
            Interface::Bare => self.before_bare == Interface::Compact,
            interface => interface == Interface::Compact,
        }
    }

    /// The title row comes over the picture's top (an interface without
    /// one of its own) while the pointer is there, over nothing else.
    pub(super) fn wants_title_strip(&self, ctx: &egui::Context) -> bool {
        let area = self.picture_area;
        let zone = Rect::from_min_max(
            area.min,
            egui::pos2(area.right(), area.top() + style::TITLE_HEIGHT + 8.0),
        );
        pointer_over(ctx, zone)
    }

    /// With only the picture, the control bar comes over its bottom while
    /// the pointer is there, a seek is dragged or a window of the bar is
    /// open.
    pub(super) fn wants_bar_over(&self, ctx: &egui::Context) -> bool {
        let area = self.picture_area;
        let height = ui::controls::height(self.compact_bar());
        let zone = Rect::from_min_max(
            egui::pos2(area.left(), area.bottom() - height - 16.0),
            area.max,
        );
        pointer_over(ctx, zone)
            || self.seek_drag.is_some()
            || self.video_panel.open
            || self.tracks_window.open
    }

    /// The control bar's view of the app.
    pub(super) fn controls_bar<'a>(
        &'a mut self,
        state: &'a State,
        fullscreen: bool,
        compact: bool,
    ) -> ui::controls::Bar<'a> {
        ui::controls::Bar {
            state,
            playlist_visible: if fullscreen {
                self.fullscreen_playlist
            } else {
                self.settings.playlist_visible
            },
            video_panel_open: self.video_panel.open,
            tracks_open: self.tracks_window.open,
            fullscreen,
            compact,
            has_prev: self.playlist.prev_in(self.settings.repeat).is_some(),
            has_next: self.playlist.next_in(self.settings.repeat).is_some(),
            chapter_marks: self.settings.chapter_marks,
            time_mode: self.settings.time_mode,
            drag: &mut self.seek_drag,
            panel_button: &mut self.panel_button,
            tracks_button: &mut self.tracks_button,
            thumbs: self.thumbs.as_mut(),
        }
    }

    pub(super) fn size_window(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        picture: [f64; 2],
    ) {
        if frameless::is_fullscreen(ctx) {
            self.pending_picture = Some(picture);
            self.set_fullscreen(ctx, frame, false);
            return;
        }
        let Some(hwnd) = self.hwnd else {
            return;
        };
        let (Some(work), Some((window, client))) =
            (win::work_area(hwnd), win::restored_rects(hwnd))
        else {
            return;
        };
        let ppp = f64::from(ctx.pixels_per_point());
        let chrome = [
            f64::from(self.chrome.x) * ppp,
            f64::from(self.chrome.y) * ppp,
        ];
        let min = crate::MIN_SIZE.map(|v| f64::from(v) * ppp);
        // A frame around the client area, if Windows draws one.
        let border = [
            window[2] - window[0] - client[0],
            window[3] - window[1] - client[1],
        ];
        let room = [
            f64::from(work[2] - work[0] - border[0]),
            f64::from(work[3] - work[1] - border[1]),
        ];
        let size = sizing::client_size(picture, chrome, room, min);
        let rect = sizing::placed(window, [size[0] + border[0], size[1] + border[1]], work);
        log::debug!("window for a picture of {picture:?} px: {rect:?}");
        win::set_rect(hwnd, rect);
        // Measured with the interface shown.
        self.sized_for = self.interface();
    }

    /// The window for a file just loaded, as `window_on_open` says, once
    /// its picture's size is known; not maximized or in fullscreen, nor for
    /// sound alone.
    pub(super) fn fit_on_open(
        &mut self,
        ctx: &egui::Context,
        frame: &eframe::Frame,
        state: &State,
    ) {
        if !self.fit_on_open {
            return;
        }
        // The mini player takes the picture's shape itself.
        if self.mini.is_some() {
            self.fit_on_open = false;
            return;
        }
        let no_picture = !state.tracks.is_empty()
            && !state
                .tracks
                .iter()
                .any(|t| t.kind == Kind::Video && !t.albumart);
        if state.idle || no_picture {
            self.fit_on_open = false;
            return;
        }
        let Some(video) = self.picture_size(state) else {
            return;
        };
        self.fit_on_open = false;
        if frameless::is_fullscreen(ctx) || frameless::is_maximized(ctx) {
            return;
        }
        let target = match self.settings.window_on_open {
            WindowOnOpen::Keep => return,
            WindowOnOpen::Video => video,
            WindowOnOpen::Fit => {
                let ppp = f64::from(ctx.pixels_per_point());
                let area = self.picture_area.size();
                sizing::same_width(video, [f64::from(area.x) * ppp, f64::from(area.y) * ppp])
            }
        };
        self.size_window(ctx, frame, target);
    }

    pub(super) fn picture_size(&self, state: &State) -> Option<[f64; 2]> {
        let [w, h] = video_size(state)?;
        Some(if self.adjust.rotate % 180 == 90 {
            [h, w]
        } else {
            [w, h]
        })
    }

    /// The picture's size as shown: the video's display size, turned by
    /// the Video panel's rotation (done when the picture is drawn, so not
    /// in mpv's `dwidth`/`dheight`).
    /// The picture's shape and its area, for the navigators.
    pub(super) fn navigator_geometry(&self, state: &State) -> ui::navigator::Geometry {
        ui::navigator::Geometry::new(self.picture_size(state), self.picture_area.size())
    }

    /// The frames over the seek bar follow the file playing and the
    /// picture as shown: for a file on the computer with a picture and a
    /// duration while the setting is on (the core starts only once the bar
    /// asks, `thumbs.rs`).
    pub(super) fn follow_thumbs(&mut self, state: &State) {
        let wanted = state
            .path
            .as_deref()
            .map(PathBuf::from)
            .filter(|p| !playlist::is_url(p))
            .zip(state.duration.filter(|d| d.is_finite() && *d > 1.0))
            .filter(|_| {
                self.settings.seek_preview
                    && !state.idle
                    && state.width.is_some()
                    && self.player.as_ref().is_some_and(|p| !p.loading())
            });
        let Some((input, duration)) = wanted else {
            self.thumbs = None;
            return;
        };
        let request = crate::thumbs::Request {
            input,
            picture: self.picture_options(),
            size: (crate::thumbs::SIZE * self.ppp).round() as u32,
        };
        if !self
            .thumbs
            .as_ref()
            .is_some_and(|t| t.is_for(&request, duration))
        {
            self.thumbs = Some(crate::thumbs::Thumbs::new(request, duration));
        }
    }

    /// mpv draws the picture as the settings say (`quality.rs`): only what
    /// changed is given, also the screen's refresh rate for the smooth
    /// motion when the window goes to another monitor or the rate changes.
    pub(super) fn follow_quality(&mut self) {
        let s = &self.settings;
        let wanted = crate::quality::Quality {
            scaling: s.scaling,
            shaders: if s.shaders_on {
                s.shaders.clone()
            } else {
                Vec::new()
            },
            smooth: s.smooth_motion,
            refresh: if s.smooth_motion {
                self.refresh_rate()
            } else {
                None
            },
        };
        if self.quality.as_ref() == Some(&wanted) {
            return;
        }
        let Some(player) = &self.player else {
            return;
        };
        for (name, value) in wanted.changes(self.quality.as_ref()) {
            log::debug!("quality: {name}={value}");
            player.mpv().set(name, &crate::mpv::Value::Str(value));
        }
        self.quality = Some(wanted);
    }

    /// The refresh rate of the window's screen, read again every few
    /// seconds (a mode changed) or when the window is on another monitor.
    fn refresh_rate(&mut self) -> Option<f64> {
        const AGAIN: Duration = Duration::from_secs(3);
        let hwnd = self.hwnd?;
        let monitor = win::monitor_of(hwnd);
        if let Some((m, at, rate)) = self.refresh
            && m == monitor
            && at.elapsed() < AGAIN
        {
            return rate;
        }
        let rate = win::refresh_rate(hwnd);
        if self
            .refresh
            .is_none_or(|(m, _, r)| m != monitor || r != rate)
        {
            log::debug!("screen refresh rate: {rate:?}");
        }
        self.refresh = Some((monitor, Instant::now(), rate));
        rate
    }

    /// mpv's options of the picture as shown, for the second cores (the
    /// video map, the seek bar's frames): the turn, the shape and a mirror
    /// (upside down is one turned half round); not the colour, the zoom or
    /// the other filters.
    pub(super) fn picture_options(&self) -> Vec<(&'static str, String)> {
        let a = &self.adjust;
        let mut picture = vec![
            ("video-rotate", a.rotation().to_string()),
            ("video-aspect-override", a.aspect.option().to_owned()),
        ];
        if a.mirror != a.flip {
            picture.push(("vf", "lavfi=[hflip]".to_owned()));
        }
        picture
    }
}

pub(super) fn video_size(state: &State) -> Option<[f64; 2]> {
    match (state.width, state.height) {
        (Some(w), Some(h)) => Some([w as f64, h as f64]),
        _ => None,
    }
}
