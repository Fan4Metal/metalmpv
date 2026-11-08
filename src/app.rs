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
//!
//! Here: the `App`'s state, its making, opening and playing entries and
//! eframe's `App`. The rest is in parts, each an `impl App` of its own
//! with its free items: `commands` (`App::run`), `events` (the player's
//! events and what follows the file), `files` (dialogs, saving, dropped and
//! pasted files), `layout` (the interface and the window's size),
//! `video_area` (the picture, the start screen), `fullscreen`, `menu` (the
//! context menu), `playlist_window`, `system` (Windows: hooked messages,
//! the taskbar, media controls, sleep) and `mini` (the mini player).

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CursorIcon, FontId, Rect, Sense, TextureHandle};

use crate::adjust::Adjust;
use crate::cover::Cover;
use crate::dialog::{self, Ask, Dialogs};
use crate::format::Osd;
use crate::frameless;
use crate::input::{self, Cmd};
use crate::keymap;
use crate::mpv::Mpv;
use crate::player::{DataEnd, Happened, Player, Seek, State};
use crate::playlist::{self, Playlist, Repeat};
use crate::resume::{Positions, Tracks};
use crate::settings::{Interface, OnTop, Settings, WindowOnOpen};
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

mod commands;
mod events;
mod files;
mod fullscreen;
mod layout;
mod menu;
mod mini;
mod playlist_window;
mod system;
mod video_area;

// The free items of the parts, for one another (each part takes
// `super::*`).
use events::*;
use files::*;
use fullscreen::*;
use layout::*;
use menu::*;

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
/// Files handed over by other starts this soon after the last ones make
/// one playlist (Explorer starts one process per selected file, one after
/// another). Short: a double click on another file comes later and
/// plays it; with 1.5 s it was added to the playlist.
const SAME_BATCH: Duration = Duration::from_millis(300);
/// How long the window stays cloaked at most as it is shown (maximized).
const UNCLOAK_TIMEOUT: Duration = Duration::from_secs(1);
/// Frames the window paints, cloaked, before it is shown.
const PAINTED_TO_SHOW: u64 = 3;
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
    /// Files that failed one after another: with the list repeated, a list
    /// of broken files is gone round once, not for good.
    failed_in_row: usize,
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
    clip_dialog: ui::clip::ClipDialog,
    /// The video map being made, and the frames the OSD said were taken.
    video_map: Option<(crate::video_map::Job, u32)>,
    /// The Video, sound and subtitles panel (`U`, `F7`) and the picture's adjustments it edits.
    video_panel: VideoPanel,
    /// Where the control bar drew the panel's button this frame.
    panel_button: Option<egui::Rect>,
    /// The tracks window (`T`) and where the bar drew its button.
    tracks_window: ui::tracks::TracksWindow,
    tracks_button: Option<egui::Rect>,
    /// The audio filters mpv has (`App::sync_audio_filters`).
    af_applied: String,
    /// The channel layout of the audio track the filters were made for
    /// (the voice's filter and the upmix depend on it): kept while a file
    /// loads (no tracks yet), else they went and came back with each file.
    af_layout: Option<String>,
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
    /// The file dialog open, on a thread of its own.
    dialogs: Dialogs,
    /// Where the next file loaded starts, a bookmark's place (taken by
    /// `play` when the file is that one).
    start_at: Option<(PathBuf, f64)>,
    /// The app icon of the title row and of the start screen, each at its
    /// size.
    icon: ui::IconCache,
    start_icon: ui::IconCache,
    /// The lines of the subtitle file shown (one of its own), for the
    /// panel's timeline: the file, what was read of it, and the thread
    /// reading it meanwhile.
    sub_lines: Option<SubLines>,
    /// The playlist panel's search, folded groups, drag and scrolling.
    list_state: ui::playlist::ListState,
    /// The start screen's recent files while they scroll: the wheel there
    /// scrolls them instead of setting the volume.
    recent_scroll: Option<Rect>,
    /// The accent of the window's icon (blue from `main`).
    window_icon: style::Accent,
    /// The playlist's own window (`dock.rs`): how egui makes it, while it
    /// is open, and its handle once found.
    playlist_builder: Option<egui::ViewportBuilder>,
    playlist_hwnd: Option<isize>,
    /// The playlist's window made hidden (no white flash): frames it was
    /// looked for, and frames painted since it was shown (uncloaked at
    /// `PLAYLIST_PAINTED`).
    playlist_looked: u32,
    playlist_painted: u32,
    /// The seek bar is being dragged: the position under the pointer.
    seek_drag: Option<f64>,
    /// When the last file came from another start, or this start's own
    /// files were opened: Explorer starts one copy per selected file, and
    /// the others' files join the first's.
    last_received: Option<Instant>,
    /// The window title shown, to change it only when it changes.
    shown_title: String,
    /// The window is cloaked until it has painted a few frames (else it
    /// flashed white as it showed), and been maximized when it is to be:
    /// the time it is shown anyway, the frame it was cloaked in, maximized.
    cloak: Option<(Instant, u64, bool)>,
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
    /// What the kept playlist's file holds (`restore_playlist`).
    session: crate::session::Stored,
    /// The frames over the seek bar (`seek_preview`).
    thumbs: Option<crate::thumbs::Thumbs>,
    /// How the picture is drawn, as mpv was given it last (`quality.rs`).
    quality: Option<crate::quality::Quality>,
    /// The screen's refresh rate for the smooth motion: the monitor it was
    /// read for and when.
    refresh: Option<(isize, Instant, Option<f64>)>,
    /// The file being loaded gets the tracks most like the last chosen
    /// (`prefer_last_tracks`), once it is loaded (`pick_ready`).
    pick_tracks: bool,
    pick_ready: bool,
    /// A file was loaded: the window is to be sized for its picture once
    /// its size is known (`window_on_open`).
    fit_on_open: bool,
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
    /// What a shrink for an interface could not take off (the window at
    /// its least size), in pixels across and down, with the rectangle it
    /// left: the next growth from there is that much less, else `7` twice
    /// left a small window larger.
    fit_slack: Option<([i32; 4], [i32; 2])>,
    /// Points to pixels as of the last frame, for `save`.
    ppp: f32,
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
    /// The last media key taken, the way it came and when: the same press
    /// may come as the window's key, its `WM_APPCOMMAND` and from the
    /// media controls.
    last_media: Option<(win::MediaKey, win::Via, Instant)>,
    /// The window is over the other windows now (`settings.on_top`).
    on_top: bool,
    /// The mini player (`0`, `mini.rs`): the window before it.
    mini: Option<mini::Before>,
    /// The mini player was asked for in fullscreen: it comes once that has
    /// ended.
    mini_pending: bool,
    /// The picture's shape the mini player was made for.
    mini_aspect: Option<f64>,
    /// The window's frame around its client area as the mini player came,
    /// in pixels across and down.
    mini_border: [i32; 2],
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
        style::set_theme(settings.theme);
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
            win::cloak(hwnd, true);
            let maximized = crate::MAXIMIZE_WHEN_SHOWN.swap(false, Ordering::Relaxed);
            if maximized {
                // A rectangle on a monitor no longer there would restore
                // the window off the screen.
                let normal = settings.window_normal.filter(|r| win::on_a_monitor(*r));
                win::show_maximized(hwnd, normal, crate::DEFAULT_SIZE);
            }
            cloak = Some((
                Instant::now() + UNCLOAK_TIMEOUT,
                ctx.cumulative_frame_nr(),
                maximized,
            ));
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
                // Always: the default look is not mpv's.
                player.set_sub_style(&settings.sub);
                player.set_sub_layout(settings.sub_ass_override, settings.sub_margins);
                player.set_track_prefs(&settings.track_prefs);
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
        let af_applied = crate::audio::filters(
            settings.normalize.then_some(settings.normalize_strength),
            &settings.equalizer,
            None,
            None,
            crate::audio::Subwoofer::default(),
            false,
        );
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
            failed_in_row: 0,
            frames_saving: std::collections::VecDeque::new(),
            playlist: Playlist::default(),
            about: AboutDialog::default(),
            info: InfoWindow::default(),
            url_dialog: ui::url::UrlDialog::default(),
            goto_dialog: ui::goto::GoToDialog::default(),
            clip_dialog: ui::clip::ClipDialog::default(),
            video_map: None,
            video_panel: VideoPanel::default(),
            panel_button: None,
            tracks_window: ui::tracks::TracksWindow::default(),
            tracks_button: None,
            af_applied,
            af_layout: None,
            logos: crate::logos::Logos::default(),
            adjust,
            cover: None,
            facts: None,
            settings_dialog: SettingsDialog::default(),
            updates,
            osd: OsdLine::default(),
            hwnd,
            dialogs: Dialogs::default(),
            start_at: None,
            icon: None,
            start_icon: None,
            recent_scroll: None,
            list_state: ui::playlist::ListState::default(),
            sub_lines: None,
            window_icon: style::Accent::Blue,
            playlist_builder: None,
            playlist_hwnd: None,
            playlist_looked: 0,
            playlist_painted: 0,
            seek_drag: None,
            last_received: (!files.is_empty()).then(Instant::now),
            shown_title: String::new(),
            cloak,
            pending: Vec::new(),
            first_frame_logged: false,
            fullscreen: false,
            last_sub: None,
            positions,
            session: crate::session::Stored::Unknown,
            thumbs: None,
            quality: None,
            refresh: None,
            pick_tracks: false,
            pick_ready: false,
            fit_on_open: false,
            fill: false,
            before_bare: Interface::Full,
            sized_for: Interface::Full,
            fit_slack: None,
            ppp: 1.0,
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
            mini: None,
            mini_pending: false,
            mini_aspect: None,
            mini_border: [0, 0],
        };
        app.sized_for = app.settings.interface;
        // Before the first file: the scalers, shaders and sync it starts with.
        app.follow_quality();
        if files.is_empty() {
            app.restore_playlist();
        } else {
            app.open(files);
        }
        app
    }

    /// The playlist of the last run, its entry current, nothing played:
    /// Play goes on with it (from where it was left).
    fn restore_playlist(&mut self) {
        if !self.settings.restore_playlist {
            return;
        }
        let Some(kept) = crate::session::load() else {
            return;
        };
        self.playlist.replace(kept.paths);
        self.playlist.describe(kept.channels);
        if let Some(current) = kept.current {
            self.playlist.set_current(current);
        }
        self.session =
            crate::session::Stored::List(self.playlist.revision(), self.playlist.current());
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
    /// brings the media files of its folder along when the setting says so
    /// (only a sound file its folder's sound files, with `fill_audio_only`).
    fn open(&mut self, paths: Vec<PathBuf>) {
        // A playlist file is its entries, not one of a folder's files.
        let from_list = paths.iter().any(|p| playlist::is_playlist_file(p));
        let (paths, channels) = playlist::expand_lists(paths);
        let Some(target) = paths.first().cloned() else {
            return;
        };
        let audio_only = self.settings.fill_audio_only;
        let fill = paths.len() == 1
            && !from_list
            && self.settings.fill_playlist
            && (!audio_only || playlist::is_audio(&target))
            && target.is_file();
        let list = if fill {
            playlist::folder_of(&target, audio_only)
        } else {
            paths
        };
        self.failed_in_row = 0;
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

    /// Append `paths` to the playlist; play the first of them (also one
    /// already listed) if nothing is playing or loading: the files of
    /// other starts of a selection in Explorer come while the start's own
    /// is loading, mpv still idle, and replaced it.
    fn add(&mut self, paths: Vec<PathBuf>) {
        let (paths, channels) = playlist::expand_lists(paths);
        self.playlist.describe(channels);
        let before = self.playlist.len();
        let first = self.playlist.add(paths);
        let busy = self
            .player
            .as_ref()
            .is_some_and(|p| !p.state.idle || p.loading());
        if let Some(first) = first
            && !busy
        {
            self.play(first);
            return;
        }
        // Only added, out of sight: said, else nothing seemed to happen.
        let added = self.playlist.len() - before;
        let shown = if self.fullscreen {
            self.fullscreen_playlist
        } else {
            self.settings.playlist_visible && self.interface() != Interface::Bare
        };
        if added > 0 && !shown {
            self.osd.show_always(Osd::Text(tr!(
                format!("[ Added to the playlist: {added} ]"),
                format!("[ Добавлено в плейлист: {added} ]")
            )));
        }
    }

    /// Play playlist entry `index`.
    fn play(&mut self, index: usize) {
        // The file playing chosen again starts over, not where it is. By
        // mpv's path: a list just replaced makes its first entry current,
        // and a file opened while another played came back at 0.
        let playing = self.player.as_ref().and_then(|p| p.state.path.clone());
        let again = !self.state_idle()
            && self
                .playlist
                .items()
                .get(index)
                .zip(playing.as_deref())
                .is_some_and(|(entry, p)| crate::bookmarks::same_path(entry, Path::new(p)));
        let Some(path) = self.playlist.set_current(index).map(Path::to_path_buf) else {
            return;
        };
        // The entry playing is selected, unless several are (to be moved or
        // removed: the next file coming should not undo that) or one is
        // being dragged (the drop would move the one playing).
        if self.playlist.selection().len() <= 1 && !self.list_state.dragging() {
            self.playlist.select(index);
        }
        self.file_error = None;
        self.osd.clear_at_end();
        self.remember_position();
        let left = self.positions.get(&path);
        let bookmark = self
            .start_at
            .take()
            .filter(|(p, _)| crate::bookmarks::same_path(p, &path))
            .map(|(_, t)| t);
        let start = bookmark.or(left
            .and_then(|l| l.time)
            .filter(|_| self.settings.resume_playback && !again));
        let tracks = left
            .and_then(|l| l.tracks.clone())
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
        if let Some(time) = bookmark {
            self.osd.show(Osd::Bookmark { added: false, time });
        } else if let Some(t) = start {
            let at = crate::format::time(Some(t), false);
            let key = crate::keymap::shortcut(Cmd::Start);
            self.osd.show(Osd::Text(if key.is_empty() {
                tr!(format!("Resumed from {at}"), format!("Продолжение с {at}"))
            } else {
                tr!(
                    format!("Resumed from {at} ({key}: from the beginning)"),
                    format!("Продолжение с {at} ({key} — с начала)")
                )
            }));
        }
    }

    /// Show the window once it is maximized (see `win::show_maximized`).
    fn uncloak(&mut self, ctx: &egui::Context) {
        let Some((until, frame, maximized)) = self.cloak else {
            return;
        };
        // A few frames painted (Windows showed the window white until its
        // first one was), and maximized when it is to be.
        let ready = ctx.cumulative_frame_nr() >= frame + PAINTED_TO_SHOW
            && (!maximized || frameless::is_maximized(ctx));
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

/// A rest of `seconds` as a `Duration`: infinity (no move yet) the longest,
/// NaN or less than zero none.
fn rest_of(seconds: f32) -> Duration {
    Duration::try_from_secs_f32(seconds.max(0.0)).unwrap_or(Duration::MAX)
}

/// Files that came at `at` join those that came last (at `last`): Explorer
/// starts one process per selected file, the first of them this one (its
/// own files count as received at the start).
fn same_batch(last: Option<Instant>, at: Instant) -> bool {
    last.is_some_and(|t| at.saturating_duration_since(t) < SAME_BATCH)
}

/// The font chosen for the OSD line, read from its file; `None` for the
/// interface's font or a file that cannot be used.
fn osd_font(settings: &Settings) -> Option<egui::FontData> {
    let (file, index) = settings.osd_font_file.as_ref()?;
    crate::fonts::read(file, *index)
}

impl eframe::App for App {
    /// Every frame, and alone while the window is minimised: the player's
    /// events keep a playlist going.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.updates.poll();
        self.take_events();
        self.take_hooked();
        if let Some(answer) = self.dialogs.take() {
            self.answer(answer);
        }
        if let Some(why) = self.save_now.take()
            && let Some(storage) = frame.storage_mut()
        {
            log::info!("{why}: saving");
            self.save(storage);
            storage.flush();
        }
        self.follow_audio_device();
        self.follow_quality();
        self.follow_video_map();
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
        win::set_playlist_follow(!self.fullscreen && self.mini.is_none());
        self.follow_on_top(ctx);
        self.show_on_taskbar();
        self.follow_media_keys();
        self.show_in_media_controls();
    }

    fn ui(&mut self, root_ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        self.uncloak(&ctx);
        self.ppp = ctx.pixels_per_point();
        // Whatever chose it (a key, the menu, the settings' page): the
        // interface only the picture gives back.
        if self.settings.interface != Interface::Bare {
            self.before_bare = self.settings.interface;
        }
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
        self.follow_mini(&ctx, &state);
        self.follow_thumbs(&state);
        let mini = self.mini.is_some();
        let mut cmds = std::mem::take(&mut self.pending);

        self.drop_files(dropped_files(&ctx), &state);

        // The keys are the player's: no widget keeps the focus to take
        // Space or Enter as a click. A modal takes the keys while open.
        let keys = !self.video_panel.typing(&ctx)
            && !ui::playlist::typing(&ctx)
            && !self.about.open
            && !self.settings_dialog.open
            && !self.url_dialog.is_open()
            && !self.goto_dialog.is_open()
            && !self.clip_dialog.is_open();
        let pasted = win::take_paste(win::PasteIn::Player);
        if keys && !egui::Popup::is_any_open(&ctx) {
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            // Nor do Tab and the arrows move the focus to a widget this
            // frame: they are the player's keys (Tab the position).
            ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            cmds.extend(input::keys(&ctx, &self.settings.keys));
            if pasted {
                cmds.push(Cmd::Paste);
            }
        }

        if !fullscreen && !maximized {
            frameless::resize_edges(root_ui);
        }
        let title = self.title(&state);
        let interface = self.interface();
        let (top_held, bottom_held) = Self::held(&ctx, self.shown_over);
        self.title_strip = !fullscreen
            && !mini
            && interface.title_over()
            && (top_held || self.wants_title_strip(&ctx));
        let bar_over = !fullscreen
            && !mini
            && interface == Interface::Bare
            && (bottom_held || self.wants_bar_over(&ctx));
        if !fullscreen {
            self.shown_over = (self.title_strip, bar_over);
        }
        let icon = (!fullscreen && !mini && (interface.title_row() || self.title_strip))
            .then(|| ui::app_icon(&ctx, 16.0, &mut self.icon));
        let newer = self.updates.newer().map(str::to_owned);
        if !fullscreen && !mini && interface != Interface::Bare {
            if let Some(icon) = icon.as_ref().filter(|_| interface.title_row()) {
                egui::Panel::top("title")
                    .exact_size(style::TITLE_HEIGHT)
                    .frame(egui::Frame::NONE.fill(style::title_bg()))
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
                    .default_size(ui::playlist::MIN_WIDTH)
                    .size_range(ui::playlist::MIN_WIDTH..=600.0)
                    .frame(egui::Frame::NONE.fill(style::panel_bg()).inner_margin(6))
                    .show(root_ui, |ui| {
                        let logos = self.settings.channel_logos.then_some(&mut self.logos);
                        cmds.extend(ui::playlist::panel(
                            ui,
                            ui::playlist::Panel {
                                playlist: &mut self.playlist,
                                state: &mut self.list_state,
                                positions: &self.positions,
                                logos,
                                repeat: self.settings.repeat,
                                groups: self.settings.playlist_groups,
                                progress: self.settings.playlist_progress,
                                chapters: &state.chapters,
                                time: state.time,
                                duration: state.duration,
                                detached: false,
                            },
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
                if mini {
                    self.mini_controls(ui, ui.max_rect(), &state, &mut cmds);
                }
                if bar_over {
                    let compact = self.compact_bar();
                    let area = ui.max_rect();
                    let rect = Rect::from_min_max(
                        egui::pos2(area.left(), area.bottom() - ui::controls::height(compact)),
                        area.max,
                    );
                    let mut child =
                        ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("bar_over"));
                    let bar = self.controls_bar(&state, false, compact);
                    ui::controls::frame(compact)
                        .fill(style::bar_bg().gamma_multiply_u8(235))
                        .show(&mut child, |ui| {
                            ui.set_min_size(ui.max_rect().size());
                            cmds.extend(ui::controls::bar(ui, bar));
                        });
                }
            })
            .response
            .rect;
        self.picture_area = picture;
        // The chrome of the player's window, not the mini player's.
        if !fullscreen && !mini {
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

        let prefs_before = self.settings.track_prefs.clone();
        // As before the panel changes them.
        let layout_before = (self.settings.sub_ass_override, self.settings.sub_margins);
        let changed = self.settings_dialog.show(
            &ctx,
            &mut self.settings,
            &mut self.updates,
            state.hwdec.as_deref(),
        );
        if changed.keys {
            crate::keymap::publish(&self.settings.keys);
        }
        if changed.look {
            style::set_theme(self.settings.theme);
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
        if self.settings.track_prefs != prefs_before
            && let Some(player) = &mut self.player
        {
            player.set_track_prefs(&self.settings.track_prefs);
        }
        if self.settings_dialog.open {
            win::set_snap(self.settings.snap_to_centre);
            if !self.settings.remember_recent {
                self.settings.recent.clear();
            }
        }
        if changed.frames_folder {
            self.pick_frames_folder(&ctx);
        }
        if changed.add_shaders {
            self.pick_shaders(&ctx);
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
        match self.clip_dialog.show(&ctx, &mut self.settings.clip) {
            Some(ui::clip::Action::Browse(path)) => self.browse_clip(&ctx, path),
            Some(ui::clip::Action::Saved(path)) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.osd.show_always(Osd::Text(tr!(
                    format!("[ Stretch saved: {name} ]"),
                    format!("[ Фрагмент сохранён: {name} ]")
                )));
            }
            Some(ui::clip::Action::Failed(why)) => {
                self.osd.show_always(Osd::Text(format!("[ {why} ]")))
            }
            None => {}
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
        let info_wanted = self.info_wanted();
        if let Some(player) = &mut self.player {
            if info_wanted {
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
        let subject = info_subject(&self.playlist, self.facts.as_ref(), &state);
        let info_cmds = self.info.show(&ctx, &subject, cover, channel);
        cmds.extend(info_cmds);
        self.update_sub_lines(&ctx, &state);
        let before = self.adjust.clone();
        let subs_before = self.settings.sub.clone();
        let output_before = (
            self.settings.audio_channels,
            self.settings.volume_max,
            self.settings.passthrough,
        );
        let voice_boost = self.settings.voice_boost;
        let upmix = self.settings.upmix
            && self.settings.audio_channels.takes_upmix()
            && tracks::selected(&state.tracks, Kind::Audio).is_some_and(crate::audio::stereo_track);
        let geometry = self.navigator_geometry(&state);
        let kept = ui::picture::Kept {
            audio_device: &mut self.settings.audio_device,
            devices: &state.audio_devices,
            channels: &mut self.settings.audio_channels,
            passthrough: &mut self.settings.passthrough,
            volume_max: &mut self.settings.volume_max,
            normalize: &mut self.settings.normalize,
            normalize_strength: &mut self.settings.normalize_strength,
            voice_db: &mut self.settings.voice_db,
            voice_boost: &mut self.settings.voice_boost,
            upmix: &mut self.settings.upmix,
            upmix_lfe: &mut self.settings.upmix_lfe,
            subwoofer: &mut self.settings.subwoofer,
            equalizer: &mut self.settings.equalizer,
            eq_presets: &mut self.settings.eq_presets,
            subs: &mut self.settings.sub,
            sub_presets: &mut self.settings.sub_presets,
            sub_ass_override: &mut self.settings.sub_ass_override,
            sub_margins: &mut self.settings.sub_margins,
            keep_adjust: &mut self.settings.keep_adjust,
            navigator: &mut self.settings.navigator,
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
                upmix,
                time: state.time.filter(|_| !state.idle),
                sub_lines: self.sub_lines.as_ref().and_then(|s| s.lines.as_deref()),
                subs_on: tracks::selected(&state.tracks, Kind::Sub).is_some(),
                subs_external: tracks::selected(&state.tracks, Kind::Sub)
                    .is_some_and(|t| t.external),
                geometry,
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
            if self.settings.passthrough != output_before.2 {
                player.set_passthrough(self.settings.passthrough);
            }
        }
        self.sync_audio_filters(&ctx, &state);

        for cmd in cmds {
            self.run(&ctx, frame, cmd);
        }
        self.fit_on_open(&ctx, frame, &state);
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
            // The interface changed while maximized: the window is made
            // for it once restored, so the rectangle kept is too (the next
            // start takes the interface as the window's).
            let r = if self.sized_for == self.interface() {
                r
            } else {
                self.refit(r, self.sized_for, self.interface(), self.ppp, &mut [0, 0])
            };
            self.settings.window_normal = Some(r);
        }
        self.save_mini(storage);
        self.settings.save(storage);
        self.remember_position();
        storage.set_string(POSITIONS_KEY, self.positions.to_text());
        self.session =
            crate::session::follow(&self.playlist, self.settings.restore_playlist, self.session);
    }

    /// The render context goes first, while the OpenGL context lives, then
    /// the core.
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        crate::instance::closing();
        // A stretch or a map being saved is cancelled, and its thread is
        // given a moment to delete the file half written.
        self.clip_dialog.cancel();
        if let Some((job, _)) = &self.video_map {
            job.cancel_and_wait(std::time::Duration::from_secs(3));
        }
        self.thumbs = None;
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
    fn a_pointer_never_moved_has_rested_for_ever() {
        assert_eq!(rest_of(f32::INFINITY), Duration::MAX);
        assert_eq!(rest_of(f32::NAN), Duration::ZERO);
        assert_eq!(rest_of(-1.0), Duration::ZERO);
        assert_eq!(rest_of(0.5), Duration::from_millis(500));
    }

    #[test]
    fn files_of_other_starts_join_the_first() {
        let start = Instant::now();
        // The start's own files, then another start's 100 ms later.
        assert!(same_batch(Some(start), start + Duration::from_millis(100)));
        assert!(!same_batch(Some(start), start + Duration::from_secs(2)));
        // A double click on another file half a second later plays it.
        assert!(!same_batch(Some(start), start + Duration::from_millis(500)));
        // Nothing of its own: the files replace the playlist.
        assert!(!same_batch(None, start));
    }
}
