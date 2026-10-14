//! The window: the eframe `App` that puts the player, the picture and the
//! interface together.
//!
//! The layout: the title row, the picture, the control
//! bar under it, the playlist on the right when shown. In fullscreen only
//! the picture is left; the control bar comes over it while the mouse moves.
//!
//! Every action of keys, buttons and menus is an `input::Cmd`, gathered
//! during the frame and run at its end (`App::run`), outside the closures of
//! egui: mpv may call its wakeup callback from inside a libmpv call, and
//! that callback takes egui's lock (`request_repaint`).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Sense, TextureHandle};

use crate::adjust::Adjust;
use crate::cover::Cover;
use crate::format::Osd;
use crate::frameless;
use crate::input::{self, Cmd};
use crate::keymap;
use crate::mpv::Mpv;
use crate::player::{DataEnd, Happened, Player, Seek, State};
use crate::playlist::{self, Playlist, Repeat};
use crate::resume::{Positions, Tracks};
use crate::settings::{Interface, OnTop, Settings};
use crate::sizing;
use crate::tracks::{self, Kind};
use crate::ui::about::AboutDialog;
use crate::ui::info::{ChannelView, CoverView, InfoWindow};
use crate::ui::osd::OsdLine;
use crate::ui::picture::VideoPanel;
use crate::ui::settings::SettingsDialog;
use crate::ui::{self, style};
use crate::update::Updates;
use crate::video::Video;
use crate::win;

/// How long the mouse may rest in fullscreen, away from the panels, before
/// the pointer hides.
const HIDE_AFTER: Duration = Duration::from_millis(500);
/// The height of the strip with the file's name at the top in fullscreen.
const FULLSCREEN_TITLE_HEIGHT: f32 = 34.0;
/// How near the bottom edge the pointer shows the control bar in
/// fullscreen (the bar is a little lower).
const FULLSCREEN_BAR_ZONE: f32 = 90.0;
/// The shortest A-B repeat, in seconds.
const MIN_LOOP: f64 = 0.1;
/// Files handed over by other starts this close together make one
/// playlist (Explorer starts one process per selected file).
const SAME_BATCH: Duration = Duration::from_millis(1500);
/// How long the window stays cloaked at most while it is being maximized.
const UNCLOAK_TIMEOUT: Duration = Duration::from_secs(1);
/// The storage key of where the files were left (`resume.rs`).
const POSITIONS_KEY: &str = "resume_positions";

pub struct App {
    settings: Settings,
    player: Option<Player>,
    video: Option<Video>,
    /// Why there is no player or no picture (libmpv missing, …).
    engine_error: Option<crate::player::EngineError>,
    /// Why the last file could not be played.
    file_error: Option<String>,
    /// Frames asked for and not written yet, in order: their names are
    /// taken, and the OSD names each once written.
    frames_saving: std::collections::VecDeque<PathBuf>,
    playlist: Playlist,
    about: AboutDialog,
    settings_dialog: SettingsDialog,
    info: InfoWindow,
    /// The address to play (`Ctrl+U`).
    url_dialog: ui::url::UrlDialog,
    goto_dialog: ui::goto::GoToDialog,
    /// The Video, sound and subtitles panel (`U`, `F7`) and the picture's adjustments it edits.
    video_panel: VideoPanel,
    /// Where the control bar drew the panel's button this frame.
    panel_button: Option<egui::Rect>,
    /// The tracks window (`T`) and where the bar drew its button.
    tracks_window: ui::tracks::TracksWindow,
    tracks_button: Option<egui::Rect>,
    /// The audio filters mpv has (`App::sync_audio_filters`).
    af_applied: String,
    /// The channel layout the voice's filter was made for: kept while a
    /// file loads (no tracks yet), else it went and came back with each
    /// file.
    af_voice: Option<String>,
    /// The pictures of IPTV channels (`logos.rs`).
    logos: crate::logos::Logos,
    adjust: Adjust,
    /// The cover of the file playing, while or once it is looked for.
    cover: Option<CoverSlot>,
    /// What the container of the file playing says (`probe.rs`), read
    /// when the information window first shows it.
    facts: Option<FactsSlot>,
    updates: Updates,
    osd: OsdLine,
    hwnd: Option<isize>,
    /// The app icon of the title row and of the start screen, each at its
    /// size.
    icon: ui::IconCache,
    start_icon: ui::IconCache,
    /// The accent of the window's icon (blue from `main`).
    window_icon: style::Accent,
    /// The playlist's own window (`dock.rs`): how egui makes it, while it
    /// is open, and its handle once found.
    playlist_builder: Option<egui::ViewportBuilder>,
    playlist_hwnd: Option<isize>,
    /// The seek bar is being dragged: the position under the pointer.
    seek_drag: Option<f64>,
    /// When the last file came from another start, or this start's own
    /// files were opened: Explorer starts one copy per selected file, and
    /// the others' files join the first's.
    last_received: Option<Instant>,
    /// The window title shown, to change it only when it changes.
    shown_title: String,
    /// The window is cloaked until its first maximized frame.
    cloak: Option<(Instant, u64)>,
    /// Commands from `logic` (other starts, the player), run in `ui`.
    pending: Vec<Cmd>,
    /// The first video frame has been logged (`METALMPV_TRACE`).
    first_frame_logged: bool,
    /// The subtitles shown before `V` turned them off.
    last_sub: Option<i64>,
    /// In fullscreen as of the last frame (for `save`, which has no egui
    /// context).
    fullscreen: bool,
    /// Where the files were left (`resume_playback`).
    positions: Positions,
    /// The file being loaded gets the tracks most like the last chosen
    /// (`prefer_last_tracks`), once it is loaded (`pick_ready`).
    pick_tracks: bool,
    pick_ready: bool,
    /// Fullscreen with the picture filling the screen, cropped (`6`,
    /// mpv's `panscan`); ends with fullscreen.
    fill: bool,
    /// The interface `7` brings back from only the picture: the last
    /// other one shown.
    before_bare: Interface,
    /// The interface the window's size was made for: when another is
    /// shown, the window grows or shrinks by the difference, so that the
    /// picture stays put (`fit_interface`; not while maximized or in
    /// fullscreen, which keep their size, but once it is a window again).
    sized_for: Interface,
    /// The docked playlist's width as of the last frame with it, in points.
    playlist_inset: f32,
    /// The title row is shown over the picture's top this frame (an
    /// interface without its own, the pointer there).
    title_strip: bool,
    /// The panels over the picture shown in the last frame (the top, the
    /// bottom): a menu or a popup opened over them keeps them
    /// ([`Self::held`]).
    shown_over: (bool, bool),
    /// Windows is ending the session or going to sleep: the settings and
    /// positions are saved in `logic` (winit does not close the window for
    /// either); why, for the log.
    save_now: Option<&'static str>,
    /// The right button went down on the picture: the context menu opens
    /// when it is let go.
    menu_press: bool,
    /// The middle button went down on the picture: the playlist shows or
    /// hides when it is let go.
    middle_press: bool,
    /// The playlist over the picture's right side in fullscreen (`L`, the
    /// middle button); closed when fullscreen ends.
    fullscreen_playlist: bool,
    /// How wide it was drawn in the last frame (0 while closed), in points:
    /// the panels over the picture keep off it.
    fullscreen_list_width: f32,
    /// The video area of the last frame, in points.
    picture_area: Rect,
    /// The title row, the control bar and the playlist around the video
    /// area in a window (not fullscreen), in points.
    chrome: egui::Vec2,
    /// The picture's size in pixels the window is to be made for once
    /// fullscreen has ended (`1`, `2`, `3` in fullscreen).
    pending_picture: Option<[f64; 2]>,
    /// The output device given to mpv (empty: Windows' default, as mpv
    /// starts), kept in step with the setting and the devices there are.
    audio_device_used: String,
    /// What playback last asked Windows to keep on.
    awake: win::Awake,
    /// The taskbar button's progress and thumbnail buttons.
    taskbar: Option<crate::taskbar::Taskbar>,
    /// Windows' media controls: the media keys without the focus, the
    /// flyout with the title (`smtc.rs`).
    smtc: Option<crate::smtc::Smtc>,
    /// Windows has no media controls for the window: not asked again.
    smtc_failed: bool,
    /// The last media key taken and when: the same press may come both
    /// as the window's key and from the media controls.
    last_media: Option<(win::MediaKey, Instant)>,
    /// The window is over the other windows now (`settings.on_top`).
    on_top: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        boot: JoinHandle<Result<Mpv, crate::player::EngineError>>,
        files: Vec<PathBuf>,
    ) -> Self {
        log::debug!("window created at {:.0} ms", crate::since_start());
        let ctx = &cc.egui_ctx;
        let mut settings = cc.storage.map(Settings::load).unwrap_or_default();
        let positions = cc
            .storage
            .and_then(|s| s.get_string(POSITIONS_KEY))
            .map(|text| Positions::parse(&text))
            .unwrap_or_default();
        // Before any text is made.
        crate::i18n::set_lang(settings.language.resolve());
        crate::keymap::publish(&settings.keys);
        let osd = osd_font(&settings);
        if osd.is_none() {
            // Not chosen, or its file is gone: the interface's font.
            settings.osd_font.clear();
            settings.osd_font_file = None;
        }
        style::set_fonts(ctx, osd);
        style::set_accent(settings.accent);
        style::apply(ctx);
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            o.input_options.max_double_click_delay = win::double_click_time();
        });

        let hwnd = win::hwnd(cc);
        let mut cloak = None;
        if let Some(hwnd) = hwnd {
            win::round_corners(hwnd);
            let wake = ctx.clone();
            win::hook_window(hwnd, move || wake.request_repaint());
            win::set_snap(settings.snap_to_centre);
            crate::instance::listen(ctx, hwnd);
            if crate::MAXIMIZE_WHEN_SHOWN.swap(false, Ordering::Relaxed) {
                win::cloak(hwnd, true);
                // A rectangle on a monitor no longer there would restore
                // the window off the screen.
                let normal = settings.window_normal.filter(|r| win::on_a_monitor(*r));
                win::show_maximized(hwnd, normal, crate::DEFAULT_SIZE);
                cloak = Some((Instant::now() + UNCLOAK_TIMEOUT, ctx.cumulative_frame_nr()));
            }
        }

        // The core was started on its own thread meanwhile; the render
        // context must exist before a file is loaded.
        let mut engine_error = None;
        let (player, video) = match boot.join() {
            Ok(Ok(mpv)) => {
                let mut player = Player::new(mpv, ctx, &settings.audio_setup());
                if !settings.hwdec {
                    player.set_hwdec(false);
                }
                if settings.repeat == Repeat::One {
                    player.set_loop_file(true);
                }
                if settings.sub != crate::sub_style::SubStyle::default() {
                    player.set_sub_style(&settings.sub);
                }
                player.set_sub_layout(settings.sub_ass_override, settings.sub_margins);
                if settings.keep_adjust {
                    player.adjust(&Adjust::default(), &settings.adjust);
                }
                log::debug!("player made at {:.0} ms", crate::since_start());
                let video = match cc.gl.as_ref() {
                    Some(gl) => {
                        Video::new(gl, player.mpv(), ctx).map_err(|e| format!("video output: {e}"))
                    }
                    None => Err("no OpenGL context".into()),
                };
                let video = video
                    .map_err(|e| engine_error = Some(crate::player::EngineError::Failed(e)))
                    .ok();
                (Some(player), video)
            }
            Ok(Err(e)) => {
                engine_error = Some(e);
                (None, None)
            }
            Err(_) => {
                engine_error = Some(crate::player::EngineError::Failed(
                    "the mpv thread failed".into(),
                ));
                (None, None)
            }
        };
        if let Some(e) = &engine_error {
            log::error!("{e}");
        }
        log::debug!("player ready at {:.0} ms", crate::since_start());

        let mut updates = Updates::new(settings.check_updates, settings.last_update_check);
        updates.start_if_due(ctx);
        let af_applied = crate::audio::filters(settings.normalize, &settings.equalizer, None);
        let adjust = if settings.keep_adjust {
            settings.adjust.clone()
        } else {
            Adjust::default()
        };
        let mut app = Self {
            settings,
            player,
            video,
            engine_error,
            file_error: None,
            frames_saving: std::collections::VecDeque::new(),
            playlist: Playlist::default(),
            about: AboutDialog::default(),
            info: InfoWindow::default(),
            url_dialog: ui::url::UrlDialog::default(),
            goto_dialog: ui::goto::GoToDialog::default(),
            video_panel: VideoPanel::default(),
            panel_button: None,
            tracks_window: ui::tracks::TracksWindow::default(),
            tracks_button: None,
            af_applied,
            af_voice: None,
            logos: crate::logos::Logos::default(),
            adjust,
            cover: None,
            facts: None,
            settings_dialog: SettingsDialog::default(),
            updates,
            osd: OsdLine::default(),
            hwnd,
            icon: None,
            start_icon: None,
            window_icon: style::Accent::Blue,
            playlist_builder: None,
            playlist_hwnd: None,
            seek_drag: None,
            last_received: (!files.is_empty()).then(Instant::now),
            shown_title: String::new(),
            cloak,
            pending: Vec::new(),
            first_frame_logged: false,
            fullscreen: false,
            last_sub: None,
            positions,
            pick_tracks: false,
            pick_ready: false,
            fill: false,
            before_bare: Interface::Full,
            sized_for: Interface::Full,
            playlist_inset: 0.0,
            title_strip: false,
            shown_over: (false, false),
            save_now: None,
            menu_press: false,
            middle_press: false,
            fullscreen_playlist: false,
            fullscreen_list_width: 0.0,
            picture_area: Rect::NOTHING,
            chrome: egui::Vec2::ZERO,
            pending_picture: None,
            audio_device_used: String::new(),
            awake: win::Awake::No,
            taskbar: hwnd.and_then(crate::taskbar::Taskbar::new),
            smtc: None,
            smtc_failed: false,
            last_media: None,
            on_top: false,
        };
        app.sized_for = app.settings.interface;
        app.open(files);
        app
    }

    /// Nothing is loaded (or there is no player).
    fn state_idle(&self) -> bool {
        self.player.as_ref().is_none_or(|p| p.state.idle)
    }

    fn state(&self) -> State {
        self.player
            .as_ref()
            .map(|p| p.state.clone())
            .unwrap_or_default()
    }

    /// Replace the playlist with `paths` and play the first. One file
    /// brings the media files of its folder along when the setting says so.
    fn open(&mut self, paths: Vec<PathBuf>) {
        // A playlist file is its entries, not one of a folder's files.
        let from_list = paths.iter().any(|p| playlist::is_playlist_file(p));
        let (paths, channels) = playlist::expand_lists(paths);
        let Some(target) = paths.first().cloned() else {
            return;
        };
        let list =
            if paths.len() == 1 && !from_list && self.settings.fill_playlist && target.is_file() {
                playlist::folder_of(&target)
            } else {
                paths
            };
        self.playlist.replace(list);
        self.playlist.describe(channels);
        let index = self
            .playlist
            .items()
            .iter()
            .position(|p| *p == target)
            .unwrap_or(0);
        self.play(index);
    }

    /// Append `paths` to the playlist; play the first of them if nothing
    /// is playing.
    fn add(&mut self, paths: Vec<PathBuf>) {
        let (paths, channels) = playlist::expand_lists(paths);
        self.playlist.describe(channels);
        let first = self.playlist.add(paths);
        if let Some(first) = first
            && self.state().idle
        {
            self.play(first);
        }
    }

    /// Play playlist entry `index`.
    fn play(&mut self, index: usize) {
        // The entry playing chosen again starts over, not where it is.
        let again = self.playlist.current() == Some(index) && !self.state_idle();
        let Some(path) = self.playlist.set_current(index).map(Path::to_path_buf) else {
            return;
        };
        self.playlist.selected = Some(index);
        self.file_error = None;
        self.osd.clear_at_end();
        self.remember_position();
        let left = self.positions.get(&path);
        let start = left
            .and_then(|l| l.time)
            .filter(|_| self.settings.resume_playback && !again);
        let tracks = left
            .and_then(|l| l.tracks)
            .filter(|_| self.settings.remember_tracks);
        log::debug!(
            "loading {} at {:.0} ms",
            path.display(),
            crate::since_start()
        );
        // A file without tracks of its own to come back to gets those like
        // the last chosen, once it is loaded.
        self.pick_tracks = tracks.is_none()
            && self.settings.prefer_last_tracks
            && (self.settings.last_audio.is_some() || self.settings.last_sub.is_some());
        self.pick_ready = false;
        if let Some(player) = &mut self.player {
            player.load(&path, start, tracks);
        }
        if let Some(t) = start {
            let at = crate::format::time(Some(t), false);
            self.osd.show(Osd::Text(tr!(
                format!("Resumed from {at} (Home: from the beginning)"),
                format!("Продолжение с {at} (Home — с начала)")
            )));
        }
    }

    /// Keep where the file playing is now (`resume_playback`) and which
    /// tracks it plays (`remember_tracks`; only when there is a choice: more
    /// than one audio track, or subtitles).
    fn remember_position(&mut self) {
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
        // gives the duration after a seek past the data).
        let time = player.data_end_at().or(state.time);
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
                .and_then(|left| left.tracks)
                .map(|t| t.audio),
            None => Some(None),
        };
        let chosen = (tracks && choice)
            .then_some(audio)
            .flatten()
            .map(|audio| Tracks {
                audio,
                sub: tracks::selected(&state.tracks, Kind::Sub).map(|t| t.id),
            });
        let time = resume.then_some((time, duration));
        self.positions.remember(Path::new(path), time, chosen);
    }

    /// Read what the container of the file playing says while the
    /// information window is open (once per file), and take it when read.
    fn update_facts(&mut self, ctx: &egui::Context, state: &State) {
        let wanted = self
            .playlist
            .current_path()
            .filter(|p| self.info.open && !state.idle && !playlist::is_url(p));
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
    /// the voice's depends on the channels of the track playing, so it
    /// follows a change of track (`audio::filters`).
    fn sync_audio_filters(&mut self, ctx: &egui::Context, state: &State) {
        let Some(player) = &mut self.player else {
            return;
        };
        // While a slider is dragged (a band of the equalizer), the chain is
        // rebuilt once it is let go: each step rebuilt it, and seeked
        // under the normalization.
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let voice = if !self.settings.voice_boost {
            None
        } else if state.tracks.is_empty() {
            self.af_voice.clone()
        } else {
            tracks::selected(&state.tracks, Kind::Audio)
                .and_then(|t| crate::audio::voice_layout(t.layout.as_deref(), t.channels))
        };
        let af = crate::audio::filters(
            self.settings.normalize,
            &self.settings.equalizer,
            voice.as_deref(),
        );
        self.af_voice = voice;
        if af != self.af_applied {
            player.set_audio_filters(&af);
            self.af_applied = af;
        }
    }

    /// Look for the cover of the file playing once it is loaded, and take
    /// it when it is found.
    fn update_cover(&mut self, ctx: &egui::Context, state: &State) {
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

    /// Save the cover of the file playing where the user says.
    /// Save the frame on screen into Pictures\MetalMPV as
    /// `<file> <position>.png`, never over another.
    fn save_frame(&mut self, state: &State) {
        let (Some(player), Some(dir)) = (&mut self.player, frames_dir()) else {
            return;
        };
        if video_size(state).is_none() {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("cannot make {}: {e}", dir.display());
            return;
        }
        // A channel's name (a stream's file is all "index"), else the
        // file's; without the characters a file name cannot hold.
        let current = self.playlist.current_path();
        let stem = current
            .and_then(|p| self.playlist.channel(p)?.title.clone())
            .or_else(|| {
                current
                    .and_then(Path::file_stem)
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .map(|s| s.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_"))
            .unwrap_or_else(|| "frame".into());
        let time = crate::format::time(state.time, true).replace(':', "-");
        let mut path = dir.join(format!("{stem} {time}.png"));
        for n in 2.. {
            if !path.exists() && !self.frames_saving.contains(&path) {
                break;
            }
            path = dir.join(format!("{stem} {time} ({n}).png"));
        }
        player.save_frame(&path);
        self.frames_saving.push_back(path);
    }

    /// mpv wrote the frame asked for first, or could not (`error`).
    fn frame_saved(&mut self, error: Option<String>) {
        let path = self.frames_saving.pop_front();
        let name = path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.osd.show(Osd::Text(match error {
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

    /// Write the playlist to an M3U8 file the user names.
    fn save_playlist(&mut self, frame: &eframe::Frame) {
        if self.playlist.is_empty() {
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(tr!("Save the playlist", "Сохранить плейлист"))
            .set_file_name(tr!("Playlist.m3u8", "Плейлист.m3u8"))
            .add_filter(tr!("M3U8 playlist", "Плейлист M3U8"), &["m3u8"]);
        if let Some(dir) = self
            .playlist
            .current_path()
            .filter(|p| !playlist::is_url(p))
            .and_then(Path::parent)
        {
            dialog = dialog.set_directory(dir);
        }
        if let Some(target) = dialog.save_file()
            && let Err(e) = std::fs::write(&target, playlist::m3u_text(&self.playlist))
        {
            log::warn!("cannot save the playlist to {}: {e}", target.display());
            self.osd.show(Osd::Text(tr!(
                format!("Cannot save the playlist: {e}"),
                format!("Не удалось сохранить плейлист: {e}")
            )));
        }
    }

    fn save_cover(&mut self, frame: &eframe::Frame) {
        let Some((cover, _)) = self.cover.as_ref().and_then(|c| c.loaded.as_ref()) else {
            return;
        };
        let path = self.playlist.current_path();
        let stem = path
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cover".into());
        let mut dialog = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(tr!("Save the cover", "Сохранить обложку"))
            .set_file_name(format!("{stem}.{}", cover.extension))
            .add_filter(cover.extension.to_uppercase(), &[cover.extension]);
        if let Some(dir) = path.and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(target) = dialog.save_file()
            && let Err(e) = std::fs::write(&target, &cover.bytes)
        {
            log::warn!("cannot save the cover to {}: {e}", target.display());
        }
    }

    /// An audio file with a cover: the cover as big as fits, the title, the
    /// artist and the album under it. Whether there was one.
    fn audio_cover(&self, ui: &egui::Ui, rect: Rect, state: &State) -> bool {
        let has_video = state
            .tracks
            .iter()
            .any(|t| t.kind == tracks::Kind::Video && !t.albumart);
        let Some((_, texture)) = self
            .cover
            .as_ref()
            .filter(|_| !has_video)
            .and_then(|c| c.loaded.as_ref())
        else {
            return false;
        };
        let painter = ui.painter_at(rect);
        let title = crate::info::tag(&state.metadata, &["title"])
            .map(str::to_owned)
            .or_else(|| state.title.clone())
            .unwrap_or_default();
        let mut lines = vec![(title, 18.0, style::TEXT_BRIGHT)];
        if let Some(artist) = crate::info::artist(&state.metadata) {
            lines.push((artist.to_owned(), 15.0, style::TEXT));
        }
        if let Some(album) = crate::info::album(&state.metadata) {
            lines.push((album, 14.0, style::TEXT_WEAK));
        }
        let galleys: Vec<_> = lines
            .into_iter()
            .map(|(text, size, color)| {
                ui.fonts_mut(|f| {
                    let mut job = egui::text::LayoutJob::simple_singleline(
                        text,
                        FontId::proportional(size),
                        color,
                    );
                    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 48.0);
                    f.layout_job(job)
                })
            })
            .collect();
        const GAP: f32 = 18.0;
        let text_height: f32 = galleys.iter().map(|g| g.size().y + 4.0).sum();
        let room = egui::vec2(
            rect.width() - 64.0,
            (rect.height() - 64.0 - GAP - text_height).min(rect.height() * 0.7),
        );
        if room.x < 32.0 || room.y < 32.0 {
            return false;
        }
        let picture = texture.size_vec2();
        let scale = (room.x / picture.x).min(room.y / picture.y);
        let size = picture * scale;
        let top = rect.center().y - (size.y + GAP + text_height) / 2.0;
        let image = Rect::from_min_size(egui::pos2(rect.center().x - size.x / 2.0, top), size);
        painter.image(
            texture.id(),
            image,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let mut y = image.bottom() + GAP;
        for galley in galleys {
            let h = galley.size().y;
            let x = rect.center().x - galley.size().x / 2.0;
            painter.galley(egui::pos2(x, y), galley, style::TEXT);
            y += h + 4.0;
        }
        true
    }

    /// Ask for media files to play (`add`: append them to the playlist).
    fn pick_files(&mut self, frame: &eframe::Frame, add: bool) {
        let mut dialog = rfd::FileDialog::new()
            .set_parent(frame)
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
        if let Some(dir) = self.playlist.current_path().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        let Some(files) = dialog.pick_files() else {
            return;
        };
        if add {
            self.add(files);
        } else {
            self.open(files);
        }
    }

    /// Play audio track or show subtitle track `id` (`None`: no subtitles)
    /// and say so on the OSD.
    fn select_track(&mut self, kind: Kind, id: Option<i64>) {
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
    fn cycle_track(&mut self, kind: Kind) {
        let next = self
            .player
            .as_ref()
            .and_then(|p| tracks::next(&p.state.tracks, kind));
        if let Some(id) = next {
            self.select_track(kind, id);
        }
    }

    /// Ask for a subtitle file for the playing video.
    fn pick_subtitles(&mut self, frame: &eframe::Frame) {
        if self.state().idle {
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(tr!("Load subtitles", "Загрузить субтитры"))
            .add_filter(tr!("Subtitles", "Субтитры"), tracks::SUBTITLE_EXTENSIONS)
            .add_filter(tr!("All files", "Все файлы"), &["*"]);
        if let Some(dir) = self.playlist.current_path().and_then(Path::parent) {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.add_subtitles(&path);
        }
    }

    fn add_subtitles(&mut self, path: &Path) {
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

    /// Seek by `delta` seconds and say so on the OSD.
    fn seek_by(&mut self, delta: f64, how: Seek) {
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

    fn seek_to(&mut self, seconds: f64, how: Seek) {
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

    fn set_fullscreen(&mut self, ctx: &egui::Context, frame: &eframe::Frame, on: bool) {
        if on != frameless::is_fullscreen(ctx) {
            frameless::set_fullscreen(ctx, frame, on);
        }
    }

    /// Run a command of a key, a button or a menu.
    fn run(&mut self, ctx: &egui::Context, frame: &eframe::Frame, cmd: Cmd) {
        let state = self.state();
        match cmd {
            Cmd::TogglePause => {
                if state.idle {
                    // After Stop or a failed file: play the current entry again.
                    if let Some(i) = self
                        .playlist
                        .current()
                        .or((!self.playlist.is_empty()).then_some(0))
                    {
                        self.play(i);
                    } else {
                        self.pick_files(frame, false);
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
                // Video panel, then fullscreen, then the program (as its
                // close button: the settings are saved).
                if self.info.close() {
                } else if self.tracks_window.open {
                    self.tracks_window.open = false;
                } else if self.video_panel.open {
                    self.video_panel.open = false;
                } else if frameless::is_fullscreen(ctx) && self.fullscreen_playlist {
                    self.fullscreen_playlist = false;
                } else if frameless::is_fullscreen(ctx) {
                    self.set_fullscreen(ctx, frame, false);
                } else if self.interface() == Interface::Bare {
                    self.run(ctx, frame, Cmd::ToggleInterface);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Cmd::ToggleInfo => self.info.toggle(),
            Cmd::ToggleVideoPanel => {
                self.video_panel.toggle();
                if self.video_panel.open {
                    self.tracks_window.open = false;
                }
            }
            Cmd::ShowEqualizer => self.video_panel.open_on(ui::picture::Tab::Sound),
            Cmd::SaveFrame => self.save_frame(&state),
            Cmd::OpenRecent(i) => {
                if let Some(path) = self.settings.recent.get(i).cloned() {
                    if path.is_file() {
                        self.open(vec![path]);
                    } else {
                        self.settings.recent.remove(i);
                        self.osd.show(Osd::Text(tr!(
                            format!("Not found: {}", path.display()),
                            format!("Не найден: {}", path.display())
                        )));
                    }
                }
            }
            Cmd::ClearRecent => self.settings.recent.clear(),
            Cmd::ShowFrames => {
                if let Some(dir) = frames_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    win::shell_open(&dir);
                }
            }
            Cmd::SaveCover => self.save_cover(frame),
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
            Cmd::Open => self.pick_files(frame, false),
            Cmd::OpenUrl => self.url_dialog.open(),
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
                let shown = if frameless::is_fullscreen(ctx) {
                    self.fullscreen_playlist
                } else {
                    self.settings.playlist_visible && self.interface() != Interface::Bare
                };
                if shown && let Some(i) = self.playlist.selected {
                    self.playlist.remove(i);
                }
            }
            Cmd::PlayIndex(i) => self.play(i),
            Cmd::PlaylistAdd => self.pick_files(frame, true),
            Cmd::PlaylistRemove(i) => self.playlist.remove(i),
            Cmd::PlaylistSort => self.playlist.sort(),
            Cmd::PlaylistClear => self.playlist.clear(),
            Cmd::PlaylistSave => self.save_playlist(frame),
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
            Cmd::LoadSubtitles => self.pick_subtitles(frame),
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

    /// The picture filling the screen, cropped, or whole (`fill`).
    /// The picture's size as shown: the video's display size, turned by
    /// the Video panel's rotation (done when the picture is drawn, so not
    /// in mpv's `dwidth`/`dheight`).
    fn picture_size(&self, state: &State) -> Option<[f64; 2]> {
        let [w, h] = video_size(state)?;
        Some(if self.adjust.rotate % 180 == 90 {
            [h, w]
        } else {
            [w, h]
        })
    }

    /// The media keys of a keyboard, a headset or a remote (the window's
    /// messages, `win::hook_window`), as the player's commands.
    fn take_hooked(&mut self) {
        let paused = self
            .player
            .as_ref()
            .is_none_or(|p| p.state.paused || p.state.idle);
        // Play with nothing to play would open the file dialog, perhaps
        // over a minimised window: a headset's button does nothing then.
        let nothing = self.state_idle() && self.playlist.is_empty();
        let play = |cmd: Cmd| (!(nothing && cmd == Cmd::TogglePause)).then_some(cmd);
        for hooked in win::take_hooked() {
            let key = match hooked {
                win::Hooked::Media(key) => key,
                win::Hooked::TaskbarButton => {
                    if let Some(taskbar) = &mut self.taskbar {
                        taskbar.button_created();
                    }
                    continue;
                }
                win::Hooked::EndSession => {
                    self.save_now = Some("the session is ending");
                    continue;
                }
                win::Hooked::Suspend => {
                    self.save_now = Some("going to sleep");
                    continue;
                }
                win::Hooked::Close => {
                    log::info!("the Restart Manager asks to close");
                    self.pending.push(Cmd::Close);
                    continue;
                }
                win::Hooked::Thumb(id) => {
                    self.pending.extend(match id {
                        crate::taskbar::PREVIOUS => Some(Cmd::Prev),
                        crate::taskbar::PLAY_PAUSE => play(Cmd::TogglePause),
                        crate::taskbar::NEXT => Some(Cmd::Next),
                        _ => None,
                    });
                    continue;
                }
            };
            if same_press(self.last_media, key, Instant::now()) {
                continue;
            }
            self.last_media = Some((key, Instant::now()));
            let cmd = match key {
                win::MediaKey::PlayPause => Some(Cmd::TogglePause),
                win::MediaKey::Play => paused.then_some(Cmd::TogglePause),
                win::MediaKey::Pause => (!paused).then_some(Cmd::TogglePause),
                win::MediaKey::Stop => Some(Cmd::Stop),
                win::MediaKey::Next => Some(Cmd::Next),
                win::MediaKey::Previous => Some(Cmd::Prev),
            };
            self.pending.extend(cmd.and_then(play));
        }
    }

    /// Keep the display on while a picture plays in sight and the computer
    /// awake while sound plays; let them sleep when paused, stopped or
    /// idle.
    /// The position as the taskbar button's progress (yellow when paused)
    /// and the thumbnail buttons' state.
    fn show_on_taskbar(&mut self) {
        let Some(taskbar) = &mut self.taskbar else {
            return;
        };
        let state = self.player.as_ref().map(|p| &p.state);
        let progress = state
            .filter(|s| !s.idle)
            .and_then(|s| s.fraction())
            .map(|p| (p * 1000.0).round() as u64);
        let repeat = self.settings.repeat;
        taskbar.show(crate::taskbar::Shown {
            progress,
            paused: state.is_none_or(|s| s.paused || s.idle),
            has_previous: self.playlist.prev_in(repeat).is_some(),
            has_next: self.playlist.next_in(repeat).is_some(),
        });
    }

    /// Take the media keys as the setting says: Windows' media controls
    /// made or let go, the windows' own keys on or off.
    fn follow_media_keys(&mut self) {
        use crate::settings::MediaKeys;
        let wanted = self.settings.media_keys;
        win::set_media_keys(wanted != MediaKeys::Off);
        if wanted == MediaKeys::Always {
            if self.smtc.is_none() && !self.smtc_failed {
                self.smtc = self.hwnd.and_then(crate::smtc::Smtc::new);
                self.smtc_failed = self.smtc.is_none();
            }
        } else if self.smtc.take().is_some() {
            log::debug!("media controls let go");
        }
    }

    /// What plays in Windows' media controls: the state, previous and
    /// next, the title (and the artist of a song).
    fn show_in_media_controls(&mut self) {
        if self.smtc.is_none() {
            return;
        }
        use crate::smtc::Status;
        let state = self.state();
        let status = match &self.player {
            _ if self.engine_error.is_some() => Status::Closed,
            None => Status::Closed,
            Some(_) if state.idle && self.playlist.is_empty() => Status::Closed,
            Some(_) if state.idle => Status::Stopped,
            Some(_) if state.paused || state.eof => Status::Paused,
            Some(_) => Status::Playing,
        };
        let music = state.video().is_none();
        let shown = crate::smtc::Shown {
            status,
            has_previous: self.playlist.prev_in(self.settings.repeat).is_some(),
            has_next: self.playlist.next_in(self.settings.repeat).is_some(),
            title: if status == Status::Closed {
                String::new()
            } else {
                self.title(&state)
            },
            artist: music
                .then(|| crate::info::artist(&state.metadata).map(str::to_owned))
                .flatten(),
            music,
        };
        if let Some(smtc) = &mut self.smtc {
            smtc.show(shown);
        }
    }

    fn follow_on_top(&mut self, ctx: &egui::Context) {
        let playing = self
            .player
            .as_ref()
            .is_some_and(|p| !p.state.idle && !p.state.paused && !p.state.eof);
        let on_top = match self.settings.on_top {
            OnTop::Never => false,
            OnTop::Always => true,
            OnTop::Playing => playing,
        };
        if on_top != self.on_top {
            self.on_top = on_top;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }
    }

    fn keep_awake(&mut self, minimized: bool) {
        let awake = match &self.player {
            Some(p) if !p.state.idle && !p.state.paused && !p.state.eof => {
                if p.state.video().is_some() && !minimized {
                    win::Awake::Display
                } else {
                    win::Awake::System
                }
            }
            _ => win::Awake::No,
        };
        if awake != self.awake {
            log::debug!("keep awake: {awake:?}");
            win::keep_awake(awake);
            self.awake = awake;
        }
    }

    /// Give mpv the chosen output device while Windows has it, else the
    /// default one; also when a device comes or goes, or the setting
    /// changes.
    fn follow_audio_device(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        let Some(device) =
            crate::audio::device_to_use(&self.settings.audio_device, &player.state.audio_devices)
        else {
            return;
        };
        if device != self.audio_device_used {
            log::debug!(
                "audio device: {:?}",
                if device.is_empty() { "auto" } else { device }
            );
            player.set_audio_device(device);
            self.audio_device_used = device.to_owned();
        }
    }

    fn set_fill(&mut self, on: bool) {
        self.fill = on;
        if let Some(player) = &mut self.player {
            player.set_fill(on);
        }
    }

    /// Size the window so that its video area shows `picture` pixels,
    /// keeping its centre and staying on the monitor's work area (a
    /// picture too big for it is scaled down); a maximized window is
    /// restored first. In fullscreen, fullscreen ends and the window is
    /// sized in the next frame (`pending_picture`).
    /// Files dropped on a window: subtitle files dropped on a playing video
    /// go with it, the others are opened.
    fn drop_files(&mut self, dropped: Vec<PathBuf>, state: &State) {
        let (subs, media): (Vec<PathBuf>, Vec<PathBuf>) = dropped
            .into_iter()
            .partition(|p| tracks::is_subtitles(p) && !state.idle);
        for sub in &subs {
            self.add_subtitles(sub);
        }
        if !media.is_empty() {
            self.open(media);
        }
    }

    /// The playlist in a window of its own (`dock.rs`) while it is shown
    /// and detached: an egui viewport drawn in this frame, as metalmd's
    /// detached tabs are; not in fullscreen when it is over the player's
    /// screen. `keys`: the player's keys work (no modal is open).
    fn playlist_window(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        cmds: &mut Vec<Cmd>,
        keys: bool,
    ) {
        let player = self.hwnd.and_then(win::window_rect);
        let over_player = match (player, self.settings.playlist_rect) {
            (Some(m), Some(p)) => p[0] < m[2] && m[0] < p[2] && p[1] < m[3] && m[1] < p[3],
            _ => false,
        };
        let shown = self.settings.playlist_visible
            && self.settings.playlist_detached
            && self.interface() != Interface::Bare
            && !(self.fullscreen && over_player);
        if !shown {
            if self.playlist_builder.take().is_some() {
                self.playlist_hwnd = None;
                win::playlist_closed();
            }
            return;
        }
        let ppp = ctx.pixels_per_point();
        let saved = self
            .settings
            .playlist_rect
            .filter(|r| win::on_a_monitor(*r));
        let height = ctx.content_rect().height();
        let accent = self.settings.accent;
        let builder = self
            .playlist_builder
            .get_or_insert_with(|| {
                // Frameless as the player's: the title row is the app's own.
                let builder = egui::ViewportBuilder::default()
                    .with_title(tr!("Playlist", "Плейлист"))
                    .with_decorations(false)
                    .with_resizable(true)
                    .with_min_inner_size([ui::playlist::MIN_WIDTH, 160.0])
                    .with_minimize_button(false)
                    .with_maximize_button(false)
                    .with_drag_and_drop(true)
                    .with_taskbar(false)
                    .with_icon(crate::window_icon(accent));
                // Near enough: the window is put exactly once it is found.
                match saved {
                    Some(r) => builder
                        .with_position(egui::pos2(r[0] as f32 / ppp, r[1] as f32 / ppp))
                        .with_inner_size([(r[2] - r[0]) as f32 / ppp, (r[3] - r[1]) as f32 / ppp]),
                    None => builder.with_inner_size([PLAYLIST_WIDTH, height]),
                }
            })
            .clone();
        let id = egui::ViewportId::from_hash_of("playlist_window");
        let mut close = false;
        let mut dropped = Vec::new();
        ctx.show_viewport_immediate(id, builder, |ui, _class| {
            let ctx = ui.ctx().clone();
            close = ctx.input(|i| i.viewport().close_requested());
            dropped = dropped_files(&ctx);
            if keys && !egui::Popup::is_any_open(&ctx) {
                if let Some(id) = ctx.memory(|m| m.focused()) {
                    ctx.memory_mut(|m| m.surrender_focus(id));
                }
                // Esc here closes this window, not the program.
                cmds.extend(input::keys(&ctx, &self.settings.keys).into_iter().map(
                    |cmd| match cmd {
                        Cmd::Escape => Cmd::TogglePlaylist,
                        cmd => cmd,
                    },
                ));
            }
            frameless::resize_edges(ui);
            egui::Panel::top("playlist_title")
                .exact_size(style::TITLE_HEIGHT)
                .frame(egui::Frame::NONE.fill(style::TITLE_BG))
                .show_separator_line(false)
                .show(ui, |ui| {
                    close |= ui::title::tool_row(ui, tr!("Playlist", "Плейлист"));
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(style::PANEL_BG).inner_margin(6))
                .show(ui, |ui| {
                    let logos = self.settings.channel_logos.then_some(&mut self.logos);
                    cmds.extend(ui::playlist::panel(
                        ui,
                        &mut self.playlist,
                        logos,
                        self.settings.repeat,
                        &state.chapters,
                        state.time,
                        true,
                    ));
                });
        });
        self.drop_files(dropped, state);
        if close {
            self.settings.playlist_visible = false;
        }
        // eframe gives no handle for the window: the visible one of this
        // thread that is not the player's. Found again if it was made anew
        // (its handle may then be the old one's: not owned by the player
        // yet).
        if let (Some(h), Some(main)) = (self.playlist_hwnd, self.hwnd)
            && !win::owned_by(h, main)
        {
            self.playlist_hwnd = None;
        }
        if self.playlist_hwnd.is_none()
            && let Some(main) = self.hwnd
            && let Some(child) = win::thread_windows().into_iter().find(|h| *h != main)
        {
            self.playlist_hwnd = Some(child);
            let first = || {
                let main = win::visible_rect(main)?;
                Some(crate::dock::first_place(
                    main,
                    (PLAYLIST_WIDTH * ppp).round() as i32,
                ))
            };
            if let Some(rect) = saved.or_else(first) {
                win::set_visible_rect(child, rect);
            }
            win::adopt_playlist(child, main);
        }
        // A minimised window is far off the screen.
        if let Some(rect) = self.playlist_hwnd.and_then(win::visible_rect)
            && rect[0] > -30000
        {
            self.settings.playlist_rect = Some(rect);
        }
    }

    /// The A-B repeat: its start or its end at the position now, or off;
    /// `CycleLoop` does the next of the three. An end without a start
    /// starts at the beginning; one before the start swaps them.
    fn ab_loop(&mut self, cmd: Cmd, state: &State) {
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

    /// The interface shown.
    fn interface(&self) -> Interface {
        self.settings.interface
    }

    /// What `interface` takes around the picture in a window, in points
    /// (left, top, right, bottom).
    fn insets(&self, interface: Interface) -> [f32; 4] {
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
    fn fit_interface(&mut self, ctx: &egui::Context) {
        let now = self.interface();
        if self.sized_for == now
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
        let (old, new) = (self.insets(self.sized_for), self.insets(now));
        self.sized_for = now;
        let ppp = ctx.pixels_per_point();
        let grow = |i: usize| (new[i] * ppp).round() as i32 - (old[i] * ppp).round() as i32;
        let rect = [
            window[0] - grow(0),
            window[1] - grow(1),
            window[2] + grow(2),
            window[3] + grow(3),
        ];
        let rect = match win::work_area(hwnd) {
            Some(work) => sizing::placed(rect, [rect[2] - rect[0], rect[3] - rect[1]], work),
            None => rect,
        };
        win::set_rect(hwnd, rect);
    }

    /// The title row comes over the picture's top (an interface without
    /// one of its own) while the pointer is there, over nothing else.
    fn wants_title_strip(&self, ctx: &egui::Context) -> bool {
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
    fn wants_bar_over(&self, ctx: &egui::Context) -> bool {
        let area = self.picture_area;
        let height = ui::controls::height(self.settings.interface == Interface::Compact);
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
    fn controls_bar<'a>(
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
        }
    }

    fn size_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame, picture: [f64; 2]) {
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

    /// Take what the player and other starts have to say; also called
    /// while the window is minimised, so a playlist goes on.
    fn take_events(&mut self) {
        let received = crate::instance::take();
        if let Some(&(_, last)) = received.last() {
            let batch = same_batch(self.last_received, received[0].1);
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
                    self.pick_ready = self.pick_tracks;
                }
                Happened::Expanded { entries, playing } => {
                    let entries = entries
                        .into_iter()
                        .map(|(file, title)| (PathBuf::from(file), title))
                        .collect();
                    self.playlist.expand_current(entries, playing);
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
                    self.osd.show(Osd::Text(tr!(
                        format!("Cannot play {name}: {e}"),
                        format!("Не удалось воспроизвести {name}: {e}")
                    )));
                    self.file_error = Some(e);
                    // A broken file in a list is skipped.
                    if let Some(i) = self.playlist.next() {
                        self.play(i);
                    }
                }
                Happened::Restarted => {}
                Happened::FrameSaved(error) => self.frame_saved(error),
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
    fn pick_like_tracks(&mut self) {
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
        let sub = match &self.settings.last_sub {
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

    /// Show the window once it is maximized (see `win::show_maximized`).
    fn uncloak(&mut self, ctx: &egui::Context) {
        let Some((until, frame)) = self.cloak else {
            return;
        };
        let ready = ctx.cumulative_frame_nr() > frame && frameless::is_maximized(ctx);
        if ready || Instant::now() >= until {
            if let Some(hwnd) = self.hwnd {
                win::cloak(hwnd, false);
            }
            self.cloak = None;
        } else {
            ctx.request_repaint();
        }
    }

    /// The name of what is playing, for the title row and the window.
    fn title(&self, state: &State) -> String {
        // Nothing plays without the engine: no file's name.
        if self.engine_error.is_some() || (state.idle && self.playlist.current_path().is_none()) {
            return crate::APP_NAME.to_owned();
        }
        self.playlist
            .current_path()
            .map(|p| self.playlist.title(p))
            .or_else(|| state.title.clone())
            .unwrap_or_else(|| crate::APP_NAME.to_owned())
    }

    /// The picture, or what stands in for it; the mouse on it.
    fn video_area(
        &mut self,
        ui: &mut egui::Ui,
        frame: &mut eframe::Frame,
        state: &State,
        cmds: &mut Vec<Cmd>,
    ) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
        // A double click switches fullscreen, a drag moves the window.
        let response = frameless::drag_area(ui, rect, ui.id().with("video"), Sense::CLICK);
        let visible = state.video().is_some() && self.video.is_some();
        if !state.idle
            && let Some(video) = &mut self.video
        {
            video.show(ui, frame, rect, visible);
        }
        if visible {
            if !self.first_frame_logged {
                self.first_frame_logged = true;
                log::debug!("video size known at {:.0} ms", crate::since_start());
            }
        } else {
            if let Some(cmd) = self.placeholder(ui, rect, state) {
                cmds.push(cmd);
            }
        }
        if ui::double_clicked(&response) {
            cmds.push(Cmd::ToggleFullscreen);
        }
        // The middle button shows or hides the playlist when let go, however
        // the pointer moves meanwhile, as the right one opens the menu
        // (egui's click allows 6 points: a click on the move was lost).
        let (middle_pressed, middle_down) = ui.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Middle),
                i.pointer.button_down(egui::PointerButton::Middle),
            )
        });
        if middle_pressed {
            self.middle_press = response.contains_pointer();
        }
        if self.middle_press && !middle_down {
            self.middle_press = false;
            cmds.push(Cmd::TogglePlaylist);
        }
        if response.hovered() {
            let notches = ui::wheel_notches(ui);
            if notches != 0 {
                cmds.push(Cmd::Volume(notches as f64 * input::VOLUME_STEP));
            }
        }
        // In only the picture the playlist is not shown: the menu offers it.
        let playlist_visible = if self.fullscreen {
            self.fullscreen_playlist
        } else {
            self.settings.playlist_visible && self.interface() != Interface::Bare
        };
        let chapter_marks = self.settings.chapter_marks;
        let marks = Marks {
            fill: self.fill,
            interface: self.interface(),
            aspect: self.adjust.aspect,
            repeat: self.settings.repeat,
            on_top: self.settings.on_top,
            playlist_detached: self.settings.playlist_detached,
        };
        // As Windows' menus do: the right button opens the menu when let go
        // and a press anywhere off the menus closes it, however the pointer
        // moves meanwhile (egui's click allows 6 points: a click on the
        // move was lost). The press closes it before it is shown, so that
        // the right button let go there opens it anew, once the old one is
        // gone (egui closes a menu shown in the last frame on any click).
        let menu = egui::Popup::default_response_id(&response);
        let ctx = ui.ctx().clone();
        let (pressed, right_pressed, right_down, pos) = ui.input(|i| {
            (
                i.pointer.any_pressed(),
                i.pointer.button_pressed(egui::PointerButton::Secondary),
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.interact_pos(),
            )
        });
        let on_menus = pos
            .and_then(|p| ctx.layer_id_at(p))
            .is_some_and(|layer| layer.order == egui::Order::Foreground);
        if pressed && !on_menus && egui::Popup::is_id_open(&ctx, menu) {
            egui::Popup::close_id(&ctx, menu);
        }
        if right_pressed {
            self.menu_press = response.contains_pointer();
        }
        let let_go = self.menu_press && !right_down;
        let old_shown = ctx.read_response(menu).is_some();
        if let_go && old_shown {
            ctx.request_repaint();
        }
        let open = let_go && !old_shown;
        if open {
            self.menu_press = false;
        }
        egui::Popup::menu(&response)
            .open_memory(open.then_some(egui::SetOpenCommand::Bool(true)))
            .at_pointer_fixed()
            .show(|ui| context_menu(ui, state, playlist_visible, chapter_marks, marks, cmds));
        if ui.input(|i| !i.raw.hovered_files.is_empty()) {
            ui.painter()
                .rect_filled(rect.shrink(12.0), 8.0, Color32::from_black_alpha(160));
            ui.painter().rect_stroke(
                rect.shrink(12.0),
                8.0,
                egui::Stroke::new(2.0, style::accent()),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                tr!("Drop to play", "Отпустите, чтобы воспроизвести"),
                FontId::proportional(20.0),
                style::TEXT_BRIGHT,
            );
        }
        // In fullscreen the line goes under the strip with the name while
        // that is shown.
        let osd_rect = if self.fullscreen && self.fullscreen_panels(ui.ctx()).top {
            rect.with_min_y(rect.top() + FULLSCREEN_TITLE_HEIGHT)
        } else if self.title_strip {
            rect.with_min_y(rect.top() + style::TITLE_HEIGHT)
        } else {
            rect
        };
        let scale = (self.settings.osd_scale / 100.0) as f32;
        self.osd.paint(ui.ctx(), osd_rect, state, scale);
    }

    /// The black area without a picture: the start screen, the title of an
    /// audio file, or why nothing plays.
    fn placeholder(&mut self, ui: &egui::Ui, rect: Rect, state: &State) -> Option<Cmd> {
        let failed = self.engine_error.is_some() || (state.idle && self.file_error.is_some());
        if state.idle && !failed {
            return self.start_screen(ui, rect);
        }
        if !failed && self.audio_cover(ui, rect, state) {
            return None;
        }
        let size = start_icon_size(rect);
        let icon = ui::app_icon(ui.ctx(), size, &mut self.start_icon);
        let painter = ui.painter_at(rect);
        let tint = if state.idle {
            Color32::from_white_alpha(70)
        } else {
            Color32::WHITE
        };
        let centre = rect.center() - egui::vec2(0.0, size * 0.3);
        let icon_rect = ui::paint_icon(&painter, &icon, centre, tint);
        let below = icon_rect.center_bottom() + egui::vec2(0.0, 14.0);
        let (text, color) = if let Some(e) = &self.engine_error {
            (e.message(), style::ERROR)
        } else if let Some(e) = self.file_error.as_ref().filter(|_| state.idle) {
            (
                tr!(
                    format!("Cannot play the file: {e}"),
                    format!("Не удалось воспроизвести файл: {e}")
                ),
                style::ERROR,
            )
        } else {
            // An audio file: its title.
            (state.title.clone().unwrap_or_default(), style::TEXT)
        };
        // Wrapped to the area, each line centred: an error may be long.
        let mut job = egui::text::LayoutJob::simple(
            text,
            FontId::proportional(14.0),
            color,
            (rect.width() - 48.0).clamp(120.0, 640.0),
        );
        job.halign = egui::Align::Center;
        let galley = painter.layout_job(job);
        let at = below - egui::vec2(galley.rect.center().x, galley.rect.top());
        painter.galley(at, galley, color);
        None
    }

    /// Nothing plays: the icon, the name, the version and what to do, as
    /// in qview; centred, a little above the middle.
    fn start_screen(&mut self, ui: &egui::Ui, rect: Rect) -> Option<Cmd> {
        let size = start_icon_size(rect);
        let big = size > 64.0;
        let icon = ui::app_icon(ui.ctx(), size, &mut self.start_icon);
        let painter = ui.painter_at(rect);
        let layout = |text: String, points: f32, color: Color32| {
            painter.layout_no_wrap(text, FontId::proportional(points), color)
        };
        let name = layout(
            crate::APP_NAME.to_owned(),
            if big { 30.0 } else { 22.0 },
            style::TEXT_BRIGHT,
        );
        let version = layout(
            tr!(
                format!("Version {}", crate::VERSION),
                format!("Версия {}", crate::VERSION)
            ),
            13.0,
            style::TEXT_WEAK,
        );
        let open = keymap::shortcut(Cmd::Open);
        let hint = if open.is_empty() {
            tr!("Drop a file here", "Перетащите файл сюда").to_owned()
        } else {
            tr!(
                format!("Drop a file here, or open one with {open}"),
                format!("Перетащите файл сюда или откройте его через {open}")
            )
        };
        let hint = layout(hint, 14.0, style::TEXT);
        const GAPS: [f32; 3] = [14.0, 2.0, 18.0];
        let base =
            size + GAPS.iter().sum::<f32>() + name.size().y + version.size().y + hint.size().y;
        // The recent files under the hint, as many as fit.
        const RECENT_ROW: f32 = 36.0;
        const RECENT_EXTRA: f32 = 32.0 + 20.0 + 8.0 + 22.0;
        let fit = ((rect.height() - 16.0 - base - RECENT_EXTRA) / RECENT_ROW).floor();
        let shown = self.settings.recent.len().min(fit.max(0.0) as usize).min(8);
        let height = if shown > 0 {
            base + RECENT_EXTRA + shown as f32 * RECENT_ROW
        } else {
            base
        };
        let top = (rect.center().y - height * 0.55).max(rect.top() + 8.0);
        let icon_rect = ui::paint_icon(
            &painter,
            &icon,
            egui::pos2(rect.center().x, top + size / 2.0),
            Color32::WHITE,
        );
        let mut y = icon_rect.bottom() + GAPS[0];
        for (galley, gap) in [(name, GAPS[1]), (version, GAPS[2]), (hint, 0.0)] {
            let h = galley.size().y;
            let x = rect.center().x - galley.size().x / 2.0;
            painter.galley(egui::pos2(x, y), galley, style::TEXT);
            y += h + gap;
        }
        if shown == 0 {
            return None;
        }
        let mut out = None;
        y += 32.0;
        // Centred under the hint, as in metalmd.
        let width = (rect.width() - 48.0).clamp(120.0, 520.0);
        let centre = rect.center().x;
        let centred = |y: f32, size: egui::Vec2| {
            Rect::from_min_size(egui::pos2(centre - size.x / 2.0, y), size)
        };
        let header = layout(tr!("RECENT", "НЕДАВНИЕ").to_owned(), 11.0, style::TEXT_WEAK);
        painter.galley(centred(y, header.size()).min, header, style::TEXT_WEAK);
        y += 20.0 + 8.0;
        let truncated = |text: String, points: f32, color: Color32| {
            let mut job =
                egui::text::LayoutJob::simple_singleline(text, FontId::proportional(points), color);
            job.wrap = egui::text::TextWrapping::truncate_at_width(width);
            painter.layout_job(job)
        };
        for (i, path) in self.settings.recent.iter().take(shown).enumerate() {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let folder = path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            // Laid out in the text colour: the hover only tints it.
            let name = truncated(name, 14.0, style::TEXT);
            let folder = truncated(folder, 11.0, style::TEXT_WEAK);
            let name_rect = centred(y, name.size());
            let folder_rect = centred(name_rect.bottom() + 1.0, folder.size());
            // The name and the folder under it take the click.
            let row = name_rect.union(folder_rect);
            let response = ui
                .interact(row, ui.id().with(("recent", i)), Sense::CLICK)
                .on_hover_text(path.display().to_string());
            let hovered = response.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let color = if hovered {
                style::accent()
            } else {
                style::TEXT
            };
            painter.galley_with_override_text_color(name_rect.min, name, color);
            painter.galley(folder_rect.min, folder, style::TEXT_WEAK);
            if response.clicked() {
                out = Some(Cmd::OpenRecent(i));
            }
            y += RECENT_ROW;
        }
        let clear = layout(
            tr!("Clear the list", "Очистить список").to_owned(),
            12.0,
            style::TEXT_WEAK,
        );
        let clear_rect = centred(y + 4.0, clear.size());
        let response = ui.interact(clear_rect, ui.id().with("recent_clear"), Sense::CLICK);
        let color = if response.hovered() {
            style::accent()
        } else {
            style::TEXT_WEAK
        };
        painter.galley_with_override_text_color(clear_rect.min, clear, color);
        if response.clicked() {
            out = Some(Cmd::ClearRecent);
        }
        out
    }

    /// How long the mouse has rested.
    fn pointer_rest(ctx: &egui::Context) -> Duration {
        Duration::from_secs_f32(ctx.input(|i| i.pointer.time_since_last_movement()).max(0.0))
    }

    /// Which panels over the picture are shown in fullscreen, each on its
    /// own: the control bar while the pointer is near the bottom, a seek is
    /// dragged or the tracks window is open; the strip with the name (when
    /// it is on) while the pointer is near the top.
    fn fullscreen_panels(&self, ctx: &egui::Context) -> Panels {
        let screen = ctx.content_rect();
        // Not over the playlist on the right.
        let pointer = ctx
            .pointer_hover_pos()
            .filter(|p| p.x < screen.right() - self.fullscreen_list_width);
        let (top, bottom) = Self::held(ctx, self.shown_over);
        Panels {
            top: self.settings.fullscreen_title
                && (top
                    || pointer
                        .is_some_and(|p| p.y < screen.top() + FULLSCREEN_TITLE_HEIGHT + 16.0)),
            bottom: bottom
                || pointer.is_some_and(|p| p.y > screen.bottom() - FULLSCREEN_BAR_ZONE)
                || self.seek_drag.is_some()
                || self.video_panel.open
                || self.tracks_window.open,
        }
    }

    /// The panels over the picture (the top, the bottom) of `shown`, the
    /// last frame's, that stay while a menu or a popup is open: the
    /// pointer went from them onto it.
    fn held(ctx: &egui::Context, shown: (bool, bool)) -> (bool, bool) {
        if egui::Popup::is_any_open(ctx) {
            shown
        } else {
            (false, false)
        }
    }

    /// The control bar and the file's name over the picture in fullscreen
    /// ([`Self::fullscreen_panels`]). The pointer hides after a rest away
    /// from them, unless the information window is open.
    fn fullscreen_controls(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        title: &str,
        cmds: &mut Vec<Cmd>,
    ) {
        let panels = self.fullscreen_panels(ctx);
        self.shown_over = (panels.top, panels.bottom);
        let rest = Self::pointer_rest(ctx);
        if rest < HIDE_AFTER {
            ctx.request_repaint_after(HIDE_AFTER - rest);
        } else if !panels.top
            && !panels.bottom
            && !self.info.open
            && !self.video_panel.open
            && !self.tracks_window.open
            && !egui::Popup::is_any_open(ctx)
            && !self.fullscreen_playlist
        {
            ctx.set_cursor_icon(CursorIcon::None);
        }
        let alpha = (255.0 * (1.0 - self.settings.fullscreen_transparency / 100.0)).round() as u8;
        let fill = style::BAR_BG.gamma_multiply_u8(alpha);
        self.fullscreen_list_width = if self.fullscreen_playlist {
            self.fullscreen_playlist_panel(
                ctx,
                state,
                style::PANEL_BG.gamma_multiply_u8(alpha),
                cmds,
            )
        } else {
            0.0
        };
        // The panels leave the playlist's side.
        let mut screen = ctx.content_rect();
        screen.max.x -= self.fullscreen_list_width;

        if panels.top {
            fullscreen_title(ctx, screen, title, fill);
        }
        if !panels.bottom {
            return;
        }
        let compact = self.settings.interface == Interface::Compact;
        let bar = self.controls_bar(state, true, compact);
        egui::Area::new(egui::Id::new("fullscreen_controls"))
            .order(egui::Order::Foreground)
            .anchor(Align2::LEFT_BOTTOM, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(fill)
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 6,
                        bottom: 4,
                    })
                    .show(ui, |ui| {
                        ui.set_width(screen.width());
                        cmds.extend(ui::controls::bar(ui, bar));
                    });
            });
    }

    /// The playlist over the picture's right side in fullscreen, as tall as
    /// the screen, on `fill`; its left edge dragged sets its width (kept in
    /// egui memory, apart from the window's playlist). Its width.
    fn fullscreen_playlist_panel(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        fill: Color32,
        cmds: &mut Vec<Cmd>,
    ) -> f32 {
        const GRIP: f32 = 6.0;
        let screen = ctx.content_rect();
        let id = egui::Id::new("fullscreen_playlist_width");
        let most = (screen.width() * 0.6).max(ui::playlist::MIN_WIDTH);
        let width = ctx
            .data_mut(|d| *d.get_persisted_mut_or(id, 320.0_f32))
            .clamp(ui::playlist::MIN_WIDTH, most);
        let rect = Rect::from_min_max(egui::pos2(screen.right() - width, screen.top()), screen.max);
        let logos = self.settings.channel_logos.then_some(&mut self.logos);
        let (playlist, repeat) = (&mut self.playlist, self.settings.repeat);
        egui::Area::new(egui::Id::new("fullscreen_playlist"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                egui::Frame::NONE.fill(fill).inner_margin(6).show(ui, |ui| {
                    let inner = rect.size() - egui::vec2(12.0, 12.0);
                    ui.set_min_size(inner);
                    ui.set_max_size(inner);
                    cmds.extend(ui::playlist::panel(
                        ui,
                        playlist,
                        logos,
                        repeat,
                        &state.chapters,
                        state.time,
                        false,
                    ));
                });
                // The left edge resizes it.
                let grip = Rect::from_min_size(rect.min, egui::vec2(GRIP, rect.height()));
                let response = ui.interact(grip, id.with("grip"), Sense::drag());
                if response.hovered() || response.dragged() {
                    ctx.set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                if response.dragged() {
                    let wider =
                        (width - response.drag_delta().x).clamp(ui::playlist::MIN_WIDTH, most);
                    ctx.data_mut(|d| d.insert_persisted(id, wider));
                }
            });
        width
    }

    fn update_window_title(&mut self, ctx: &egui::Context, title: &str) {
        let wanted = if title == crate::APP_NAME {
            title.to_owned()
        } else {
            format!("{title} - {}", crate::APP_NAME)
        };
        if self.shown_title != wanted {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(wanted.clone()));
            self.shown_title = wanted;
        }
    }
}

/// Media key `key` at `at` is the press `last` was, come again by the other
/// way (the window's key and Windows' media controls): Play, Pause and
/// Play/Pause are one button.
fn same_press(last: Option<(win::MediaKey, Instant)>, key: win::MediaKey, at: Instant) -> bool {
    use win::MediaKey::{Pause, Play, PlayPause};
    let button = |k: win::MediaKey| match k {
        Play | Pause | PlayPause => PlayPause,
        other => other,
    };
    last.is_some_and(|(k, t)| {
        button(k) == button(key) && at.saturating_duration_since(t) < Duration::from_millis(300)
    })
}

/// Files that came at `at` join those that came last (at `last`): Explorer
/// starts one process per selected file, the first of them this one (its
/// own files count as received at the start).
fn same_batch(last: Option<Instant>, at: Instant) -> bool {
    last.is_some_and(|t| at.saturating_duration_since(t) < SAME_BATCH)
}

/// The panels over the picture in fullscreen that are shown.
struct Panels {
    /// The strip with the file's name.
    top: bool,
    /// The control bar.
    bottom: bool,
}

/// The icon of the start screen in points: 112 as in qview, 64 in a low
/// window (both embedded at 100–200 %).
/// Where saved frames go: Pictures\MetalMPV.
fn frames_dir() -> Option<PathBuf> {
    Some(win::pictures_dir()?.join(crate::APP_NAME))
}

/// The video's display size in pixels, `None` without a picture.
fn video_size(state: &State) -> Option<[f64; 2]> {
    match (state.width, state.height) {
        (Some(w), Some(h)) => Some([w as f64, h as f64]),
        _ => None,
    }
}

fn start_icon_size(rect: Rect) -> f32 {
    if rect.height() >= 340.0 { 112.0 } else { 64.0 }
}

/// The cover of a file: looked for on a thread of its own, then decoded
/// with its texture.
struct CoverSlot {
    path: PathBuf,
    loading: Option<std::sync::mpsc::Receiver<Option<Cover>>>,
    loaded: Option<(Cover, TextureHandle)>,
}

struct FactsSlot {
    path: PathBuf,
    loading: Option<std::sync::mpsc::Receiver<crate::probe::Facts>>,
    loaded: Option<crate::probe::Facts>,
}

/// The font chosen for the OSD line, read from its file; `None` for the
/// interface's font or a file that cannot be used.
fn osd_font(settings: &Settings) -> Option<egui::FontData> {
    let (file, index) = settings.osd_font_file.as_ref()?;
    crate::fonts::read(file, *index)
}

/// The strip with the file's name across the top in fullscreen. It takes no
/// clicks, so a double click there still leaves fullscreen.
fn fullscreen_title(ctx: &egui::Context, screen: Rect, title: &str, fill: Color32) {
    egui::Area::new(egui::Id::new("fullscreen_title"))
        .order(egui::Order::Foreground)
        .interactable(false)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(screen.width(), FULLSCREEN_TITLE_HEIGHT),
                Sense::hover(),
            );
            ui.painter().rect_filled(rect, 0.0, fill);
            let galley = ui.fonts_mut(|f| {
                let mut job = egui::text::LayoutJob::simple_singleline(
                    title.to_owned(),
                    FontId::proportional(14.0),
                    style::TEXT,
                );
                job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 48.0);
                f.layout_job(job)
            });
            let pos = Align2::CENTER_CENTER
                .anchor_size(rect.center(), galley.size())
                .min;
            ui.painter().galley(pos, galley, style::TEXT);
        });
}

/// The pointer is in `zone` and over nothing but the window's own panels
/// (no menu, popup or window of egui).
fn pointer_over(ctx: &egui::Context, zone: Rect) -> bool {
    ctx.pointer_hover_pos().is_some_and(|p| {
        zone.contains(p)
            && ctx
                .layer_id_at(p)
                .is_none_or(|layer| layer.order == egui::Order::Background)
    })
}

/// The menu of a right click on the picture.
fn context_menu(
    ui: &mut egui::Ui,
    state: &State,
    playlist_visible: bool,
    chapter_marks: bool,
    marks: Marks,
    cmds: &mut Vec<Cmd>,
) {
    // Gathered apart: `item` holds `cmds` until its last use.
    let mut menu_cmds = Vec::new();
    let mut item = |ui: &mut egui::Ui, text: String, cmd: Cmd| {
        let button = egui::Button::new(text).shortcut_text(keymap::shortcut(cmd));
        if ui.add(button).clicked() {
            cmds.push(cmd);
            ui.close();
        }
    };
    if marks.interface == Interface::Bare {
        // First, where it is found without the interface.
        item(
            ui,
            tr!("Show the interface", "Вернуть интерфейс").into(),
            Cmd::ToggleInterface,
        );
        ui.separator();
    }
    item(ui, tr!("Open…", "Открыть…").into(), Cmd::Open);
    item(
        ui,
        tr!("Open an address…", "Открыть адрес…").into(),
        Cmd::OpenUrl,
    );
    item(
        ui,
        tr!("File information", "Сведения о файле").into(),
        Cmd::ToggleInfo,
    );
    item(
        ui,
        tr!("Video, sound and subtitles…", "Видео, звук и субтитры…").into(),
        Cmd::ToggleVideoPanel,
    );
    if video_size(state).is_some() {
        item(
            ui,
            tr!("Save the frame", "Сохранить кадр").into(),
            Cmd::SaveFrame,
        );
    }
    ui.separator();
    let pause = if state.paused || state.idle {
        tr!("Play", "Воспроизвести")
    } else {
        tr!("Pause", "Пауза")
    };
    item(ui, pause.into(), Cmd::TogglePause);
    item(ui, tr!("Stop", "Стоп").into(), Cmd::Stop);
    if !state.idle {
        item(
            ui,
            tr!("Go to time…", "Перейти ко времени…").into(),
            Cmd::GoToTime,
        );
    }
    item(
        ui,
        tr!("Previous file", "Предыдущий файл").into(),
        Cmd::Prev,
    );
    item(ui, tr!("Next file", "Следующий файл").into(), Cmd::Next);
    ui.separator();
    item(
        ui,
        tr!("Fullscreen", "Полный экран").into(),
        Cmd::ToggleFullscreen,
    );
    interface_menu(ui, marks.interface, &mut menu_cmds);
    zoom_menu(ui, state, marks.fill, &mut menu_cmds);
    on_top_menu(ui, marks.on_top, &mut menu_cmds);
    aspect_menu(ui, state, marks.aspect, &mut menu_cmds);
    speed_menu(ui, state, &mut menu_cmds);
    repeat_menu(ui, marks.repeat, &mut menu_cmds);
    loop_menu(ui, state, &mut menu_cmds);
    let list = if playlist_visible {
        tr!("Hide the playlist", "Скрыть плейлист")
    } else {
        tr!("Show the playlist", "Показать плейлист")
    };
    item(ui, list.into(), Cmd::TogglePlaylist);
    let window = if marks.playlist_detached {
        tr!("Playlist in the player's window", "Плейлист в окне плеера")
    } else {
        tr!(
            "Playlist in a window of its own",
            "Плейлист в отдельном окне"
        )
    };
    item(ui, window.into(), Cmd::TogglePlaylistWindow);
    let mute = if state.mute {
        tr!("Sound on", "Включить звук")
    } else {
        tr!("Mute", "Без звука")
    };
    item(ui, mute.into(), Cmd::ToggleMute);
    ui.separator();
    crate::ui::tracks::menus(ui, &state.tracks, &mut menu_cmds);
    chapters_menu(ui, state, chapter_marks, &mut menu_cmds);
    ui.separator();
    item(ui, tr!("Settings…", "Настройки…").into(), Cmd::Settings);
    item(ui, tr!("About", "О программе").into(), Cmd::About);
    cmds.extend(menu_cmds);
}

/// The width of the playlist's own window the first time, in points.
const PLAYLIST_WIDTH: f32 = 300.0;

/// The files dropped on the window `ctx` is drawing.
fn dropped_files(ctx: &egui::Context) -> Vec<PathBuf> {
    ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect()
    })
}

/// What the context menu marks as chosen.
struct Marks {
    fill: bool,
    /// The interface shown (`8`; only the picture, `7`).
    interface: Interface,
    aspect: crate::adjust::Aspect,
    repeat: Repeat,
    on_top: OnTop,
    playlist_detached: bool,
}

/// The interfaces, the current marked.
fn interface_menu(ui: &mut egui::Ui, current: Interface, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Interface", "Интерфейс"), |ui| {
        for i in Interface::ALL {
            let key = if i == Interface::Bare {
                keymap::shortcut(Cmd::ToggleInterface)
            } else {
                String::new()
            };
            let button = egui::Button::new(i.label())
                .shortcut_text(key)
                .selected(current == i);
            if ui.add(button).clicked() {
                out.push(Cmd::SetInterface(i));
                ui.close();
            }
        }
    });
}

/// Over the other windows: never, always, while playing; the current
/// marked.
fn on_top_menu(ui: &mut egui::Ui, current: OnTop, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("On top", "Поверх других окон"), |ui| {
        for o in OnTop::ALL {
            if ui.radio(current == o, o.label()).clicked() {
                out.push(Cmd::SetOnTop(o));
                ui.close();
            }
        }
        switches(ui, Cmd::CycleOnTop);
    });
}

/// The line under a submenu that names the key cycling through it, if
/// it has one.
fn switches(ui: &mut egui::Ui, cmd: Cmd) {
    let key = keymap::shortcut(cmd);
    if !key.is_empty() {
        ui.separator();
        ui.weak(tr!(
            format!("{key} switches it"),
            format!("{key} переключает")
        ));
    }
}

/// Repeat off, the file or the list, the current marked.
fn repeat_menu(ui: &mut egui::Ui, current: Repeat, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Repeat", "Повтор"), |ui| {
        for r in Repeat::ALL {
            if ui.radio(current == r, r.label()).clicked() {
                out.push(Cmd::SetRepeat(r));
                ui.close();
            }
        }
        switches(ui, Cmd::CycleRepeat);
    });
}

/// The A-B repeat: its start and its end at the position now, off; the
/// points set, if any.
fn loop_menu(ui: &mut egui::Ui, state: &State, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("A-B repeat", "Повтор A-B"), |ui| {
        let lang = crate::i18n::lang();
        let point = |t: Option<f64>| t.map(|t| crate::format::clock_tenths_in(lang, t));
        let items = [
            (
                Cmd::LoopA,
                tr!("Start here (A)", "Начало здесь (A)"),
                !state.idle,
                point(state.loop_a),
            ),
            (
                Cmd::LoopB,
                tr!("End here (B)", "Конец здесь (B)"),
                !state.idle,
                point(state.loop_b),
            ),
            (
                Cmd::LoopClear,
                tr!("Off", "Выключить"),
                state.loop_a.is_some() || state.loop_b.is_some(),
                None,
            ),
        ];
        for (cmd, text, enabled, at) in items {
            let text = match at {
                Some(at) => format!("{text}: {at}"),
                None => text.to_owned(),
            };
            let button = egui::Button::new(text).shortcut_text(keymap::shortcut(cmd));
            if ui.add_enabled(enabled, button).clicked() {
                out.push(cmd);
                ui.close();
            }
        }
    });
}

/// The playback speed: the steps of `format::SPEEDS`, the current marked,
/// with the keys that change it.
fn speed_menu(ui: &mut egui::Ui, state: &State, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Speed", "Скорость"), |ui| {
        for speed in crate::format::SPEEDS {
            let text = crate::format::speed_in(crate::i18n::lang(), speed);
            let key = if speed == 1.0 {
                keymap::shortcut(Cmd::Speed(0))
            } else {
                String::new()
            };
            let button = egui::Button::new(text)
                .shortcut_text(key)
                .selected((state.speed - speed).abs() < 1e-9);
            if ui.add(button).clicked() {
                out.push(Cmd::SetSpeed(speed));
                ui.close();
            }
        }
        let faster = keymap::shortcut(Cmd::Speed(1));
        let slower = keymap::shortcut(Cmd::Speed(-1));
        if !faster.is_empty() || !slower.is_empty() {
            ui.separator();
            let line = match (faster.is_empty(), slower.is_empty()) {
                (false, false) => tr!(
                    format!("{faster} faster, {slower} slower"),
                    format!("{faster} быстрее, {slower} медленнее")
                ),
                (false, true) => tr!(format!("{faster} faster"), format!("{faster} быстрее")),
                _ => tr!(format!("{slower} slower"), format!("{slower} медленнее")),
            };
            ui.weak(line);
        }
    });
}

/// The picture's shape: the file's or a standard one, the current marked.
fn aspect_menu(
    ui: &mut egui::Ui,
    state: &State,
    current: crate::adjust::Aspect,
    out: &mut Vec<Cmd>,
) {
    ui.menu_button(tr!("Aspect ratio", "Пропорции"), |ui| {
        let video = video_size(state).is_some();
        for a in crate::adjust::Aspect::ALL {
            if ui
                .add_enabled(video, egui::RadioButton::new(current == a, a.label()))
                .clicked()
            {
                out.push(Cmd::SetAspect(a));
                ui.close();
            }
        }
    });
}

/// The picture's size: the window for 50 % to 200 % of the video, the
/// largest window without black bars, the window to the picture shown,
/// and fullscreen without black bars.
fn zoom_menu(ui: &mut egui::Ui, state: &State, fill: bool, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Zoom", "Масштаб"), |ui| {
        let video = video_size(state).is_some();
        let mut item = |ui: &mut egui::Ui, text: &str, selected: bool, cmd: Cmd| {
            let button = egui::Button::new(text)
                .shortcut_text(keymap::shortcut(cmd))
                .selected(selected);
            if ui.add_enabled(video, button).clicked() {
                out.push(cmd);
                ui.close();
            }
        };
        for scale in [0.5, 1.0, 1.5, 2.0] {
            let text = format!("{:.0}%", scale * 100.0);
            item(ui, &text, false, Cmd::WindowScale(scale));
        }
        ui.separator();
        item(
            ui,
            tr!("Window to the screen", "Окно по экрану"),
            false,
            Cmd::FitScreen,
        );
        item(
            ui,
            tr!("Window to the picture", "Окно по размеру картинки"),
            false,
            Cmd::FitWindow,
        );
        ui.separator();
        item(
            ui,
            tr!(
                "Fullscreen without black bars (crops)",
                "Полный экран без чёрных полос (с обрезкой)"
            ),
            fill,
            Cmd::ToggleFill,
        );
    });
}

/// The chapters' submenu: the marks on the seek bar, then the chapters,
/// the playing one marked; a click goes to one.
fn chapters_menu(ui: &mut egui::Ui, state: &State, marks: bool, out: &mut Vec<Cmd>) {
    ui.menu_button(tr!("Chapters", "Главы"), |ui| {
        let mut on = marks;
        if ui
            .checkbox(
                &mut on,
                tr!("Marks on the seek bar", "Метки на полосе перемотки"),
            )
            .clicked()
        {
            out.push(Cmd::ToggleChapterMarks);
        }
        ui.separator();
        if state.idle || state.chapters.is_empty() {
            ui.weak(tr!("No chapters in the file", "В файле нет глав"));
            return;
        }
        let current = state
            .time
            .and_then(|t| crate::chapters::current(&state.chapters, t));
        let long = state.duration.is_some_and(|d| d >= 3600.0);
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                for (i, c) in state.chapters.iter().enumerate() {
                    let text = format!(
                        "{}. {}  ({})",
                        i + 1,
                        c.name(i + 1),
                        crate::format::time(Some(c.time), long)
                    );
                    if ui.radio(current == Some(i), text).clicked() {
                        out.push(Cmd::Chapter(i));
                        ui.close();
                    }
                }
            });
    });
}

impl eframe::App for App {
    /// Every frame, and alone while the window is minimised: the player's
    /// events keep a playlist going.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.updates.poll();
        self.take_events();
        self.take_hooked();
        if let Some(why) = self.save_now.take()
            && let Some(storage) = frame.storage_mut()
        {
            log::info!("{why}: saving");
            self.save(storage);
            storage.flush();
        }
        self.follow_audio_device();
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        // eframe calls no `ui` for a minimised window: the thumbnail
        // buttons' commands run here, else they waited for the window.
        if minimized {
            for cmd in std::mem::take(&mut self.pending) {
                self.run(ctx, frame, cmd);
            }
        }
        // No picture is painted: mpv's frames are taken unseen.
        if (minimized || self.state_idle())
            && let Some(video) = &self.video
        {
            video.skip_unseen();
        }
        self.keep_awake(minimized);
        win::set_playlist_follow(!self.fullscreen);
        self.follow_on_top(ctx);
        self.show_on_taskbar();
        self.follow_media_keys();
        self.show_in_media_controls();
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.uncloak(&ctx);
        let fullscreen = frameless::is_fullscreen(&ctx);
        // Fullscreen left by other means (Esc, Enter, `1`…): the picture is
        // whole again.
        if self.fullscreen && !fullscreen && self.fill {
            self.set_fill(false);
        }
        self.fullscreen = fullscreen;
        if !fullscreen {
            self.fullscreen_playlist = false;
            self.fullscreen_list_width = 0.0;
        }
        let maximized = frameless::is_maximized(&ctx);
        // The interface changed while maximized or in fullscreen: the
        // restored window is made for it now.
        self.fit_interface(&ctx);
        let state = self.state();
        let mut cmds = std::mem::take(&mut self.pending);

        self.drop_files(dropped_files(&ctx), &state);

        // The keys are the player's: no widget keeps the focus to take
        // Space or Enter as a click. A modal takes the keys while open.
        let keys = !self.video_panel.typing(&ctx)
            && !self.about.open
            && !self.settings_dialog.open
            && !self.url_dialog.is_open()
            && !self.goto_dialog.is_open();
        if keys && !egui::Popup::is_any_open(&ctx) {
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            cmds.extend(input::keys(&ctx, &self.settings.keys));
        }

        if !fullscreen && !maximized {
            frameless::resize_edges(root_ui);
        }
        let title = self.title(&state);
        let interface = self.interface();
        let (top_held, bottom_held) = Self::held(&ctx, self.shown_over);
        self.title_strip =
            !fullscreen && interface.title_over() && (top_held || self.wants_title_strip(&ctx));
        let bar_over = !fullscreen
            && interface == Interface::Bare
            && (bottom_held || self.wants_bar_over(&ctx));
        if !fullscreen {
            self.shown_over = (self.title_strip, bar_over);
        }
        let icon = (!fullscreen && (interface.title_row() || self.title_strip))
            .then(|| ui::app_icon(&ctx, 16.0, &mut self.icon));
        let newer = self.updates.newer().map(str::to_owned);
        if !fullscreen && interface != Interface::Bare {
            if let Some(icon) = icon.as_ref().filter(|_| interface.title_row()) {
                egui::Panel::top("title")
                    .exact_size(style::TITLE_HEIGHT)
                    .frame(egui::Frame::NONE.fill(style::TITLE_BG))
                    .show_separator_line(false)
                    .show(root_ui, |ui| {
                        if let Some(cmd) =
                            ui::title::row(ui, &title, maximized, icon, newer.as_deref())
                        {
                            cmds.push(cmd);
                        }
                    });
            }
            let compact = interface == Interface::Compact;
            let bar = self.controls_bar(&state, false, compact);
            egui::Panel::bottom("controls")
                .exact_size(ui::controls::height(compact))
                .frame(ui::controls::frame(compact))
                .show_separator_line(false)
                .show(root_ui, |ui| cmds.extend(ui::controls::bar(ui, bar)));
            if self.settings.playlist_visible && !self.settings.playlist_detached {
                egui::Panel::right("playlist")
                    .resizable(true)
                    .default_size(260.0)
                    .size_range(ui::playlist::MIN_WIDTH..=600.0)
                    .frame(egui::Frame::NONE.fill(style::PANEL_BG).inner_margin(6))
                    .show(root_ui, |ui| {
                        let logos = self.settings.channel_logos.then_some(&mut self.logos);
                        cmds.extend(ui::playlist::panel(
                            ui,
                            &mut self.playlist,
                            logos,
                            self.settings.repeat,
                            &state.chapters,
                            state.time,
                            false,
                        ))
                    });
            }
        }
        self.playlist_window(&ctx, &state, &mut cmds, keys);
        let picture = egui::CentralPanel::no_frame()
            .show(root_ui, |ui| {
                self.video_area(ui, frame, &state, &mut cmds);
                // Over the picture, in its layer: the edges' strips under
                // it stay as under the title row.
                if self.title_strip
                    && let Some(icon) = &icon
                {
                    let rect = Rect::from_min_size(
                        ui.max_rect().min,
                        egui::vec2(ui.max_rect().width(), style::TITLE_HEIGHT),
                    );
                    if let Some(cmd) =
                        ui::title::over(ui, rect, &title, maximized, icon, newer.as_deref())
                    {
                        cmds.push(cmd);
                    }
                }
                if bar_over {
                    let compact = self.settings.interface == Interface::Compact;
                    let area = ui.max_rect();
                    let rect = Rect::from_min_max(
                        egui::pos2(area.left(), area.bottom() - ui::controls::height(compact)),
                        area.max,
                    );
                    let mut child =
                        ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("bar_over"));
                    let bar = self.controls_bar(&state, false, compact);
                    ui::controls::frame(compact)
                        .fill(style::BAR_BG.gamma_multiply_u8(235))
                        .show(&mut child, |ui| {
                            ui.set_min_size(ui.max_rect().size());
                            cmds.extend(ui::controls::bar(ui, bar));
                        });
                }
            })
            .response
            .rect;
        self.picture_area = picture;
        if !fullscreen {
            if interface != Interface::Bare {
                self.playlist_inset = (ctx.content_rect().right() - picture.right()).max(0.0);
            }
            self.chrome = ctx.content_rect().size() - picture.size();
            if let Some(picture) = self.pending_picture.take() {
                self.size_window(&ctx, frame, picture);
            }
        }
        if fullscreen {
            self.fullscreen_controls(&ctx, &state, &title, &mut cmds);
        }

        // Before the settings too: their Reset sets these, and no flag of
        // `Changed` tells.
        let layout_before = (self.settings.sub_ass_override, self.settings.sub_margins);
        let changed = self.settings_dialog.show(
            &ctx,
            &mut self.settings,
            &mut self.updates,
            state.hwdec.as_deref(),
            &state.audio_devices,
        );
        if changed.keys {
            crate::keymap::publish(&self.settings.keys);
        }
        if changed.accent {
            style::set_accent(self.settings.accent);
            style::apply(&ctx);
        }
        // Not while a colour is dragged in the picker: once let go.
        if self.window_icon != self.settings.accent && !ctx.input(|i| i.pointer.any_down()) {
            self.window_icon = self.settings.accent;
            ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(crate::window_icon(
                self.window_icon,
            ))));
        }
        if changed.hwdec
            && let Some(player) = &mut self.player
        {
            player.set_hwdec(self.settings.hwdec);
        }
        if self.settings_dialog.open {
            win::set_snap(self.settings.snap_to_centre);
            if !self.settings.remember_recent {
                self.settings.recent.clear();
            }
        }
        if let Some(player) = &mut self.player {
            if changed.audio_channels {
                player.set_channels(self.settings.audio_channels);
            }
            if changed.volume_max {
                player.set_volume_max(self.settings.volume_max);
            }
            if changed.passthrough {
                player.set_passthrough(self.settings.passthrough);
            }
        }
        if changed.subs
            && let Some(player) = &mut self.player
        {
            player.set_sub_style(&self.settings.sub);
        }
        if changed.forget_positions {
            self.positions.clear();
            self.settings.recent.clear();
            self.settings.last_audio = None;
            self.settings.last_sub = None;
        }
        if changed.osd_font {
            let data = osd_font(&self.settings);
            if data.is_none() {
                // Unreadable: back to the interface's font.
                self.settings.osd_font.clear();
                self.settings.osd_font_file = None;
            }
            style::set_fonts(&ctx, data);
        }
        self.about.show(&ctx);
        if let Some(url) = self.url_dialog.show(&ctx) {
            self.open(vec![PathBuf::from(url)]);
        }
        if let Some(time) = self.goto_dialog.show(&ctx, state.time, state.duration) {
            // Exact: the time typed is the frame wanted.
            self.seek_to(time, Seek::Exact);
        }
        self.update_cover(&ctx, &state);
        self.update_facts(&ctx, &state);
        let cover = self
            .cover
            .as_ref()
            .and_then(|c| c.loaded.as_ref())
            .map(|(cover, texture)| CoverView { cover, texture });
        if let Some(player) = &mut self.player {
            if self.info.open {
                player.ask_scan();
            }
            // The figures of the moment, for the information window and
            // the panel's Sound tab; they come every second, paused or not.
            if self.info.open || self.video_panel.open {
                player.ask_live();
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        let channel = self
            .playlist
            .current_path()
            .and_then(|p| self.playlist.channel(p));
        let channel = channel.map(|channel| ChannelView {
            channel,
            logo: channel
                .logo
                .as_deref()
                .filter(|_| self.settings.channel_logos && self.info.open)
                .and_then(|url| self.logos.get(&ctx, url).cloned()),
        });
        let path = self.playlist.current_path();
        let subject = crate::info::Subject {
            path,
            state: &state,
            facts: self
                .facts
                .as_ref()
                .filter(|f| Some(f.path.as_path()) == path)
                .and_then(|f| f.loaded.as_ref()),
        };
        let info_cmds = self.info.show(&ctx, &subject, cover, channel);
        cmds.extend(info_cmds);
        let before = self.adjust.clone();
        let subs_before = self.settings.sub.clone();
        let output_before = (self.settings.audio_channels, self.settings.volume_max);
        let voice_boost = self.settings.voice_boost;
        let kept = ui::picture::Kept {
            audio_device: &mut self.settings.audio_device,
            devices: &state.audio_devices,
            channels: &mut self.settings.audio_channels,
            volume_max: &mut self.settings.volume_max,
            normalize: &mut self.settings.normalize,
            voice_boost: &mut self.settings.voice_boost,
            equalizer: &mut self.settings.equalizer,
            eq_presets: &mut self.settings.eq_presets,
            subs: &mut self.settings.sub,
            sub_presets: &mut self.settings.sub_presets,
            sub_ass_override: &mut self.settings.sub_ass_override,
            sub_margins: &mut self.settings.sub_margins,
            keep_adjust: &mut self.settings.keep_adjust,
        };
        let panel_cmds = self.video_panel.show(
            &ctx,
            self.panel_button.take(),
            &mut self.adjust,
            kept,
            ui::picture::Now {
                has_video: video_size(&state).is_some(),
                audio_delay: state.audio_delay,
                sub_delay: state.sub_delay,
                audio: tracks::selected(&state.tracks, Kind::Audio),
                audio_out: state.live.audio_out.as_ref(),
                audio_bitrate: state.live.audio_bitrate,
                voice_boost,
            },
        );
        cmds.extend(panel_cmds);
        let track_cmds = self.tracks_window.show(
            &ctx,
            self.tracks_button.take(),
            &state.tracks,
            &mut self.settings.tracks_wide,
        );
        cmds.extend(track_cmds);
        if let Some(player) = &mut self.player {
            if self.adjust != before {
                player.adjust(&before, &self.adjust);
            }
            if self.settings.sub != subs_before {
                player.set_sub_style(&self.settings.sub);
            }
            let layout = (self.settings.sub_ass_override, self.settings.sub_margins);
            if layout != layout_before {
                player.set_sub_layout(layout.0, layout.1);
            }
            if self.settings.audio_channels != output_before.0 {
                player.set_channels(self.settings.audio_channels);
            }
            if self.settings.volume_max != output_before.1 {
                player.set_volume_max(self.settings.volume_max);
            }
        }
        self.sync_audio_filters(&ctx, &state);

        for cmd in cmds {
            self.run(&ctx, frame, cmd);
        }
        // Chosen in the settings.
        self.fit_interface(&ctx);
        self.update_window_title(&ctx, &title);
    }

    /// More often while a file plays, so that where it was left is kept
    /// when the process is ended or the power goes.
    fn auto_save_interval(&self) -> Duration {
        let playing = self
            .player
            .as_ref()
            .is_some_and(|p| !p.state.idle && !p.state.paused && !p.state.eof);
        if playing {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(30)
        }
    }

    /// Called by eframe every `auto_save_interval` and on exit.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.check_updates = self.updates.enabled;
        self.settings.last_update_check = self.updates.last_check;
        // Kept only while the panel's switch says so.
        self.settings.adjust = if self.settings.keep_adjust {
            self.adjust.clone()
        } else {
            Adjust::default()
        };
        if let Some(player) = &self.player {
            self.settings.volume = player.state.volume;
            self.settings.mute = player.state.mute;
        }
        // The restored rectangle of a maximized window (eframe saves the
        // maximized size); not in fullscreen, where it is the screen.
        if let Some(hwnd) = self.hwnd.filter(|_| !self.fullscreen)
            && let Some(r) = win::normal_rect(hwnd)
        {
            self.settings.window_normal = Some(r);
        }
        self.settings.save(storage);
        self.remember_position();
        storage.set_string(POSITIONS_KEY, self.positions.to_text());
    }

    /// The render context goes first, while the OpenGL context lives, then
    /// the core.
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(video) = &mut self.video {
            video.free(gl);
        }
        self.video = None;
        self.player = None;
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_media_key_counts_once() {
        use win::MediaKey::{Next, Pause, PlayPause};
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        // The window's Play/Pause, then the media controls' Pause.
        assert!(same_press(Some((PlayPause, t)), Pause, ms(40)));
        assert!(!same_press(Some((PlayPause, t)), Pause, ms(500)));
        assert!(!same_press(Some((PlayPause, t)), Next, ms(40)));
        assert!(!same_press(None, Next, t));
    }

    #[test]
    fn files_of_other_starts_join_the_first() {
        let start = Instant::now();
        // The start's own files, then another start's 100 ms later.
        assert!(same_batch(Some(start), start + Duration::from_millis(100)));
        assert!(!same_batch(Some(start), start + Duration::from_secs(2)));
        // Nothing of its own: the files replace the playlist.
        assert!(!same_batch(None, start));
    }
}
