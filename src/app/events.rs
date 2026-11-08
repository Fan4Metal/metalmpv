//! The player's events and what follows the file playing: positions,
//! tracks like the last chosen, subtitle lines, the container's facts,
//! the cover, the audio filters.

use super::*;

impl App {
    /// Take what the player and other starts have to say; also called
    /// while the window is minimised, so a playlist goes on.
    pub(super) fn take_events(&mut self) {
        let received = crate::instance::take();
        if let Some(&(_, last)) = received.last() {
            let batch = same_batch(self.last_received, received[0].1);
            log::debug!(
                "{} files from other starts, {:?} after the last ones: {}",
                received.len(),
                self.last_received
                    .map(|t| received[0].1.saturating_duration_since(t)),
                if batch { "added" } else { "opened" }
            );
            let paths: Vec<PathBuf> = received.into_iter().map(|(p, _)| p).collect();
            if batch {
                self.add(paths);
            } else {
                self.open(paths);
            }
            self.last_received = Some(last);
        }
        let events = match &mut self.player {
            Some(player) => player.poll(),
            None => return,
        };
        for event in events {
            match event {
                Happened::FileLoaded => {
                    log::debug!("file loaded at {:.0} ms", crate::since_start());
                    if let Some(path) = self.playlist.current_path().filter(|p| p.is_file()) {
                        self.settings.played(path);
                    }
                    self.file_error = None;
                    self.failed_in_row = 0;
                    self.pick_ready = self.pick_tracks;
                    self.fit_on_open = self.settings.window_on_open != WindowOnOpen::Keep;
                }
                Happened::Expanded { entries, playing } => {
                    // The entries this app would load itself, as mpv would
                    // (the one it plays it has let through already).
                    let remote = self.playlist.current_path().is_some_and(playlist::is_url);
                    let mut at = 0;
                    let entries = entries
                        .into_iter()
                        .enumerate()
                        .map(|(i, (file, title))| (i, PathBuf::from(file), title))
                        .filter(|(i, path, _)| *i == playing || playlist::safe_entry(path, remote))
                        .enumerate()
                        .map(|(kept, (i, path, title))| {
                            if i == playing {
                                at = kept;
                            }
                            (path, title)
                        })
                        .collect();
                    self.playlist.expand_current(entries, at);
                }
                Happened::NoData(end) if self.settings.stay_in_incomplete => {
                    // mpv stopped at the end of the data (keep-open): it
                    // stays there, paused; Play goes on from there.
                    if let Some(player) = &mut self.player {
                        player.set_pause(true);
                    }
                    // Kept while it stays there: why it stopped.
                    self.osd.show_at_end(match end {
                        DataEnd::Seek(t) => Osd::NoData(t),
                        DataEnd::Played(t) => Osd::DataEnds(t),
                    });
                }
                Happened::Eof | Happened::NoData(_) => {
                    // Taken as the end: Play starts the file again.
                    if let Some(player) = &mut self.player {
                        player.forget_data_end();
                    }
                    if let Some(i) = self.playlist.next_in(self.settings.repeat) {
                        self.play(i);
                    }
                }
                Happened::Failed(e) => {
                    let name = self.playlist.current_path().and_then(Path::file_name);
                    let name = name
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    log::warn!("cannot play {name}: {e}");
                    self.osd.show_always(Osd::Text(tr!(
                        format!("Cannot play {name}: {e}"),
                        format!("Не удалось воспроизвести {name}: {e}")
                    )));
                    self.file_error = Some(e);
                    // A broken file in a list is skipped, round to the first
                    // with the list repeated, until each has failed once.
                    self.failed_in_row += 1;
                    if self.failed_in_row < self.playlist.len()
                        && let Some(i) = self.playlist.next_in(self.settings.repeat)
                    {
                        self.play(i);
                    }
                }
                Happened::Restarted => {}
                Happened::FrameSaved(error) => self.frame_saved(error),
                // The names asked for would wait for good, and the next
                // answers would be taken for them.
                Happened::RepliesLost => self.frames_saving.clear(),
                Happened::SubStepped(delay) => self.osd.show(match delay {
                    Some(delay) => Osd::SubDelay(delay),
                    None => Osd::NoSubLine,
                }),
                Happened::Shutdown => {
                    self.engine_error = Some(crate::player::EngineError::Failed(
                        "mpv has shut down".into(),
                    ));
                }
            }
        }
        self.pick_like_tracks();
    }

    /// In a file loaded without tracks of its own to come back to, the
    /// audio and subtitles most like the last chosen by hand
    /// (`prefer_last_tracks`); nothing alike leaves mpv's choice.
    pub(super) fn pick_like_tracks(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        if !self.pick_ready || player.state.tracks.is_empty() {
            return;
        }
        self.pick_tracks = false;
        self.pick_ready = false;
        let all = &player.state.tracks;
        let audio = self
            .settings
            .last_audio
            .as_ref()
            .and_then(|like| tracks::most_like(all, Kind::Audio, like));
        let subs_off = self.settings.track_prefs.sub_mode == tracks::SubMode::Off;
        let sub = match &self.settings.last_sub {
            // Subtitles turned off in the settings stay off: the subtitles
            // chosen by hand once do not bring them back in every file.
            Some(Some(_)) if subs_off => None,
            Some(Some(like)) => tracks::most_like(all, Kind::Sub, like).map(Some),
            // None by hand last time: none now either.
            Some(None) => Some(None),
            None => None,
        };
        let now = |kind| tracks::selected(all, kind).map(|t| t.id);
        let (audio_now, sub_now) = (now(Kind::Audio), now(Kind::Sub));
        if let Some(id) = audio.filter(|id| Some(*id) != audio_now) {
            log::debug!("audio track {id}, like the last chosen");
            player.select_track(Kind::Audio, Some(id));
        }
        if let Some(id) = sub.filter(|id| *id != sub_now) {
            log::debug!("subtitles {id:?}, like the last chosen");
            player.select_track(Kind::Sub, id);
        }
    }

    /// Read the lines of the subtitle file shown (one of its own) for the
    /// panel's timeline, once per file, while the panel is open (it lays
    /// the Subtitles tab out even unseen).
    /// The file (up to 32 MB, maybe on a network share) is read on a thread
    /// of its own: read here, it held the frame.
    pub(super) fn update_sub_lines(&mut self, ctx: &egui::Context, state: &State) {
        if !self.video_panel.open {
            return;
        }
        let file = tracks::selected(&state.tracks, Kind::Sub)
            .filter(|t| t.external)
            .and_then(|t| t.external_filename.clone());
        let Some(file) = file else {
            self.sub_lines = None;
            return;
        };
        if let Some(slot) = self.sub_lines.as_mut().filter(|s| s.file == file) {
            if let Some(read) = slot.reading.as_ref().and_then(|rx| rx.try_recv().ok()) {
                log::debug!(
                    "subtitle lines of {file}: {}",
                    read.as_ref().map_or(0, Vec::len)
                );
                slot.lines = read;
                slot.reading = None;
            }
            return;
        }
        let (send, reading) = std::sync::mpsc::channel();
        let (path, wake) = (PathBuf::from(&file), ctx.clone());
        let started = crate::worker::spawn("subtitle lines", move || {
            let _ = send.send(crate::sub_lines::read(&path));
            wake.request_repaint();
        });
        if let Err(e) = started {
            log::warn!("cannot read the subtitle lines: {e}");
        }
        self.sub_lines = Some(SubLines {
            file,
            lines: None,
            reading: Some(reading),
        });
    }

    /// Read what the container of the file playing says while its
    /// information is shown (once per file), and take it when read.
    pub(super) fn update_facts(&mut self, ctx: &egui::Context, state: &State) {
        let wanted = self
            .playlist
            .current_path()
            .filter(|p| self.info_wanted() && !state.idle && !playlist::is_url(p));
        if let Some(path) = wanted
            && self.facts.as_ref().is_none_or(|f| f.path != path)
        {
            self.facts = Some(FactsSlot {
                path: path.to_path_buf(),
                loading: Some(crate::probe::load(path.to_path_buf(), ctx)),
                loaded: None,
            });
        }
        let Some(slot) = &mut self.facts else {
            return;
        };
        if let Some(rx) = &slot.loading {
            match rx.try_recv() {
                Ok(facts) => {
                    slot.loading = None;
                    slot.loaded = Some(facts);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => slot.loading = None,
            }
        }
    }

    /// Give mpv the audio filters the settings make, when they change:
    /// the voice's and the upmix depend on the channels of the track
    /// playing, so they follow a change of track (`audio::filters`).
    pub(super) fn sync_audio_filters(&mut self, ctx: &egui::Context, state: &State) {
        let Some(player) = &mut self.player else {
            return;
        };
        // While a slider is dragged (a band of the equalizer), the chain is
        // rebuilt once it is let go: each step rebuilt it, and seeked
        // under the normalization.
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let layout = if state.tracks.is_empty() {
            self.af_layout.clone()
        } else {
            tracks::selected(&state.tracks, Kind::Audio)
                .and_then(|t| crate::audio::voice_layout(t.layout.as_deref(), t.channels))
        };
        let upmix = (self.settings.upmix
            && self.settings.audio_channels.takes_upmix()
            && layout.as_deref().is_some_and(crate::audio::is_stereo))
        .then_some(self.settings.upmix_lfe);
        let voice = layout
            .as_deref()
            .filter(|_| self.settings.voice_boost)
            .map(|layout| (layout, self.settings.voice_db));
        let af = crate::audio::filters(
            self.settings
                .normalize
                .then_some(self.settings.normalize_strength),
            &self.settings.equalizer,
            voice,
            upmix,
            self.settings.subwoofer,
            layout.as_deref().is_some_and(crate::audio::has_lfe),
        );
        self.af_layout = layout;
        if af != self.af_applied {
            player.set_audio_filters(&af);
            self.af_applied = af;
        }
    }

    /// Look for the cover of the file playing once it is loaded, and take
    /// it when it is found.
    pub(super) fn update_cover(&mut self, ctx: &egui::Context, state: &State) {
        // Another entry's cover goes at once: an address looks for none,
        // and kept the last file's.
        let current = self.playlist.current_path();
        if self
            .cover
            .as_ref()
            .is_some_and(|c| Some(c.path.as_path()) != current)
        {
            self.cover = None;
        }
        let wanted = self
            .playlist
            .current_path()
            .filter(|p| !state.idle && !state.tracks.is_empty() && !playlist::is_url(p));
        if let Some(path) = wanted
            && self.cover.as_ref().is_none_or(|c| c.path != path)
        {
            self.cover = Some(CoverSlot {
                path: path.to_path_buf(),
                loading: Some(crate::cover::load(path.to_path_buf(), ctx)),
                loaded: None,
            });
        }
        let Some(slot) = &mut self.cover else {
            return;
        };
        let Some(rx) = &slot.loading else {
            return;
        };
        match rx.try_recv() {
            Ok(found) => {
                slot.loading = None;
                slot.loaded = found.map(|mut cover| {
                    let image = std::mem::take(&mut cover.image);
                    let options = egui::TextureOptions {
                        magnification: egui::TextureFilter::Linear,
                        minification: egui::TextureFilter::Linear,
                        wrap_mode: egui::TextureWrapMode::ClampToEdge,
                        mipmap_mode: Some(egui::TextureFilter::Linear),
                    };
                    let texture = ctx.load_texture("cover", image, options);
                    (cover, texture)
                });
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => slot.loading = None,
        }
    }

    /// Keep where the file playing is now (`resume_playback`) and which
    /// tracks it plays (`remember_tracks`; only when there is a choice: more
    /// than one audio track, or subtitles).
    pub(super) fn remember_position(&mut self) {
        let (resume, tracks) = (self.settings.resume_playback, self.settings.remember_tracks);
        if !resume && !tracks {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let state = &player.state;
        // Not while a file is still loading: its tracks are not known yet.
        if state.idle || state.seeking || state.tracks.is_empty() {
            return;
        }
        // Stopped where the data ran out: there, not mpv's position (an MP4
        // gives the duration after a seek past the data). At the file's own
        // end, watched, even before its duration: one estimated too long
        // (a VBR MP3 without its Xing header) left it 10 minutes short of
        // it, and it came back at its end.
        let time = match player.ran_out_at() {
            Some(at) => Some(at),
            None if state.eof => state.duration,
            None => state.time,
        };
        let (Some(path), Some(time), Some(duration)) = (&state.path, time, state.duration) else {
            return;
        };
        let choice = tracks::of_kind(&state.tracks, Kind::Audio).len() > 1
            || !tracks::of_kind(&state.tracks, Kind::Sub).is_empty();
        let audio = match tracks::selected(&state.tracks, Kind::Audio) {
            Some(t) => Some(Some(t.id)),
            // None of the file's audio tracks plays: mpv dropped it (the
            // device failed), as the sound cannot be turned off here; what
            // was kept before stays.
            None if !tracks::of_kind(&state.tracks, Kind::Audio).is_empty() => self
                .positions
                .get(Path::new(path))
                .and_then(|left| left.tracks.as_ref())
                .map(|t| t.audio),
            None => Some(None),
        };
        let sub = tracks::selected(&state.tracks, Kind::Sub);
        // Subtitles of a file of their own by that file: their id changes.
        let sub_file = sub
            .filter(|t| t.external)
            .and_then(|t| t.external_filename.as_deref())
            .map(PathBuf::from);
        let chosen = (tracks && choice)
            .then_some(audio)
            .flatten()
            .map(|audio| Tracks {
                audio,
                sub: sub.filter(|_| sub_file.is_none()).map(|t| t.id),
                sub_file,
            });
        let time = resume.then_some((time, duration));
        self.positions.remember(Path::new(path), time, chosen);
    }

    /// The file's information is shown: its window or its chips over the
    /// picture (not in the mini player).
    pub(super) fn info_wanted(&self) -> bool {
        self.info.open || (self.settings.info_overlay && self.mini.is_none())
    }
}

/// The cover of a file: looked for on a thread of its own, then decoded
/// with its texture.
pub(super) struct CoverSlot {
    pub(super) path: PathBuf,
    pub(super) loading: Option<std::sync::mpsc::Receiver<Option<Cover>>>,
    pub(super) loaded: Option<(Cover, TextureHandle)>,
}

/// The lines of a subtitle file for the panel's timeline.
pub(super) struct SubLines {
    pub(super) file: String,
    /// `None` while reading or when the file is not one read.
    pub(super) lines: Option<Vec<crate::sub_lines::Line>>,
    pub(super) reading: Option<std::sync::mpsc::Receiver<Option<Vec<crate::sub_lines::Line>>>>,
}

pub(super) struct FactsSlot {
    pub(super) path: PathBuf,
    pub(super) loading: Option<std::sync::mpsc::Receiver<crate::probe::Facts>>,
    pub(super) loaded: Option<crate::probe::Facts>,
}

/// What the information window and the chips over the picture tell of: the
/// file playing and what its container says once read.
pub(super) fn info_subject<'a>(
    playlist: &'a Playlist,
    facts: Option<&'a FactsSlot>,
    state: &'a State,
) -> crate::info::Subject<'a> {
    let path = playlist.current_path();
    crate::info::Subject {
        path,
        state,
        facts: facts
            .filter(|f| Some(f.path.as_path()) == path)
            .and_then(|f| f.loaded.as_ref()),
    }
}
