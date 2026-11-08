//! The commands of keys, buttons and menus (`Cmd`), run at the end of a frame.

use super::*;

impl App {
    /// Run a command of a key, a button or a menu.
    pub(super) fn run(&mut self, ctx: &egui::Context, frame: &eframe::Frame, cmd: Cmd) {
        if self.mini.is_some() && mini::leaves_mini(&cmd) {
            self.leave_mini(ctx);
            // The playlist asked for in the mini player: shown (from only
            // the picture, the interface comes back with it below).
            if cmd == Cmd::TogglePlaylist && self.interface() != Interface::Bare {
                self.settings.playlist_visible = true;
                return;
            }
        }
        let state = self.state();
        match cmd {
            Cmd::TogglePause => {
                if state.idle {
                    // After Stop or a failed file: play the current entry
                    // again, or the one after its place when it was removed.
                    if let Some(i) = self
                        .playlist
                        .current()
                        .or_else(|| self.playlist.next())
                        .or((!self.playlist.is_empty()).then_some(0))
                    {
                        self.play(i);
                    } else {
                        self.pick_files(ctx, false);
                    }
                } else if let Some(player) = &mut self.player {
                    player.toggle_pause();
                    self.osd.show(Osd::Pause(player.state.paused));
                }
            }
            Cmd::Stop => {
                self.remember_position();
                if let Some(player) = &mut self.player {
                    player.stop();
                }
            }
            Cmd::SeekShort(d) => self.seek_by(d * self.settings.seek_short, Seek::Keyframe),
            Cmd::SeekLong(d) => self.seek_by(d * self.settings.seek_long, Seek::Keyframe),
            Cmd::SeekExact(d) => self.seek_by(d, Seek::Exact),
            Cmd::SeekTo { seconds, exact } => {
                self.seek_to(seconds, if exact { Seek::Exact } else { Seek::Keyframe });
            }
            Cmd::Chapter(i) => {
                // Exact, as mpv's own chapter seeks: a keyframe seek would
                // land before the start, in the chapter before.
                if let Some(chapter) = state.chapters.get(i)
                    && let Some(player) = &mut self.player
                    && !player.state.idle
                {
                    player.seek_to(chapter.time, Seek::Exact);
                    self.osd.show(Osd::Chapter {
                        number: i + 1,
                        title: chapter.title.clone(),
                    });
                }
            }
            Cmd::ShowPosition => {
                let state = self.state();
                if let (false, Some(time)) = (state.idle, state.time) {
                    let chapter = crate::chapters::current(&state.chapters, time)
                        .map(|i| (i + 1, state.chapters[i].title.clone()));
                    self.osd.show(Osd::Position {
                        time,
                        duration: state.duration,
                        chapter,
                    });
                }
            }
            Cmd::ToggleChapterMarks => {
                self.settings.chapter_marks = !self.settings.chapter_marks;
            }
            Cmd::ToggleShaders => {
                let text = if self.settings.shaders.is_empty() {
                    tr!(
                        "[ No shaders: they are added in the settings, Video ]",
                        "[ Шейдеров нет: они добавляются в настройках, «Видео» ]"
                    )
                } else {
                    self.settings.shaders_on = !self.settings.shaders_on;
                    if self.settings.shaders_on {
                        tr!("[ Shaders: on ]", "[ Шейдеры: вкл. ]")
                    } else {
                        tr!("[ Shaders: off ]", "[ Шейдеры: выкл. ]")
                    }
                };
                self.osd.show(Osd::Text(text.into()));
            }
            Cmd::ToggleSmooth => {
                self.settings.smooth_motion = !self.settings.smooth_motion;
                self.osd.show(Osd::Text(
                    if self.settings.smooth_motion {
                        tr!("[ Smooth motion: on ]", "[ Плавное движение: вкл. ]")
                    } else {
                        tr!("[ Smooth motion: off ]", "[ Плавное движение: выкл. ]")
                    }
                    .into(),
                ));
            }
            Cmd::CycleTime => self.settings.time_mode = self.settings.time_mode.next(),
            Cmd::Frame { forward } => {
                if let Some(player) = &mut self.player
                    && !player.state.idle
                {
                    player.frame_step(forward);
                    self.osd.show(Osd::Frame { forward });
                }
            }
            Cmd::Start => self.seek_to(0.0, Seek::Keyframe),
            Cmd::End => {
                if let Some(d) = state.duration {
                    self.seek_to(d, Seek::Keyframe);
                }
            }
            Cmd::Volume(d) => self.run(ctx, frame, Cmd::SetVolume(state.volume + d)),
            Cmd::SetVolume(v) => {
                if let Some(player) = &mut self.player {
                    player.set_volume(v);
                    self.osd.show(Osd::Volume(player.state.volume));
                }
            }
            Cmd::ToggleMute => {
                if let Some(player) = &mut self.player {
                    player.set_mute(!player.state.mute);
                    self.osd.show(Osd::Mute(player.state.mute));
                }
            }
            Cmd::ToggleFullscreen => {
                self.set_fullscreen(ctx, frame, !frameless::is_fullscreen(ctx))
            }
            Cmd::Speed(steps) => {
                if let Some(player) = &self.player {
                    let speed = crate::format::speed_step(player.state.speed, steps);
                    self.run(ctx, frame, Cmd::SetSpeed(speed));
                }
            }
            Cmd::SetSpeed(speed) => {
                if let Some(player) = &mut self.player {
                    player.set_speed(speed);
                    self.osd.show(Osd::Speed(speed));
                }
            }
            Cmd::SetSubDelay(seconds) => {
                if let Some(player) = &mut self.player {
                    player.set_sub_delay(seconds);
                    self.osd.show(Osd::SubDelay(player.state.sub_delay));
                }
            }
            Cmd::SetAudioDelay(seconds) => {
                if let Some(player) = &mut self.player {
                    player.set_audio_delay(seconds);
                    self.osd.show(Osd::AudioDelay(player.state.audio_delay));
                }
            }
            Cmd::SubStep(lines) => {
                if let Some(player) = &mut self.player
                    && !state.idle
                {
                    player.sub_step(lines);
                }
            }
            Cmd::SubDelay(steps) => {
                if let Some(player) = &mut self.player {
                    let delay = if steps == 0.0 {
                        0.0
                    } else {
                        player.state.sub_delay + steps * crate::audio::DELAY_STEP
                    };
                    player.set_sub_delay(delay);
                    self.osd.show(Osd::SubDelay(player.state.sub_delay));
                }
            }
            Cmd::AudioDelay(steps) => {
                if let Some(player) = &mut self.player {
                    let delay = if steps == 0.0 {
                        0.0
                    } else {
                        player.state.audio_delay + steps * crate::audio::DELAY_STEP
                    };
                    player.set_audio_delay(delay);
                    self.osd.show(Osd::AudioDelay(player.state.audio_delay));
                }
            }
            Cmd::Escape => {
                // The big cover and the information window first, the
                // tracks window, the panel, the fullscreen playlist, then
                // fullscreen, the mini player, the interface after 7, then
                // the program (as its close button: the settings are saved).
                if self.info.close() {
                } else if self.tracks_window.open {
                    self.tracks_window.open = false;
                } else if self.video_panel.open {
                    self.video_panel.open = false;
                } else if frameless::is_fullscreen(ctx) && self.fullscreen_playlist {
                    self.fullscreen_playlist = false;
                } else if frameless::is_fullscreen(ctx) {
                    self.set_fullscreen(ctx, frame, false);
                } else if self.mini.is_some() {
                    self.leave_mini(ctx);
                } else if self.interface() == Interface::Bare {
                    self.run(ctx, frame, Cmd::ToggleInterface);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Cmd::ToggleInfo => self.info.toggle(),
            Cmd::ToggleInfoOverlay => self.settings.info_overlay = !self.settings.info_overlay,
            Cmd::ToggleVideoPanel => {
                self.video_panel.toggle();
                if self.video_panel.open {
                    self.tracks_window.open = false;
                }
            }
            Cmd::ShowEqualizer => self.video_panel.open_on(ui::picture::Tab::Sound),
            Cmd::SaveFrame => self.save_frame(&state),
            Cmd::SaveVideoMap => self.save_video_map(ctx, &state),
            Cmd::OpenRecent(i) => {
                if let Some(path) = self.settings.recent.get(i).cloned() {
                    if path.is_file() {
                        self.open(vec![path]);
                    } else {
                        self.settings.recent.remove(i);
                        self.osd.show_always(Osd::Text(tr!(
                            format!("Not found: {}", path.display()),
                            format!("Не найден: {}", path.display())
                        )));
                    }
                }
            }
            Cmd::ClearRecent => self.settings.recent.clear(),
            Cmd::ShowFrames => {
                if let Some(dir) = self.settings.frames_dir() {
                    match std::fs::create_dir_all(&dir) {
                        Ok(()) => {
                            win::shell_open(&dir);
                        }
                        Err(_) => self.osd.show_always(Osd::Text(folder_missing(&dir))),
                    }
                }
            }
            Cmd::SaveCover => self.save_cover(ctx),
            Cmd::ShowInFolder => {
                if let Some(path) = self
                    .playlist
                    .current_path()
                    .filter(|p| !playlist::is_url(p))
                {
                    win::show_in_folder(path);
                }
            }
            Cmd::Next => {
                if let Some(i) = self.playlist.next_in(self.settings.repeat) {
                    self.play(i);
                }
            }
            Cmd::Prev => {
                if let Some(i) = self.playlist.prev_in(self.settings.repeat) {
                    self.play(i);
                }
            }
            Cmd::Open => self.pick_files(ctx, false),
            Cmd::OpenUrl => self.url_dialog.open(),
            Cmd::Paste => self.paste(),
            Cmd::GoToTime => {
                if !state.idle {
                    self.goto_dialog.open(state.time, state.duration);
                }
            }
            Cmd::TogglePlaylist => {
                if frameless::is_fullscreen(ctx) {
                    self.fullscreen_playlist = !self.fullscreen_playlist;
                } else if self.interface() == Interface::Bare {
                    // Only the picture: no playlist to show or hide there,
                    // so the interface comes back with it.
                    self.settings.playlist_visible = true;
                    self.run(ctx, frame, Cmd::SetInterface(self.before_bare));
                } else {
                    self.settings.playlist_visible = !self.settings.playlist_visible;
                }
            }
            Cmd::TogglePlaylistWindow => {
                self.settings.playlist_detached = !self.settings.playlist_detached;
                self.settings.playlist_visible = true;
                if self.interface() == Interface::Bare && !frameless::is_fullscreen(ctx) {
                    self.run(ctx, frame, Cmd::SetInterface(self.before_bare));
                }
            }
            Cmd::About => self.about.toggle(),
            Cmd::Settings => self.settings_dialog.toggle(),
            Cmd::Delete => {
                if self.playlist_shown(ctx) {
                    self.playlist.remove_selected();
                }
            }
            Cmd::PlayIndex(i) => self.play(i),
            Cmd::PlaylistAdd => self.pick_files(ctx, true),
            Cmd::PlaylistRemoveSelected => self.playlist.remove_selected(),
            Cmd::PlaylistMove(slot) => self.playlist.move_selected(slot),
            Cmd::TogglePlaylistProgress => {
                self.settings.playlist_progress = !self.settings.playlist_progress;
            }
            Cmd::TogglePlaylistGroups => {
                self.settings.playlist_groups = !self.settings.playlist_groups;
            }
            Cmd::PlaylistSearch => {
                // Shown first, where it is shown now.
                if !self.playlist_shown(ctx) {
                    self.run(ctx, frame, Cmd::TogglePlaylist);
                }
                self.list_state.search();
                // In its own window the field takes the keys only once that
                // window has Windows' focus: else they went to the player.
                if self.settings.playlist_detached && !frameless::is_fullscreen(ctx) {
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::from_hash_of("playlist_window"),
                        egui::ViewportCommand::Focus,
                    );
                }
            }
            Cmd::ShowEntryInFolder(i) => {
                if let Some(path) = self
                    .playlist
                    .items()
                    .get(i)
                    .filter(|p| !playlist::is_url(p))
                {
                    win::show_in_folder(path);
                }
            }
            Cmd::PlaylistSort => self.playlist.sort(),
            Cmd::PlaylistClear => self.playlist.clear(),
            Cmd::PlaylistSave => self.save_playlist(ctx),
            Cmd::PlaylistShuffle => {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |d| d.as_nanos() as u64);
                self.playlist.shuffle(seed);
            }
            Cmd::CycleOnTop => self.run(ctx, frame, Cmd::SetOnTop(self.settings.on_top.next())),
            Cmd::SetOnTop(on_top) => {
                self.settings.on_top = on_top;
                self.osd.show(Osd::OnTop(on_top));
            }
            Cmd::CycleRepeat => self.run(ctx, frame, Cmd::SetRepeat(self.settings.repeat.next())),
            Cmd::SaveClip => self.save_clip(&state),
            Cmd::LoopA | Cmd::LoopB | Cmd::LoopClear | Cmd::CycleLoop => {
                self.ab_loop(cmd, &state);
            }
            Cmd::SetRepeat(repeat) => {
                self.settings.repeat = repeat;
                if let Some(player) = &mut self.player {
                    player.set_loop_file(repeat == Repeat::One);
                }
                self.osd.show(Osd::Repeat(repeat));
            }
            Cmd::ToggleTracks => {
                if !state.idle || self.tracks_window.open {
                    self.tracks_window.toggle();
                    // The two windows stand at the same place.
                    if self.tracks_window.open {
                        self.video_panel.open = false;
                    }
                }
            }
            Cmd::CycleAudio => self.cycle_track(Kind::Audio),
            Cmd::CycleSubs => self.cycle_track(Kind::Sub),
            Cmd::ToggleSubs => {
                let subs = tracks::of_kind(&state.tracks, Kind::Sub);
                match tracks::selected(&state.tracks, Kind::Sub) {
                    Some(t) => {
                        self.last_sub = Some(t.id);
                        self.select_track(Kind::Sub, None);
                    }
                    None => {
                        let back = self.last_sub.filter(|id| subs.iter().any(|t| t.id == *id));
                        if let Some(id) = back.or(subs.first().map(|t| t.id)) {
                            self.select_track(Kind::Sub, Some(id));
                        }
                    }
                }
            }
            Cmd::SelectAudio(id) => self.select_track(Kind::Audio, Some(id)),
            Cmd::SelectSub(id) => self.select_track(Kind::Sub, id),
            Cmd::LoadSubtitles => self.pick_subtitles(ctx),
            Cmd::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            Cmd::ToggleMaximize => {
                if !frameless::is_fullscreen(ctx) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                        !frameless::is_maximized(ctx),
                    ));
                }
            }
            Cmd::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Cmd::WindowScale(scale) => {
                if let Some(video) = self.picture_size(&state) {
                    self.size_window(ctx, frame, [video[0] * scale, video[1] * scale]);
                    self.osd.show(Osd::Scale(scale * 100.0));
                }
            }
            Cmd::FitScreen => {
                if let Some(video) = self.picture_size(&state) {
                    // Far bigger than any screen: `size_window` scales it
                    // down to the work area, keeping its shape.
                    self.size_window(ctx, frame, video.map(|v| v * 1e4));
                    self.osd.show(Osd::FitScreen);
                }
            }
            Cmd::FitWindow => {
                if let Some(video) = self.picture_size(&state) {
                    let ppp = f64::from(ctx.pixels_per_point());
                    let area = self.picture_area.size();
                    let area = [f64::from(area.x) * ppp, f64::from(area.y) * ppp];
                    self.size_window(ctx, frame, sizing::fitted(video, area));
                    self.osd.show(Osd::FitWindow);
                }
            }
            Cmd::SetAspect(aspect) => {
                let before = self.adjust.clone();
                self.adjust.aspect = aspect;
                if let Some(player) = &mut self.player {
                    player.adjust(&before, &self.adjust);
                }
                self.osd.show(Osd::Aspect(
                    (aspect != crate::adjust::Aspect::Auto).then(|| aspect.label().to_owned()),
                ));
            }
            Cmd::ResetPicture => {
                let before = std::mem::take(&mut self.adjust);
                if let Some(player) = &mut self.player {
                    player.adjust(&before, &self.adjust);
                }
                self.osd.show(Osd::ResetPicture);
            }
            Cmd::ResetSubStyle => {
                self.settings.sub = crate::sub_style::SubStyle::default();
                if let Some(player) = &mut self.player {
                    player.set_sub_style(&self.settings.sub);
                }
                self.osd.show(Osd::ResetSubStyle);
            }
            Cmd::ResetEqualizer => {
                // The filters follow in `sync_audio_filters`, next frame.
                self.settings.equalizer = crate::audio::Equalizer::default();
                self.osd.show(Osd::ResetEqualizer);
                ctx.request_repaint();
            }
            Cmd::ChapterStep(step) => {
                if !state.idle && !state.chapters.is_empty() {
                    let time = state.time.unwrap_or(0.0);
                    match crate::chapters::step(&state.chapters, time, step > 0) {
                        Some(i) => self.run(ctx, frame, Cmd::Chapter(i)),
                        // Before the first chapter: the file's start.
                        None if step < 0 => self.seek_to(0.0, Seek::Exact),
                        None => self.osd.show(Osd::Text(
                            tr!("[ The last chapter ]", "[ Это последняя глава ]").to_owned(),
                        )),
                    }
                }
            }
            Cmd::Rotate => {
                self.change_picture(|a| a.turn(1));
                self.osd.show(Osd::Rotation(self.adjust.rotate));
            }
            Cmd::CycleAspect => {
                use crate::adjust::Aspect;
                let i = Aspect::ALL.iter().position(|a| *a == self.adjust.aspect);
                let next = Aspect::ALL[i.map_or(0, |i| (i + 1) % Aspect::ALL.len())];
                self.run(ctx, frame, Cmd::SetAspect(next));
            }
            Cmd::CycleDeinterlace => {
                use crate::adjust::Deinterlace;
                self.change_picture(|a| {
                    let i = Deinterlace::ALL.iter().position(|d| *d == a.deinterlace);
                    a.deinterlace =
                        Deinterlace::ALL[i.map_or(0, |i| (i + 1) % Deinterlace::ALL.len())];
                });
                self.osd.show(Osd::Deinterlace(self.adjust.deinterlace));
            }
            Cmd::Color(color, step) => {
                self.change_picture(|a| a.set(color, (a.get(color) + step).clamp(-100, 100)));
                self.osd.show(Osd::Color(color, self.adjust.get(color)));
            }
            Cmd::Zoom(step) => {
                let range = crate::adjust::ZOOM_RANGE;
                self.change_picture(|a| {
                    a.zoom = (a.zoom + step).clamp(*range.start(), *range.end())
                });
                self.osd.show(Osd::Zoom(self.adjust.zoom));
            }
            Cmd::Pan(x, y) => {
                let max = crate::adjust::PAN_MAX;
                self.change_picture(|a| {
                    a.pan_x = (a.pan_x + x as f64).clamp(-max, max);
                    a.pan_y = (a.pan_y + y as f64).clamp(-max, max);
                });
                let (x, y) = (self.adjust.pan_x, self.adjust.pan_y);
                self.osd.show(Osd::Pan(x.round() as i64, y.round() as i64));
            }
            Cmd::ZoomReset => {
                self.change_picture(|a| {
                    let d = Adjust::default();
                    (a.zoom, a.pan_x, a.pan_y) = (d.zoom, d.pan_x, d.pan_y);
                });
                self.osd.show(Osd::Zoom(self.adjust.zoom));
            }
            // The filters follow in `sync_audio_filters`, next frame.
            Cmd::ToggleEqualizer => {
                let eq = &mut self.settings.equalizer;
                eq.on = !eq.on;
                self.osd.show(Osd::Equalizer(eq.on));
                ctx.request_repaint();
            }
            Cmd::ToggleNormalize => {
                self.settings.normalize = !self.settings.normalize;
                self.osd.show(Osd::Normalize(self.settings.normalize));
                ctx.request_repaint();
            }
            Cmd::ToggleVoiceBoost => {
                self.settings.voice_boost = !self.settings.voice_boost;
                self.osd.show(Osd::VoiceBoost(self.settings.voice_boost));
                ctx.request_repaint();
            }
            Cmd::AddBookmark => {
                if let (false, Some(time), Some(path)) =
                    (state.idle, state.time, self.playlist.current_path())
                {
                    crate::bookmarks::add(&mut self.settings.bookmarks, path, time, state.duration);
                    self.osd.show(Osd::Bookmark { added: true, time });
                }
            }
            Cmd::GoToBookmark(i) => {
                if let Some(b) = self.settings.bookmarks.get(i).cloned() {
                    self.go_to_bookmark(b, &state);
                }
            }
            Cmd::DeleteBookmark(i) => {
                if i < self.settings.bookmarks.len() {
                    self.settings.bookmarks.remove(i);
                }
            }
            Cmd::ClearBookmarks => self.settings.bookmarks.clear(),
            Cmd::ToggleUpmix => {
                self.settings.upmix = !self.settings.upmix;
                // On, but doing nothing now: say why, not only "on".
                let stereo = self
                    .player
                    .as_ref()
                    .and_then(|p| tracks::selected(&p.state.tracks, Kind::Audio))
                    .map(crate::audio::stereo_track);
                let why = if !self.settings.upmix {
                    None
                } else if !self.settings.audio_channels.takes_upmix() {
                    Some(tr!(
                        "[ Stereo to 5.1 on, but Channels makes the sound mono or stereo ]",
                        "[ Стерео в 5.1 включено, но «Каналы» сводят звук в моно или стерео ]"
                    ))
                } else if stereo == Some(false) {
                    Some(tr!(
                        "[ Stereo to 5.1 on, for stereo tracks; this one is not ]",
                        "[ Стерео в 5.1 включено, для стереодорожек; эта не стерео ]"
                    ))
                } else {
                    None
                };
                match why {
                    Some(why) => self.osd.show(Osd::Text(why.to_owned())),
                    None => self.osd.show(Osd::Upmix(self.settings.upmix)),
                }
                ctx.request_repaint();
            }
            Cmd::ToggleInterface => {
                let interface = if self.interface() == Interface::Bare {
                    self.before_bare
                } else {
                    Interface::Bare
                };
                self.run(ctx, frame, Cmd::SetInterface(interface));
            }
            Cmd::CycleInterface => {
                let next = self.interface().next();
                self.run(ctx, frame, Cmd::SetInterface(next));
                self.osd.show(Osd::Interface(next));
            }
            Cmd::ToggleMini => self.toggle_mini(ctx, frame),
            Cmd::SetInterface(interface) => {
                if self.settings.interface != Interface::Bare {
                    self.before_bare = self.settings.interface;
                }
                self.settings.interface = interface;
                self.fit_interface(ctx);
            }
            Cmd::ToggleFill => {
                if self.fill {
                    // Fullscreen stays, with the black bars.
                    self.set_fill(false);
                    self.osd.show(Osd::Fill(false));
                } else if video_size(&state).is_some() {
                    self.set_fullscreen(ctx, frame, true);
                    self.set_fill(true);
                    self.osd.show(Osd::Fill(true));
                }
            }
        }
    }

    /// Seek by `delta` seconds and say so on the OSD.
    pub(super) fn seek_by(&mut self, delta: f64, how: Seek) {
        let Some(player) = &mut self.player else {
            return;
        };
        if player.state.idle {
            return;
        }
        player.seek_by(delta, how);
        let s = &player.state;
        let target = s.time.unwrap_or(0.0) + delta;
        let time = s.duration.map_or(target, |d| target.min(d)).max(0.0);
        self.osd.show(Osd::Seek {
            delta,
            exact: how == Seek::Exact,
            time,
            duration: s.duration,
        });
    }

    pub(super) fn seek_to(&mut self, seconds: f64, how: Seek) {
        let Some(player) = &mut self.player else {
            return;
        };
        if player.state.idle {
            return;
        }
        player.seek_to(seconds, how);
        self.osd.show(Osd::Jump {
            time: seconds,
            duration: player.state.duration,
        });
    }

    pub(super) fn set_fullscreen(&mut self, ctx: &egui::Context, frame: &eframe::Frame, on: bool) {
        if on != frameless::is_fullscreen(ctx) {
            frameless::set_fullscreen(ctx, frame, on);
        }
    }

    /// The A-B repeat: its start or its end at the position now, or off;
    /// `CycleLoop` does the next of the three. An end without a start
    /// starts at the beginning; one before the start swaps them.
    pub(super) fn ab_loop(&mut self, cmd: Cmd, state: &State) {
        let Some(player) = &mut self.player else {
            return;
        };
        let (a, b) = (state.loop_a, state.loop_b);
        let cmd = match cmd {
            Cmd::CycleLoop => match (a, b) {
                (None, _) => Cmd::LoopA,
                (Some(_), None) => Cmd::LoopB,
                (Some(_), Some(_)) => Cmd::LoopClear,
            },
            cmd => cmd,
        };
        let now = state.time.filter(|_| !state.idle);
        let (a, b) = match (cmd, now) {
            (Cmd::LoopClear, _) => (None, None),
            // A start after the end leaves no end.
            (Cmd::LoopA, Some(t)) => (Some(t), b.filter(|b| *b > t + MIN_LOOP)),
            (Cmd::LoopB, Some(t)) => {
                let a = a.unwrap_or(0.0);
                if (t - a).abs() < MIN_LOOP {
                    return;
                }
                (Some(a), Some(t))
            }
            _ => return,
        };
        player.set_loop(a, b);
        self.osd.show(Osd::Loop {
            a: player.state.loop_a,
            b: player.state.loop_b,
        });
    }

    /// Change the picture's adjustments and give mpv what changed.
    pub(super) fn change_picture(&mut self, change: impl FnOnce(&mut Adjust)) {
        let before = self.adjust.clone();
        change(&mut self.adjust);
        if let Some(player) = &mut self.player
            && self.adjust != before
        {
            player.adjust(&before, &self.adjust);
        }
    }

    /// The picture filling the screen, cropped, or whole (`fill`).
    pub(super) fn set_fill(&mut self, on: bool) {
        self.fill = on;
        if let Some(player) = &mut self.player {
            player.set_fill(on);
        }
    }

    /// Go to bookmark `b`: a seek in the file playing, else its file played
    /// from there (its playlist entry, or the file opened).
    pub(super) fn go_to_bookmark(&mut self, b: crate::bookmarks::Bookmark, state: &State) {
        let playing = self.playlist.current_path().filter(|_| !state.idle);
        if playing.is_some_and(|p| b.is_of(p)) {
            if let Some(player) = &mut self.player {
                player.seek_to(b.time, Seek::Exact);
            }
            self.osd.show(Osd::Bookmark {
                added: false,
                time: b.time,
            });
            return;
        }
        let entry = self.playlist.items().iter().position(|p| b.is_of(p));
        // `play` starts the file there (paths compared without case).
        self.start_at = Some((b.path.clone(), b.time));
        match entry {
            Some(i) => self.play(i),
            None => self.open(vec![b.path]),
        }
    }

    /// Play audio track or show subtitle track `id` (`None`: no subtitles)
    /// and say so on the OSD.
    pub(super) fn select_track(&mut self, kind: Kind, id: Option<i64>) {
        let Some(player) = &mut self.player else {
            return;
        };
        if player.state.idle {
            return;
        }
        player.select_track(kind, id);
        let all = &player.state.tracks;
        // A choice by hand: what the next new file is matched against.
        let like = id
            .and_then(|id| all.iter().find(|t| t.kind == kind && t.id == id))
            .map(tracks::Likeness::of);
        match kind {
            Kind::Audio => self.settings.last_audio = like.or(self.settings.last_audio.take()),
            Kind::Sub => self.settings.last_sub = Some(like),
            Kind::Video => {}
        }
        let now = id
            .and_then(|id| all.iter().find(|t| t.kind == kind && t.id == id))
            .map(|t| t.summary_in(crate::i18n::lang(), all));
        self.osd.show(Osd::Track {
            audio: kind == Kind::Audio,
            now,
        });
    }

    /// The next audio track, or the next subtitles (then none, then the
    /// first again).
    pub(super) fn cycle_track(&mut self, kind: Kind) {
        let next = self
            .player
            .as_ref()
            .and_then(|p| tracks::next(&p.state.tracks, kind));
        if let Some(id) = next {
            self.select_track(kind, id);
        }
    }
}
