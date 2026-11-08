//! Files in and out: the dialogs and their answers, frames, maps, clips,
//! the cover and the playlist saved, files dropped and pasted.

use super::*;

impl App {
    /// Save the frame on screen into the frames' folder (Pictures\MetalMPV
    /// unless chosen) as `<file> <position>.<png|jpg>`, never over another;
    /// what stops it is said on the OSD.
    pub(super) fn save_frame(&mut self, state: &State) {
        let ext = self.settings.frames_format.name();
        let (Some(_), Some(dir)) = (&self.player, self.settings.frames_dir()) else {
            return;
        };
        if state.idle {
            return;
        }
        if video_size(state).is_none() {
            self.osd.show_always(Osd::Text(
                tr!(
                    "[ No picture to save ]",
                    "[ Нет изображения для сохранения ]"
                )
                .to_owned(),
            ));
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("cannot make {}: {e}", dir.display());
            self.osd.show_always(Osd::Text(folder_missing(&dir)));
            return;
        }
        let stem = self.frame_stem();
        let time = crate::format::time(state.time, true).replace(':', "-");
        let mut path = dir.join(format!("{stem} {time}.{ext}"));
        for n in 2.. {
            if !path.exists() && !self.frames_saving.contains(&path) {
                break;
            }
            path = dir.join(format!("{stem} {time} ({n}).{ext}"));
        }
        if let Some(player) = &mut self.player {
            player.save_frame(&path);
        }
        self.frames_saving.push_back(path);
    }

    /// The start of a saved frame's or map's name: a channel's name (a
    /// stream's file is all "index"), else the file's; without the
    /// characters a file name cannot hold.
    pub(super) fn frame_stem(&self) -> String {
        let current = self.playlist.current_path();
        current
            .and_then(|p| self.playlist.channel(p)?.title.clone())
            .or_else(|| {
                current
                    .and_then(Path::file_stem)
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .map(|s| s.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_"))
            .unwrap_or_else(|| "frame".into())
    }

    /// Make a video map of the file playing (`video_map.rs`): its frames
    /// in a grid on one picture, written into the frames' folder on a
    /// thread of its own while it plays on; `follow_video_map` says how it
    /// goes and puts it on the clipboard.
    pub(super) fn save_video_map(&mut self, ctx: &egui::Context, state: &State) {
        if state.idle {
            return;
        }
        if self.video_map.is_some() {
            self.osd.show_always(Osd::Text(
                tr!(
                    "[ The video map is being made ]",
                    "[ Карта видео уже создаётся ]"
                )
                .to_owned(),
            ));
            return;
        }
        let Some(input) = state.path.as_deref().map(PathBuf::from) else {
            return;
        };
        if video_size(state).is_none() {
            self.osd.show_always(Osd::Text(
                tr!("[ No picture for a map ]", "[ Нет изображения для карты ]").to_owned(),
            ));
            return;
        }
        if !state.duration.is_some_and(|d| d > 0.0) {
            self.osd.show_always(Osd::Text(
                tr!(
                    "[ A map needs a file of known duration ]",
                    "[ Для карты нужна известная длительность ]"
                )
                .to_owned(),
            ));
            return;
        }
        let Some(dir) = self.settings.frames_dir() else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("cannot make {}: {e}", dir.display());
            self.osd.show_always(Osd::Text(folder_missing(&dir)));
            return;
        }
        let current = self.playlist.current_path();
        let title = current
            .and_then(|p| self.playlist.channel(p)?.title.clone())
            .or_else(|| {
                current
                    .and_then(Path::file_name)
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| input.to_string_lossy().into_owned());
        let layout = self.settings.video_map;
        // The picture as shown, and deinterlaced as it is.
        let mut picture = self.picture_options();
        picture.push(("deinterlace", self.adjust.deinterlace.option().to_owned()));
        let request = crate::video_map::Request {
            output: crate::video_map::file_name(&dir, &self.frame_stem(), layout.format),
            input,
            title,
            layout,
            hwdec: if self.settings.hwdec {
                "auto-copy-safe"
            } else {
                "no"
            },
            picture,
            copy: self.settings.video_map_copy,
        };
        let wake = ctx.clone();
        match crate::video_map::Job::start(request, move || wake.request_repaint()) {
            Ok(job) => {
                self.osd.show_always(Osd::Text(map_progress(0, job.frames)));
                self.video_map = Some((job, 0));
            }
            Err(e) => self.osd.show_always(Osd::Text(map_failed(&e))),
        }
    }

    /// How the video map goes: the frames taken on the OSD, then where it
    /// was written and whether it went on the clipboard.
    pub(super) fn follow_video_map(&mut self) {
        let Some((job, shown)) = &mut self.video_map else {
            return;
        };
        match job.take() {
            None => {
                let done = job.progress();
                if done != *shown {
                    *shown = done;
                    let text = map_progress(done, job.frames);
                    self.osd.show_always(Osd::Text(text));
                }
            }
            Some(Ok(made)) => {
                let name = made
                    .output
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.osd.show_always(Osd::Text(if made.copied {
                    tr!(
                        format!("[ Video map saved and copied: {name} ]"),
                        format!("[ Карта видео сохранена и скопирована: {name} ]")
                    )
                } else {
                    tr!(
                        format!("[ Video map saved: {name} ]"),
                        format!("[ Карта видео сохранена: {name} ]")
                    )
                }));
                self.video_map = None;
            }
            Some(Err(e)) => {
                log::warn!("video map: {e}");
                self.osd.show_always(Osd::Text(map_failed(&e)));
                self.video_map = None;
            }
        }
    }

    /// mpv wrote the frame asked for first, or could not (`error`).
    pub(super) fn frame_saved(&mut self, error: Option<String>) {
        let path = self.frames_saving.pop_front();
        let name = path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.osd.show_always(Osd::Text(match error {
            None => tr!(
                format!("[ Frame saved: {name} ]"),
                format!("[ Кадр сохранён: {name} ]")
            ),
            Some(e) => {
                log::warn!("cannot save the frame {name}: {e}");
                tr!(
                    format!("Cannot save the frame: {e}"),
                    format!("Не удалось сохранить кадр: {e}")
                )
            }
        }));
    }

    /// The folder of the entry playing, for a dialog to start in.
    pub(super) fn current_folder(&self) -> Option<PathBuf> {
        self.playlist
            .current_path()
            .filter(|p| !playlist::is_url(p))
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }

    /// Show a file dialog (`dialog.rs`); its answer comes to
    /// [`Self::answer`].
    pub(super) fn ask(
        &mut self,
        ctx: &egui::Context,
        ask: Ask,
        make: impl FnOnce() -> rfd::FileDialog + Send + 'static,
        run: fn(rfd::FileDialog) -> Vec<PathBuf>,
    ) {
        self.dialogs.start(ctx, self.hwnd, ask, make, run);
    }

    /// Write the playlist to an M3U8 file the user names.
    pub(super) fn save_playlist(&mut self, ctx: &egui::Context) {
        if self.playlist.is_empty() {
            return;
        }
        let dir = self.current_folder();
        let make = move || {
            let dialog = rfd::FileDialog::new()
                .set_title(tr!("Save the playlist", "Сохранить плейлист"))
                .set_file_name(tr!("Playlist.m3u8", "Плейлист.m3u8"))
                .add_filter(tr!("M3U8 playlist", "Плейлист M3U8"), &["m3u8"]);
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::SavePlaylist, make, dialog::save_file);
    }

    pub(super) fn save_cover(&mut self, ctx: &egui::Context) {
        let Some((cover, _)) = self.cover.as_ref().and_then(|c| c.loaded.as_ref()) else {
            return;
        };
        let path = self.playlist.current_path();
        let stem = path
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cover".into());
        let extension = cover.extension;
        let bytes = cover.bytes.clone();
        let dir = self.current_folder();
        let make = move || {
            let dialog = rfd::FileDialog::new()
                .set_title(tr!("Save the cover", "Сохранить обложку"))
                .set_file_name(format!("{stem}.{extension}"))
                .add_filter(extension.to_uppercase(), &[extension]);
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::SaveCover(bytes), make, dialog::save_file);
    }

    /// Open the Save the A-B stretch window for the file playing, its
    /// tracks shown and the subtitles' look.
    pub(super) fn save_clip(&mut self, state: &State) {
        // A stretch being saved, its window hidden: shown again.
        if self.clip_dialog.reveal() {
            return;
        }
        let (Some(from), Some(to)) = (state.loop_a, state.loop_b) else {
            self.osd.show_always(Osd::Text(
                tr!(
                    "[ Set the A-B repeat's start and end first ]",
                    "[ Сначала задайте начало и конец повтора A-B ]"
                )
                .to_owned(),
            ));
            return;
        };
        let Some(input) = self.playlist.current_path().filter(|_| !state.idle) else {
            return;
        };
        let sub = tracks::selected(&state.tracks, Kind::Sub);
        let subs = match sub {
            None => crate::clip::Subs::None,
            Some(t) => match t.external_filename.as_deref().filter(|_| t.external) {
                Some(path) => crate::clip::Subs::File {
                    path: PathBuf::from(path),
                    embedded: state
                        .tracks
                        .iter()
                        .filter(|t| t.kind == Kind::Sub && !t.external)
                        .count() as i64,
                },
                None => crate::clip::Subs::Own(t.id),
            },
        };
        let folder = self
            .current_folder()
            .or_else(|| self.settings.frames_dir())
            .unwrap_or_else(std::env::temp_dir);
        // The subtitles as the player draws them.
        let mut sub_style = self.settings.sub.options().to_vec();
        let ass = if self.settings.sub_ass_override {
            "force"
        } else {
            "scale"
        };
        sub_style.push(("sub-ass-override", ass.to_owned()));
        let audio = tracks::selected(&state.tracks, Kind::Audio);
        let stretch = ui::clip::Stretch {
            input: input.to_path_buf(),
            from,
            to,
            audio: audio.map(|t| t.id),
            channels: audio.and_then(|t| t.channels),
            subs,
            sub_style,
            audio_delay: state.audio_delay,
            sub_delay: state.sub_delay,
            deinterlace: self.adjust.deinterlace.option(),
            folder,
        };
        self.clip_dialog.open(stretch);
    }

    /// The Save dialog of the stretch's file, starting at `path`.
    pub(super) fn browse_clip(&mut self, ctx: &egui::Context, path: PathBuf) {
        let make = move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title(tr!("Save the A-B stretch", "Сохранить фрагмент A-B"))
                .add_filter("MP4", &["mp4"]);
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
            match path.parent().filter(|p| p.is_dir()) {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::SaveClip, make, dialog::save_file);
    }

    /// Do what a file dialog was opened for with the paths chosen.
    pub(super) fn answer(&mut self, answer: dialog::Answer) {
        let Some(path) = answer.paths.first().cloned() else {
            return;
        };
        match answer.ask {
            Ask::Open { add: true } => self.add(answer.paths),
            Ask::Open { add: false } => self.open(answer.paths),
            Ask::Subtitles(file) => {
                // Not for another file than the one they were chosen for:
                // it may have ended, or another been chosen, meanwhile.
                if !self.state_idle() && self.playlist.current_path() == Some(file.as_path()) {
                    self.add_subtitles(&path);
                } else {
                    log::info!("subtitles for {} dropped: it plays no more", file.display());
                }
            }
            Ask::SavePlaylist => {
                if let Err(e) = std::fs::write(&path, playlist::m3u_text(&self.playlist)) {
                    log::warn!("cannot save the playlist to {}: {e}", path.display());
                    self.osd.show_always(Osd::Text(tr!(
                        format!("Cannot save the playlist: {e}"),
                        format!("Не удалось сохранить плейлист: {e}")
                    )));
                }
            }
            Ask::SaveCover(bytes) => {
                if let Err(e) = std::fs::write(&path, bytes) {
                    log::warn!("cannot save the cover to {}: {e}", path.display());
                    self.osd.show_always(Osd::Text(tr!(
                        format!("Cannot save the cover: {e}"),
                        format!("Не удалось сохранить обложку: {e}")
                    )));
                }
            }
            Ask::FramesFolder => self.settings.frames_folder = Some(path),
            Ask::SaveClip => self.clip_dialog.set_path(&path),
        }
    }

    /// Ask for media files to play (`add`: append them to the playlist).
    pub(super) fn pick_files(&mut self, ctx: &egui::Context, add: bool) {
        let dir = self.current_folder();
        let make = move || {
            let dialog = rfd::FileDialog::new()
                .set_title(if add {
                    tr!("Add to the playlist", "Добавить в плейлист")
                } else {
                    tr!("Open", "Открыть")
                })
                .add_filter(
                    tr!("Media files and playlists", "Медиафайлы и плейлисты"),
                    &[playlist::MEDIA_EXTENSIONS, playlist::PLAYLIST_EXTENSIONS].concat(),
                )
                .add_filter(tr!("All files", "Все файлы"), &["*"]);
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::Open { add }, make, dialog::pick_files);
    }

    /// Ask for a subtitle file for the playing video.
    pub(super) fn pick_subtitles(&mut self, ctx: &egui::Context) {
        let Some(file) = self.playlist.current_path().filter(|_| !self.state_idle()) else {
            return;
        };
        let file = file.to_path_buf();
        let dir = self.current_folder();
        let make = move || {
            let dialog = rfd::FileDialog::new()
                .set_title(tr!("Load subtitles", "Загрузить субтитры"))
                .add_filter(tr!("Subtitles", "Субтитры"), tracks::SUBTITLE_EXTENSIONS)
                .add_filter(tr!("All files", "Все файлы"), &["*"]);
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::Subtitles(file), make, dialog::pick_file);
    }

    /// Ask for the folder Save the frame writes to.
    pub(super) fn pick_frames_folder(&mut self, ctx: &egui::Context) {
        let dir = self.settings.frames_dir();
        let make = move || {
            let dialog =
                rfd::FileDialog::new().set_title(tr!("The folder for frames", "Папка для кадров"));
            match dir {
                Some(dir) => dialog.set_directory(dir),
                None => dialog,
            }
        };
        self.ask(ctx, Ask::FramesFolder, make, dialog::pick_folder);
    }

    /// `Ctrl+V`: the files copied in Explorer, else the addresses and paths
    /// of the clipboard's text, opened as `Ctrl+O` and `Ctrl+U` open them;
    /// subtitle files go to the video playing, as dropped ones do.
    pub(super) fn paste(&mut self) {
        let items = match arboard::Clipboard::new() {
            Ok(mut clipboard) => match clipboard.get().file_list() {
                Ok(files) if !files.is_empty() => files,
                _ => clipboard
                    .get_text()
                    .map(|text| playlist::pasted(&text, |p| p.exists()))
                    .unwrap_or_default(),
            },
            Err(e) => {
                log::warn!("clipboard: {e}");
                Vec::new()
            }
        };
        log::debug!("pasted {} item(s)", items.len());
        let (subs, media): (Vec<PathBuf>, Vec<PathBuf>) =
            items.into_iter().partition(|p| tracks::is_subtitles(p));
        if media.is_empty() && (subs.is_empty() || self.state_idle()) {
            self.osd.show(Osd::Text(
                tr!(
                    "[ No file or address to paste ]",
                    "[ В буфере обмена нет файла или адреса ]"
                )
                .into(),
            ));
            return;
        }
        if media.is_empty() {
            for sub in &subs {
                self.add_subtitles(sub);
            }
            return;
        }
        self.open(media);
        if let Some(sub) = subs.into_iter().next()
            && let Some(player) = self.player.as_mut().filter(|p| p.loading())
        {
            player.subtitles_once_loaded(sub);
        }
    }

    pub(super) fn add_subtitles(&mut self, path: &Path) {
        if let Some(player) = &mut self.player {
            player.add_subtitles(path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.osd.show(Osd::Track {
                audio: false,
                now: Some(name),
            });
        }
    }

    /// Size the window so that its video area shows `picture` pixels,
    /// keeping its centre and staying on the monitor's work area (a
    /// picture too big for it is scaled down); a maximized window is
    /// restored first. In fullscreen, fullscreen ends and the window is
    /// sized in the next frame (`pending_picture`).
    /// Files dropped on a window: subtitle files dropped on a playing video
    /// go with it, the others are added to the playlist (opened when it is
    /// empty).
    pub(super) fn drop_files(&mut self, dropped: Vec<PathBuf>, state: &State) {
        let (subs, media): (Vec<PathBuf>, Vec<PathBuf>) =
            dropped.into_iter().partition(|p| tracks::is_subtitles(p));
        if !state.idle {
            for sub in &subs {
                self.add_subtitles(sub);
            }
        }
        // Into an empty playlist as opened (the folder filled in when the
        // setting says so); else added to it, played if nothing plays.
        if media.is_empty() {
            return;
        }
        if self.playlist.is_empty() {
            self.open(media);
        } else {
            self.add(media);
        }
        // Subtitles dropped with a video while nothing played: for the
        // video that starts, not entries of the playlist.
        if state.idle
            && let Some(sub) = subs.into_iter().next()
            && let Some(player) = self.player.as_mut().filter(|p| p.loading())
        {
            player.subtitles_once_loaded(sub);
        }
    }
}

/// The video's display size in pixels, `None` without a picture.
/// The OSD's line for a frames' folder that is not there and cannot be
/// made (a drive taken out).
pub(super) fn folder_missing(dir: &Path) -> String {
    tr!(
        format!("[ The folder {} is not available ]", dir.display()),
        format!("[ Папка {} недоступна ]", dir.display())
    )
}

/// The OSD's line while a video map is made.
pub(super) fn map_progress(done: u32, frames: u32) -> String {
    tr!(
        format!("[ Video map: {done}/{frames} ]"),
        format!("[ Карта видео: {done}/{frames} ]")
    )
}

pub(super) fn map_failed(why: &str) -> String {
    let why = crate::video_map::reason_in(crate::i18n::lang(), why);
    tr!(
        format!("Cannot make the video map: {why}"),
        format!("Не удалось создать карту видео: {why}")
    )
}

/// The files dropped on the window `ctx` is drawing.
pub(super) fn dropped_files(ctx: &egui::Context) -> Vec<PathBuf> {
    ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    })
}
