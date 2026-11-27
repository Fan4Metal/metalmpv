//! Two videos compared on the same frame (`compare.rs`): B's core and
//! picture, made once the comparison is asked for, B kept on A's frame
//! (`follow_compare`, in `logic`), the two pictures in the video area
//! (`compare_picture`) and the wheel zooming at the pointer there.

use super::*;
use crate::compare::{self, Mode, Request, Side, Step};

/// B: its core and picture, and how it is shown beside A.
pub(super) struct Comparison {
    pub(super) player: Player,
    pub(super) video: Video,
    pub(super) path: PathBuf,
    /// B's time is A's plus this, in seconds.
    pub(super) offset: f64,
    /// Where the split line is, a share of the picture's width.
    pub(super) split: f32,
    /// B on the left (or on top), A on the right.
    pub(super) swapped: bool,
    /// B is the one shown in the flip (`Mode::Flip`).
    pub(super) flipped: bool,
    /// The name in the bottom right corner as last drawn: the navigator
    /// goes over it.
    pub(super) right_label: Option<Rect>,
    /// B's file is loaded.
    pub(super) loaded: bool,
    /// What B's container says, read while the information window is
    /// open.
    pub(super) facts: Option<FactsSlot>,
    /// Where the bar over the picture was last drawn.
    bar: Option<Rect>,
    sync: compare::Sync,
    /// What B was given: the picture's adjustments (A's but its shape),
    /// A's shape, the quality, the fill, the hardware decoding.
    adjust: Adjust,
    aspect: Option<f64>,
    quality: Option<crate::quality::Quality>,
    fill: bool,
    hwdec: bool,
}

impl Comparison {
    fn fps(&self) -> f64 {
        compare::frame_rate(&self.player.state.tracks)
    }
}

impl App {
    /// Compare `request.b` with `request.a` (played first if it is not
    /// the file playing), or with the file playing.
    pub(super) fn request_compare(&mut self, request: Request) {
        self.replace_compare();
        if let Some(a) = &request.a {
            let playing = !self.state_idle()
                && self
                    .playlist
                    .current_path()
                    .is_some_and(|p| crate::bookmarks::same_path(p, a));
            if !playing {
                match self.playlist.items().iter().position(|p| p == a) {
                    Some(i) => self.play(i),
                    None => self.open(vec![a.clone()]),
                }
            }
        }
        self.compare_wanted = Some(request);
    }

    /// Compare playlist entry `a` (played if it is not) with entry `b`.
    pub(super) fn compare_entries(&mut self, a: usize, b: usize) {
        let items = self.playlist.items();
        let (Some(pa), Some(pb)) = (items.get(a).cloned(), items.get(b).cloned()) else {
            return;
        };
        if a == b {
            return;
        }
        let playing = !self.state_idle()
            && self.playlist.current() == Some(a)
            && self.playlist.current_path() == Some(pa.as_path());
        self.replace_compare();
        if !playing {
            self.play(a);
        }
        self.compare_wanted = Some(Request {
            a: None,
            b: pb,
            offset: 0.0,
        });
    }

    /// Ask for the file to compare the one playing with.
    pub(super) fn pick_compare(&mut self, ctx: &egui::Context) {
        let Some(a) = self
            .playlist
            .current_path()
            .filter(|_| !self.state_idle())
            .map(Path::to_path_buf)
        else {
            self.osd.show_always(Osd::Text(
                tr!(
                    "[ Nothing plays to compare with ]",
                    "[ Сравнивать не с чем: ничего не воспроизводится ]"
                )
                .into(),
            ));
            return;
        };
        let dir = self.current_folder();
        let make = move || {
            let dialog = rfd::FileDialog::new()
                .set_title(tr!("Compare with", "Сравнить с"))
                .add_filter(tr!("Media files", "Медиафайлы"), playlist::MEDIA_EXTENSIONS)
                .add_filter(tr!("All files", "Все файлы"), &["*"]);
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::Compare(a), make, dialog::pick_file);
    }

    /// The file chosen to compare `a` with, if `a` still plays.
    pub(super) fn compare_chosen(&mut self, a: &Path, b: PathBuf) {
        let playing = !self.state_idle() && self.playlist.current_path() == Some(a);
        if playing {
            self.request_compare(Request {
                a: None,
                b,
                offset: 0.0,
            });
        } else {
            log::info!("comparison with {} dropped: it plays no more", a.display());
        }
    }

    /// The comparison ends: B's picture is freed in `logic` and its core
    /// destroyed on a thread of its own (that waits for mpv's threads); the
    /// zoom and the pan are put back as they were before it.
    pub(super) fn end_compare(&mut self) {
        self.replace_compare();
        if let Some((zoom, pan_x, pan_y)) = self.zoom_before_compare.take() {
            self.change_picture(|a| (a.zoom, a.pan_x, a.pan_y) = (zoom, pan_x, pan_y));
        }
    }

    /// The comparison ends for another to come: the zoom stays.
    fn replace_compare(&mut self) {
        self.compare_wanted = None;
        if let Some(mut c) = self.compare.take() {
            c.player.set_pause(true);
            log::debug!("comparison with {} ended", c.path.display());
            self.compare_closing.push(c);
        }
    }

    /// Free the pictures of comparisons that ended (in `logic`: eframe's
    /// OpenGL context is current) and let their cores go.
    pub(super) fn close_compare(&mut self, frame: &eframe::Frame) {
        for mut c in self.compare_closing.drain(..) {
            c.video.free(frame.gl().map(|gl| &**gl));
            let player = c.player;
            let spawned = crate::worker::spawn("compare end", move || drop(player));
            if let Err(e) = spawned {
                log::warn!("cannot end B's core on a thread: {e}");
            }
        }
    }

    /// Make B once asked for and A has a picture context: its core, its
    /// render context (eframe's OpenGL context is current in `ui`), then its
    /// file.
    pub(super) fn start_compare(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.compare_wanted.is_none() || self.player.is_none() || self.video.is_none() {
            return;
        }
        let Some(gl) = frame.gl().cloned() else {
            return;
        };
        let Some(request) = self.compare_wanted.take() else {
            return;
        };
        if self.mini.is_some() {
            self.leave_mini(ctx);
        }
        let started = Instant::now();
        let mpv = match crate::player::boot(self.settings.hwdec) {
            Ok(mpv) => mpv,
            Err(e) => {
                log::warn!("B's core: {e}");
                self.osd.show_always(Osd::Text(e.message()));
                return;
            }
        };
        let mut player = Player::new(mpv, ctx, &self.settings.audio_setup());
        player.set_hwdec(self.settings.hwdec);
        player.set_mute(true);
        let video = match Video::new(&gl, player.mpv(), ctx) {
            Ok(video) => video,
            Err(e) => {
                log::warn!("B's video output: {e}");
                self.osd.show_always(Osd::Text(tr!(
                    format!("Cannot compare: {e}"),
                    format!("Не удалось сравнить: {e}")
                )));
                let _ = crate::worker::spawn("compare end", move || drop(player));
                return;
            }
        };
        // Near A's place already; the sync puts it on A's frame.
        let start = self
            .player
            .as_ref()
            .and_then(|a| a.state.time)
            .map(|t| t + request.offset)
            .filter(|t| *t > 0.0);
        // No sound, no subtitles: B is looked at.
        let tracks = Tracks {
            audio: None,
            sub: None,
            sub_file: None,
        };
        player.load(&request.b, start, Some(tracks));
        log::debug!(
            "comparing with {} (core and picture in {:.0} ms)",
            request.b.display(),
            started.elapsed().as_secs_f64() * 1e3
        );
        // Kept through a B replaced by another.
        let a = &self.adjust;
        self.zoom_before_compare
            .get_or_insert((a.zoom, a.pan_x, a.pan_y));
        let name = file_name(&request.b);
        self.osd.show(Osd::Text(tr!(
            format!("[ Compared with {name} ]"),
            format!("[ Сравнение с {name} ]")
        )));
        self.compare = Some(Comparison {
            player,
            video,
            path: request.b,
            offset: request.offset,
            split: 0.5,
            swapped: false,
            flipped: false,
            right_label: None,
            loaded: false,
            facts: None,
            bar: None,
            sync: compare::Sync::default(),
            adjust: Adjust::default(),
            aspect: None,
            quality: None,
            fill: false,
            hwdec: self.settings.hwdec,
        });
    }

    /// B's events, what it is given of A's picture, and B kept on A's
    /// frame; every frame, also while minimised.
    pub(super) fn follow_compare(&mut self, minimized: bool) {
        let Some(c) = &mut self.compare else {
            return;
        };
        let now = Instant::now();
        let mut failed = None;
        for event in c.player.poll() {
            match event {
                Happened::FileLoaded => {
                    c.loaded = true;
                    c.sync.reset();
                }
                Happened::Restarted => c.sync.restarted(now),
                Happened::Failed(e) => failed = Some(e),
                Happened::Shutdown => failed = Some("mpv has shut down".into()),
                _ => {}
            }
        }
        if let Some(e) = failed {
            let name = file_name(&c.path);
            log::warn!("cannot compare with {name}: {e}");
            self.osd.show_always(Osd::Text(tr!(
                format!("Cannot open {name} to compare: {e}"),
                format!("Не удалось открыть {name} для сравнения: {e}")
            )));
            self.end_compare();
            return;
        }
        let Some(a) = &self.player else {
            return;
        };
        // A's picture as it is shown, but A's shape for B's own.
        let adjust = Adjust {
            aspect: crate::adjust::Aspect::Auto,
            ..self.adjust.clone()
        };
        if adjust != c.adjust {
            c.player.adjust(&c.adjust, &adjust);
            c.adjust = adjust;
        }
        let aspect = match (a.state.width, a.state.height) {
            (Some(w), Some(h)) if h > 0 => Some(w as f64 / h as f64),
            _ => None,
        };
        if aspect.is_some() && aspect != c.aspect {
            c.player.set_aspect_ratio(aspect);
            c.aspect = aspect;
        }
        let s = &self.settings;
        // No smooth motion: it sets B's speed itself.
        let quality = crate::quality::Quality {
            scaling: s.scaling,
            shaders: if s.shaders_on {
                s.shaders.clone()
            } else {
                Vec::new()
            },
            smooth: false,
            refresh: None,
        };
        if c.quality.as_ref() != Some(&quality) {
            for (name, value) in quality.changes(c.quality.as_ref()) {
                c.player.mpv().set(name, &crate::mpv::Value::Str(value));
            }
            c.quality = Some(quality);
        }
        if c.fill != self.fill {
            c.player.set_fill(self.fill);
            c.fill = self.fill;
        }
        if c.hwdec != s.hwdec {
            c.player.set_hwdec(s.hwdec);
            c.hwdec = s.hwdec;
        }
        let a_side = Side::of(&a.state, a.loading());
        let b_side = Side::of(&c.player.state, c.player.loading() || !c.loaded);
        for step in c.sync.follow(&a_side, &b_side, c.offset, now) {
            match step {
                Step::Seek(t) => c.player.seek_to(t, Seek::Exact),
                Step::Pause(p) => c.player.set_pause(p),
                Step::Speed(speed) => c.player.set_speed(speed),
            }
        }
        // No picture is painted: B's frames are taken unseen too.
        if minimized || a.state.idle {
            c.video.skip_unseen();
        }
    }

    /// A jumped (a seek, a loop): B follows.
    pub(super) fn compare_jumped(&mut self) {
        if let Some(c) = &mut self.compare {
            c.sync.jumped();
        }
    }

    /// The comparison's commands.
    pub(super) fn run_compare(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::CompareWith => self.pick_compare(ctx),
            Cmd::CompareEntries(a, b) => self.compare_entries(a, b),
            Cmd::CycleCompare => {
                let next = self.settings.compare_mode.next();
                self.set_compare_mode(next);
            }
            Cmd::SetCompare(mode) => self.set_compare_mode(mode),
            // The flip, also by the swap's key there (its sides are one).
            Cmd::CompareFlip | Cmd::SwapCompare if self.settings.compare_mode == Mode::Flip => {
                if let Some(c) = &mut self.compare {
                    c.flipped = !c.flipped;
                }
            }
            Cmd::CompareFlip => {
                // Into the flip with B: the press shows a change.
                if let Some(c) = &mut self.compare {
                    c.flipped = true;
                }
                self.set_compare_mode(Mode::Flip);
            }
            Cmd::SwapCompare => {
                if let Some(c) = &mut self.compare {
                    c.swapped = !c.swapped;
                    let text = if c.swapped {
                        tr!("[ B | A ]", "[ B | A ]")
                    } else {
                        tr!("[ A | B ]", "[ A | B ]")
                    };
                    self.osd.show(Osd::Text(text.into()));
                }
            }
            Cmd::CompareShift(frames) => {
                if let Some(c) = &mut self.compare {
                    let fps = c.fps();
                    c.offset = if frames == 0 {
                        0.0
                    } else {
                        // Whole frames: keys pressed over and over add up
                        // without a rounding drift.
                        ((c.offset * fps).round() + f64::from(frames)) / fps
                    };
                    c.sync.jumped();
                    let text = compare::shift_in(crate::i18n::lang(), c.offset, fps)
                        .unwrap_or_else(|| tr!("B without a shift", "B без сдвига").to_owned());
                    self.osd.show(Osd::Text(format!("[ {text} ]")));
                }
            }
            Cmd::EndCompare if self.compare.is_some() || self.compare_wanted.is_some() => {
                self.end_compare();
                self.osd.show(Osd::Text(
                    tr!("[ Comparison ended ]", "[ Сравнение закончено ]").into(),
                ));
            }
            _ => {}
        }
    }

    fn set_compare_mode(&mut self, mode: Mode) {
        self.settings.compare_mode = mode;
        if self.compare.is_some() {
            self.osd.show(Osd::Text(format!("[ {} ]", mode.label())));
        }
    }

    /// Where A and B are shown in `rect` (the video area): the same place
    /// for the split line, else the halves.
    fn compare_places(&self, rect: Rect, state: &State) -> (Rect, Rect) {
        let swapped = self.compare.as_ref().is_some_and(|c| c.swapped);
        match self.settings.compare_mode {
            Mode::Split | Mode::Flip => (rect, rect),
            Mode::Side => {
                let aspect = self
                    .picture_size(state)
                    .filter(|[w, h]| *w > 0.0 && *h > 0.0)
                    .map_or(16.0 / 9.0, |[w, h]| (w / h) as f32);
                let [first, second] = compare::halves(rect, aspect);
                if swapped {
                    (second, first)
                } else {
                    (first, second)
                }
            }
        }
    }

    /// Where A's and B's chips go over the picture (`Ctrl+I`) in `area`
    /// (the OSD's, off the fullscreen playlist), `rect` being the video
    /// area: each file's in the top corner of its own side, outward (the
    /// left one's at its left); one over the other, both at the right.
    pub(super) fn compare_chip_places(
        &self,
        rect: Rect,
        area: Rect,
        state: &State,
    ) -> [(Rect, egui::Align); 2] {
        let swapped = self.compare.as_ref().is_some_and(|c| c.swapped);
        let (a, b) = self.compare_places(rect, state);
        let (a, b) = if a == b {
            // The split line: A on the left of it unless swapped.
            let mid = area.center().x;
            let left = Rect::from_min_max(area.min, egui::pos2(mid, area.max.y));
            let right = Rect::from_min_max(egui::pos2(mid, area.min.y), area.max);
            if swapped {
                (right, left)
            } else {
                (left, right)
            }
        } else {
            (a.intersect(area), b.intersect(area))
        };
        let beside = (a.center().y - b.center().y).abs() < 1.0;
        let side = |r: Rect| {
            if beside && r.center().x < area.center().x {
                egui::Align::Min
            } else {
                egui::Align::Max
            }
        };
        [(a, side(a)), (b, side(b))]
    }

    /// The area the picture under `pos` is shown in (for the wheel's zoom
    /// and the navigator), A's while there is no comparison.
    pub(super) fn picture_place(&self, rect: Rect, pos: Option<egui::Pos2>, state: &State) -> Rect {
        if self.compare.is_none() {
            return rect;
        }
        let (a, b) = self.compare_places(rect, state);
        match pos {
            Some(p) if b.contains(p) && !a.contains(p) => b,
            _ => a,
        }
    }

    /// The two pictures in the video area `rect`, the split line following
    /// the pointer over it, and their names. B is rendered every frame (mpv
    /// waits for it otherwise), shown once it has a picture.
    pub(super) fn compare_picture(
        &mut self,
        ui: &egui::Ui,
        frame: &mut eframe::Frame,
        rect: Rect,
        response: &egui::Response,
        state: &State,
    ) {
        let (a_rect, b_rect) = self.compare_places(rect, state);
        let mode = self.settings.compare_mode;
        let bottom = self.compare_label_bottom(rect);
        let a_name = self
            .playlist
            .current_path()
            .map(|p| self.playlist.title(p))
            .unwrap_or_default();
        let (Some(video), Some(c)) = (&mut self.video, &mut self.compare) else {
            return;
        };
        let a_id = video.render(ui, frame, a_rect);
        let b_id = c.video.render(ui, frame, b_rect);
        let b_shown = c.loaded && c.player.state.video().is_some();
        let painter = ui.painter_at(rect);
        let whole = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let white = Color32::WHITE;
        // The split line follows the pointer over the picture.
        if mode == Mode::Split
            && response.hovered()
            && let Some(p) = ui.input(|i| i.pointer.hover_pos())
        {
            c.split = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        }
        let (left_label, right_label);
        match (mode, b_id.filter(|_| b_shown)) {
            (Mode::Split, Some(b_id)) => {
                let s = c.split;
                let x = rect.left() + rect.width() * s;
                let (left, right) = match (c.swapped, a_id) {
                    (false, Some(a_id)) => (a_id, b_id),
                    (true, Some(a_id)) => (b_id, a_id),
                    _ => return,
                };
                let uv_left = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(s, 1.0));
                let uv_right = Rect::from_min_max(egui::pos2(s, 0.0), egui::pos2(1.0, 1.0));
                painter.image(left, rect.with_max_x(x), uv_left, white);
                painter.image(right, rect.with_min_x(x), uv_right, white);
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(1.0, Color32::from_white_alpha(170)),
                );
                // In the corners of the whole picture, wherever the line is.
                (left_label, right_label) = (rect, rect);
            }
            (Mode::Flip, Some(b_id)) => {
                let shown = if c.flipped { Some(b_id) } else { a_id };
                if let Some(id) = shown {
                    painter.image(id, rect, whole, white);
                }
                (left_label, right_label) = (rect, rect);
            }
            (_, b_id) => {
                if let Some(a_id) = a_id {
                    painter.image(a_id, a_rect, whole, white);
                }
                if let Some(b_id) = b_id {
                    painter.image(b_id, b_rect, whole, white);
                }
                if mode == Mode::Side {
                    (left_label, right_label) = if c.swapped {
                        (b_rect, a_rect)
                    } else {
                        (a_rect, b_rect)
                    };
                } else {
                    // B not shown yet: A whole, B's line on its right.
                    (left_label, right_label) = (rect, rect);
                }
            }
        }
        // The names in the bottom corners, each side's own.
        let lang = crate::i18n::lang();
        let mut b_lines = vec![format!("B · {}", file_name(&c.path))];
        if !c.loaded {
            b_lines.push(tr!("loading…", "загрузка…").into());
        } else if !b_shown {
            b_lines.push(tr!("no picture", "нет картинки").into());
        } else if c.player.state.eof && !state.eof {
            b_lines.push(tr!("ended", "закончился").into());
        }
        if let Some(shift) = compare::shift_in(lang, c.offset, c.fps()) {
            b_lines.push(shift);
        }
        if let Some(drift) = c
            .sync
            .drift
            .filter(|d| !state.paused && d.abs() * c.fps() > 1.0)
        {
            let ms = (drift * 1000.0).round() as i64;
            b_lines.push(tr!(
                format!("off by {ms:+} ms"),
                format!("расхождение {ms:+} мс")
            ));
        }
        let a_lines = vec![format!("A · {a_name}")];
        // The flip: the name of the one shown bright, the other's dimmed.
        let flip = mode == Mode::Flip && b_shown;
        let a_lit = !(flip && c.flipped);
        let b_lit = !(flip && !c.flipped);
        let ((left_lines, left_lit), (right_lines, right_lit)) = if c.swapped {
            ((b_lines, b_lit), (a_lines, a_lit))
        } else {
            ((a_lines, a_lit), (b_lines, b_lit))
        };
        // Each at most half the picture's width (its own half side by side).
        let width = if left_label == right_label {
            rect.width() / 2.0
        } else {
            left_label.width()
        };
        let _ = label(
            &painter,
            left_label,
            bottom,
            &left_lines,
            Align2::LEFT_BOTTOM,
            width,
            left_lit,
        );
        c.right_label = Some(label(
            &painter,
            right_label,
            bottom,
            &right_lines,
            Align2::RIGHT_BOTTOM,
            width,
            right_lit,
        ));
    }

    /// How far above the video area's bottom the names go: over the
    /// control bar when it comes over the picture.
    fn compare_label_bottom(&self, rect: Rect) -> f32 {
        let over = if self.fullscreen {
            FULLSCREEN_BAR_ZONE
        } else if self.interface() == Interface::Bare {
            ui::controls::height(self.compact_bar())
        } else {
            0.0
        };
        rect.bottom() - over
    }

    /// The wheel over the picture while comparing: the zoom, about the
    /// point under the pointer.
    pub(super) fn zoom_at_pointer(
        &mut self,
        rect: Rect,
        pos: egui::Pos2,
        notches: i32,
        state: &State,
    ) {
        let area = self.picture_place(rect, Some(pos), state);
        let geometry = ui::navigator::Geometry::new(self.picture_size(state), area.size());
        let zoom = compare::wheel_zoom(self.adjust.zoom, notches);
        if zoom == self.adjust.zoom {
            return;
        }
        let pan = compare::pan_for_zoom(
            geometry,
            [self.adjust.pan_x, self.adjust.pan_y],
            self.adjust.zoom,
            zoom,
            pos - area.center(),
        );
        self.change_picture(|a| {
            a.zoom = zoom;
            // To a hundredth of a percent, as kept.
            a.pan_x = (pan[0] * 100.0).round() / 100.0;
            a.pan_y = (pan[1] * 100.0).round() / 100.0;
        });
        self.osd.show(Osd::Zoom(self.adjust.zoom));
    }

    /// The middle button dragging the picture while comparing: pressed at
    /// `from` with the pan `start`, the pointer now at `to`.
    pub(super) fn pan_at_pointer(
        &mut self,
        rect: Rect,
        from: egui::Pos2,
        start: [f64; 2],
        to: egui::Pos2,
        state: &State,
    ) {
        let area = self.picture_place(rect, Some(from), state);
        let geometry = ui::navigator::Geometry::new(self.picture_size(state), area.size());
        let pan = compare::pan_by(geometry, start, self.adjust.zoom, to - from)
            // To a hundredth of a percent, as kept.
            .map(|p| (p * 100.0).round() / 100.0);
        self.change_picture(|a| [a.pan_x, a.pan_y] = pan);
    }

    /// The comparison's bar over the picture's top (`top` under what comes
    /// over it there) while the pointer moves over the picture or rests on
    /// the bar, a button is down or B is loading.
    pub(super) fn compare_bar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        top: f32,
        cmds: &mut Vec<Cmd>,
    ) {
        const MARGIN: f32 = 8.0;
        let Some(c) = &self.compare else {
            return;
        };
        let ctx = ui.ctx().clone();
        let (hover, still, down) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.time_since_last_movement(),
                i.pointer.any_down(),
            )
        });
        let on_bar = hover.is_some_and(|p| c.bar.is_some_and(|b| b.expand(MARGIN).contains(p)));
        let over = hover.is_some_and(|p| rect.contains(p));
        let shown = over && (on_bar || down || still < BAR_FOR || !c.loaded);
        if !shown {
            if let Some(c) = &mut self.compare {
                c.bar = None;
            }
            return;
        }
        if !on_bar && !down {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(
                (BAR_FOR - still).max(0.05),
            ));
        }
        let fps = c.fps();
        let default = Adjust::default();
        let shown = ui::compare_bar::Shown {
            mode: self.settings.compare_mode,
            swapped: c.swapped,
            flipped: c.flipped,
            shift: (c.offset * fps).round() as i64,
            shift_text: compare::shift_in(crate::i18n::lang(), c.offset, fps)
                .unwrap_or_else(|| tr!("B without a shift", "B без сдвига").to_owned()),
            zoom: self.adjust.zoom,
            moved: self.adjust.zoom != default.zoom
                || self.adjust.pan_x != default.pan_x
                || self.adjust.pan_y != default.pan_y,
        };
        let at = egui::pos2(rect.center().x, top + MARGIN);
        let (bar, out) = ui::compare_bar::show(ui, rect.shrink(MARGIN), at, &shown);
        if let Some(c) = &mut self.compare {
            c.bar = Some(bar);
        }
        cmds.extend(out);
    }

    /// While files are dragged over the window: whether the one file
    /// dragged could be compared with the file playing.
    pub(super) fn can_compare_drop(&self, files: &[egui::HoveredFile], state: &State) -> bool {
        let [file] = files else {
            return false;
        };
        !state.idle
            && state.video().is_some()
            && self.player.is_some()
            && file
                .path
                .as_deref()
                .is_some_and(|p| playlist::is_media(p) && !tracks::is_subtitles(p))
    }

    /// The pointer in the window's client area in points, while a file is
    /// dragged over it (egui's pointer stands still then).
    pub(super) fn drop_pointer(&self, ctx: &egui::Context) -> Option<egui::Pos2> {
        let [x, y] = win::cursor_in(self.hwnd?)?;
        let ppp = ctx.pixels_per_point();
        Some(egui::pos2(x as f32 / ppp, y as f32 / ppp))
    }

    /// Files dropped where the comparison was offered: the one file
    /// compared with the file playing. Whether it was.
    pub(super) fn compare_dropped(&mut self, ctx: &egui::Context, dropped: &[PathBuf]) -> bool {
        let Some(zone) = self.compare_drop.take() else {
            return false;
        };
        let [file] = dropped else {
            return false;
        };
        if !self.drop_pointer(ctx).is_some_and(|p| zone.contains(p)) {
            return false;
        }
        self.request_compare(Request {
            a: None,
            b: file.clone(),
            offset: 0.0,
        });
        true
    }
}

/// How long the comparison's bar stays after the pointer stops, seconds.
const BAR_FOR: f32 = 2.0;

/// A file's name, or the address.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Lines of text on a dark ground in a bottom corner of `area`, no lower
/// than `bottom`, each on one line cut short with an ellipsis to fit
/// `width` (a long name was wrapped word by word into a narrow column).
fn label(
    painter: &egui::Painter,
    area: Rect,
    bottom: f32,
    lines: &[String],
    corner: Align2,
    width: f32,
    lit: bool,
) -> Rect {
    const MARGIN: f32 = 10.0;
    const PAD: egui::Vec2 = egui::vec2(7.0, 4.0);
    const GAP: f32 = 16.0;
    let room = (width - MARGIN - GAP / 2.0 - 2.0 * PAD.x).max(60.0);
    let galleys: Vec<_> = lines
        .iter()
        .map(|line| {
            let mut job = egui::text::LayoutJob::simple_singleline(
                line.clone(),
                FontId::proportional(13.0),
                Color32::PLACEHOLDER,
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(room);
            painter.layout_job(job)
        })
        .collect();
    let text = egui::vec2(
        galleys.iter().map(|g| g.size().x).fold(0.0, f32::max),
        galleys.iter().map(|g| g.size().y).sum(),
    );
    let size = text + 2.0 * PAD;
    let y = area.bottom().min(bottom) - MARGIN - size.y;
    let x = if corner == Align2::LEFT_BOTTOM {
        area.left() + MARGIN
    } else {
        area.right() - MARGIN - size.x
    };
    let ground = Rect::from_min_size(egui::pos2(x, y), size);
    let (ground_alpha, colour) = if lit {
        (150, style::TEXT_BRIGHT)
    } else {
        (90, style::text_weak())
    };
    painter.rect_filled(ground, 4.0, Color32::from_black_alpha(ground_alpha));
    let mut at = ground.min + PAD;
    for galley in galleys {
        let h = galley.size().y;
        // Right-aligned in the right corner.
        let dx = if corner == Align2::LEFT_BOTTOM {
            0.0
        } else {
            text.x - galley.size().x
        };
        painter.galley(at + egui::vec2(dx, 0.0), galley, colour);
        at.y += h;
    }
    ground
}
