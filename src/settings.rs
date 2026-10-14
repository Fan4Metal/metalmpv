//! Settings kept between runs in eframe's storage
//! (`%APPDATA%\MetalMPV\data\app.ron`, or [`PORTABLE_FILE`] next to the exe,
//! see [`location`]). Window size and position and egui's own state (panel
//! widths) are saved by eframe itself (`persist_window`).
//!
//! Plain strings through `eframe::Storage`: no serde derives needed, and an
//! unknown or malformed value falls back to its default, so settings of an
//! older or newer version never make the file unreadable.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::audio::{self, Channels, Passthrough};
use crate::format::TimeMode;
use crate::i18n::LangChoice;
use crate::keymap::Keymap;
use crate::playlist::Repeat;
use crate::sub_style::{self, SubStyle};
use crate::tracks::Likeness;
use crate::ui::style::Accent;

const LANGUAGE: &str = "language";
const CHECK_UPDATES: &str = "check_updates";
const LAST_UPDATE_CHECK: &str = "last_update_check";
const VOLUME: &str = "volume";
const MUTE: &str = "mute";
const SEEK_SHORT: &str = "seek_short";
const SEEK_LONG: &str = "seek_long";
const HWDEC: &str = "hwdec";
const FILL_PLAYLIST: &str = "fill_playlist_from_folder";
const CHANNEL_LOGOS: &str = "channel_logos";
const PLAYLIST_VISIBLE: &str = "playlist_visible";
const CHAPTER_MARKS: &str = "chapter_marks";
const TIME_DISPLAY: &str = "time_display";
const FULLSCREEN_TRANSPARENCY: &str = "fullscreen_transparency";
const FULLSCREEN_TITLE: &str = "fullscreen_title";
const INFO_COLOURS: &str = "info_colours";
/// Keys of settings there are no more, removed when saving: the panels
/// could be unpinned (the information window is always pinned now, the
/// Video, sound and subtitles panel stands over its button).
const DROPPED: [&str; 2] = ["panels_pinned", "info_pinned"];
const SNAP_TO_CENTRE: &str = "snap_to_centre";
const REPEAT: &str = "repeat";
const ON_TOP: &str = "on_top";
const KEYS: &str = "keys";
const ACCENT: &str = "accent";
const PLAYLIST_DETACHED: &str = "playlist_detached";
const PLAYLIST_RECT: &str = "playlist_rect";

/// When the window stays over the other windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnTop {
    #[default]
    Never,
    Always,
    /// While a file plays (not paused, not stopped).
    Playing,
}

impl OnTop {
    pub const ALL: [OnTop; 3] = [OnTop::Never, OnTop::Always, OnTop::Playing];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            OnTop::Never => "never",
            OnTop::Always => "always",
            OnTop::Playing => "playing",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.name() == name)
    }

    /// Never, always, while playing (`Ctrl+T`).
    pub fn next(self) -> Self {
        match self {
            OnTop::Never => OnTop::Always,
            OnTop::Always => OnTop::Playing,
            OnTop::Playing => OnTop::Never,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OnTop::Never => tr!("Never", "Никогда"),
            OnTop::Always => tr!("Always", "Всегда"),
            OnTop::Playing => tr!("While playing", "Во время воспроизведения"),
        }
    }
}
const AUDIO_DEVICE: &str = "audio_device";
const AUDIO_CHANNELS: &str = "audio_channels";
const VOLUME_MAX: &str = "volume_max";
const NORMALIZE: &str = "normalize";
const EQ_ON: &str = "eq_on";
const EQ_GAINS: &str = "eq_gains";
const PASSTHROUGH: &str = "audio_passthrough";
const SUB_FONT: &str = "sub_font";
const SUB_SCALE: &str = "sub_scale";
const SUB_COLOR: &str = "sub_color";
const SUB_OUTLINE_COLOR: &str = "sub_outline_color";
const SUB_OUTLINE: &str = "sub_outline";
const SUB_POS: &str = "sub_pos";
const SUB_BOLD: &str = "sub_bold";
const SUB_SHADOW: &str = "sub_shadow";
const SUB_BOX: &str = "sub_box";
const SUB_PRESETS: &str = "sub_presets";
const SUB_ASS_OVERRIDE: &str = "sub_ass_override";
const SUB_MARGINS: &str = "sub_margins";
const VOICE_BOOST: &str = "voice_boost";
const EQ_PRESETS: &str = "eq_presets";
const KEEP_ADJUST: &str = "keep_adjust";
const ADJUST: &str = "adjust";
const OSD_FONT: &str = "osd_font";
const OSD_FONT_FILE: &str = "osd_font_file";
const OSD_SCALE: &str = "osd_scale";
const WINDOW_NORMAL: &str = "window_normal";
const RESUME_PLAYBACK: &str = "resume_playback";
const REMEMBER_TRACKS: &str = "remember_tracks";
const PREFER_LAST_TRACKS: &str = "prefer_last_tracks";
const STAY_IN_INCOMPLETE: &str = "stay_in_incomplete";
const LAST_AUDIO: &str = "last_audio";
const LAST_SUB: &str = "last_sub";
const RECENT: &str = "recent_files";
const REMEMBER_RECENT: &str = "remember_recent";

/// How many recent files are kept.
pub const RECENT_COUNT: usize = 10;

/// The longest seek step the settings accept, in seconds.
/// The shortest seek step, in whole seconds as the settings show it.
pub const MIN_STEP: f64 = 1.0;
pub const MAX_STEP: f64 = 3600.0;
/// The most transparent the fullscreen panels may be, in percent: beyond it
/// their text is lost on a light picture.
pub const MAX_TRANSPARENCY: f64 = 80.0;
/// The range of the OSD size, in percent.
pub const OSD_SCALE_RANGE: std::ops::RangeInclusive<f64> = 50.0..=200.0;

/// Settings file that makes a copy portable: when it lies next to the exe
/// (even empty) and can be written, the settings are kept in it instead of
/// the user profile. The portable zip ships an empty one
/// (`PORTABLE_SETTINGS` in tools/make_release.py).
pub const PORTABLE_FILE: &str = "metalmpv.ron";

/// Where the settings of this run are kept.
#[derive(Debug)]
pub struct Location {
    /// The settings file, `None` if there is nowhere to keep it.
    pub file: Option<PathBuf>,
    /// `file` is [`PORTABLE_FILE`] next to the exe.
    pub portable: bool,
}

impl Location {
    /// Whether settings of an earlier run are there (a portable file starts
    /// empty). `NativeOptions::centered` is set only when there are none:
    /// eframe would otherwise re-centre a restored window.
    pub fn has_saved(&self) -> bool {
        self.file
            .as_ref()
            .is_some_and(|f| f.metadata().is_ok_and(|m| m.len() > 0))
    }
}

/// Where the settings are kept, decided once per run.
pub fn location() -> &'static Location {
    static LOCATION: OnceLock<Location> = OnceLock::new();
    LOCATION.get_or_init(|| {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf));
        match exe_dir.and_then(|d| portable_file_in(&d)) {
            Some(file) => Location {
                file: Some(file),
                portable: true,
            },
            None => Location {
                file: eframe::storage_dir(crate::APP_ID).map(|d| d.join("app.ron")),
                portable: false,
            },
        }
    })
}

/// [`PORTABLE_FILE`] in `dir` if it is there and can be written; a copy on
/// a read-only medium keeps its settings in the profile instead of losing
/// them.
fn portable_file_in(dir: &Path) -> Option<PathBuf> {
    let file = dir.join(PORTABLE_FILE);
    // Opening for appending checks the permission without changing the file.
    std::fs::OpenOptions::new().append(true).open(&file).ok()?;
    // eframe writes a new file beside it and renames it: the folder must
    // take a file too.
    let probe = dir.join(format!(".metalmpv-{}.tmp", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .ok()?;
    let _ = std::fs::remove_file(&probe);
    Some(file)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Interface language, or the one of Windows.
    pub language: LangChoice,
    /// Ask GitHub for a newer release at start-up (off unless enabled).
    pub check_updates: bool,
    /// Unix seconds of the last update check that got an answer.
    pub last_update_check: u64,
    /// 0 to 100.
    pub volume: f64,
    pub mute: bool,
    /// The step of Left and Right, in seconds (keyframe seek).
    pub seek_short: f64,
    /// The step of Ctrl+Left and Ctrl+Right, in seconds (keyframe seek).
    pub seek_long: f64,
    /// Hardware decoding (`hwdec=auto-safe`), else software.
    pub hwdec: bool,
    /// Opening one file lists the media files of its folder.
    pub fill_playlist: bool,
    /// The pictures of an M3U list's channels are fetched (`logos.rs`).
    pub channel_logos: bool,
    pub playlist_visible: bool,
    /// The playlist is in a window of its own (`dock.rs`).
    pub playlist_detached: bool,
    /// That window's visible rectangle in pixels (left, top, right,
    /// bottom), once it was open.
    pub playlist_rect: Option<[i32; 4]>,
    /// Marks at the chapters' starts on the seek bar.
    pub chapter_marks: bool,
    /// What the time in the control bar shows (a click changes it).
    pub time_mode: TimeMode,
    /// How transparent the panels over the picture in fullscreen are, in
    /// percent (0 to [`MAX_TRANSPARENCY`]).
    pub fullscreen_transparency: f64,
    /// A strip with the file's name at the top in fullscreen.
    pub fullscreen_title: bool,
    /// The chips of the information window in colour: the format as its
    /// file icon, the picture, the sound and the subtitles each in a hue.
    pub info_colours: bool,
    /// The window, being moved, is drawn to the screen's centre lines.
    pub snap_to_centre: bool,
    /// What plays after the end of a file (`R`, the playlist's button).
    pub repeat: Repeat,
    /// When the window stays over the other windows (`Ctrl+T`).
    pub on_top: OnTop,
    /// The keys, as changed on the Keys page (`keymap.rs`).
    pub keys: Keymap,
    /// The interface's accent colour and the icon's screen.
    pub accent: Accent,
    /// The output device as mpv names it; empty for Windows' default.
    pub audio_device: String,
    pub audio_channels: Channels,
    /// The volume's ceiling in percent (one of [`audio::BOOSTS`]).
    pub volume_max: f64,
    /// The volume evened out (`audio::NORMALIZE_FILTER`).
    pub normalize: bool,
    /// Speech made clearer (`audio::voice_filter`).
    pub voice_boost: bool,
    /// The equalizer's gains the user saved; not a setting, Reset keeps
    /// them.
    pub eq_presets: Vec<audio::OwnEq>,
    /// The ten-band equalizer (the Video panel's Sound tab).
    pub equalizer: audio::Equalizer,
    /// Compressed sound sent to a receiver as it is.
    pub passthrough: Passthrough,
    /// How subtitles look (`sub_style.rs`).
    pub sub: SubStyle,
    /// The subtitles' looks the user saved; not a setting, Reset keeps
    /// them.
    pub sub_presets: Vec<sub_style::Own>,
    /// The subtitles' style applies to ASS ones too (`sub-ass-override`).
    pub sub_ass_override: bool,
    /// Subtitles go in the black bars where there are any.
    pub sub_margins: bool,
    /// The picture's adjustments are kept between runs, as `adjust`.
    pub keep_adjust: bool,
    pub adjust: crate::adjust::Adjust,
    /// The font family of the OSD line, empty for the interface's.
    pub osd_font: String,
    /// The file and the face number of `osd_font`.
    pub osd_font_file: Option<(PathBuf, u32)>,
    /// The size of the OSD line, in percent.
    pub osd_scale: f64,
    /// The window's restored rectangle in pixels (left, top, right,
    /// bottom) while it is maximized: eframe saves the maximized size.
    pub window_normal: Option<[i32; 4]>,
    /// A file opened again goes on from where it was left (`resume.rs`).
    pub resume_playback: bool,
    /// A file opened again plays the audio and subtitles it played.
    pub remember_tracks: bool,
    /// A file opened for the first time plays the tracks most like the
    /// last chosen by hand (`tracks::most_like`).
    pub prefer_last_tracks: bool,
    /// Where the data of an incomplete file ends, playback stops there,
    /// paused; off, the file ends and the next one plays.
    pub stay_in_incomplete: bool,
    /// The audio track and the subtitles last chosen by hand (subtitles:
    /// `Some(None)` for none). Not settings, so Reset keeps them.
    pub last_audio: Option<Likeness>,
    pub last_sub: Option<Option<Likeness>>,
    /// The start screen lists the files played last ([`Self::recent`]);
    /// off, none are kept.
    pub remember_recent: bool,
    /// The files played last, the latest first (the start screen). Not a
    /// setting, so Reset keeps them; Forget all clears them.
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LangChoice::System,
            check_updates: false,
            last_update_check: 0,
            volume: 100.0,
            mute: false,
            seek_short: 10.0,
            seek_long: 60.0,
            hwdec: true,
            fill_playlist: false,
            channel_logos: true,
            playlist_visible: false,
            playlist_detached: false,
            playlist_rect: None,
            chapter_marks: true,
            time_mode: TimeMode::Both,
            fullscreen_transparency: 12.0,
            fullscreen_title: true,
            info_colours: true,
            snap_to_centre: true,
            repeat: Repeat::Off,
            on_top: OnTop::Never,
            keys: Keymap::default(),
            accent: Accent::Blue,
            audio_device: String::new(),
            audio_channels: Channels::Auto,
            volume_max: 100.0,
            normalize: false,
            voice_boost: false,
            eq_presets: Vec::new(),
            equalizer: audio::Equalizer::default(),
            passthrough: Passthrough::Off,
            sub: SubStyle::default(),
            sub_presets: Vec::new(),
            sub_ass_override: false,
            sub_margins: true,
            keep_adjust: false,
            adjust: crate::adjust::Adjust::default(),
            osd_font: String::new(),
            osd_font_file: None,
            osd_scale: 100.0,
            window_normal: None,
            resume_playback: true,
            remember_tracks: true,
            prefer_last_tracks: true,
            stay_in_incomplete: true,
            last_audio: None,
            last_sub: None,
            remember_recent: true,
            recent: Vec::new(),
        }
    }
}

/// A rectangle written as `left,top,right,bottom`, not empty.
fn rect(text: &str) -> Option<[i32; 4]> {
    let n: Vec<i32> = text
        .split(',')
        .filter_map(|x| x.trim().parse().ok())
        .collect();
    <[i32; 4]>::try_from(n)
        .ok()
        .filter(|r| r[2] > r[0] && r[3] > r[1])
}

impl Settings {
    /// What the player starts with.
    pub fn audio_setup(&self) -> audio::Setup {
        audio::Setup {
            volume: self.volume,
            mute: self.mute,
            volume_max: self.volume_max,
            channels: self.audio_channels,
            normalize: self.normalize,
            equalizer: self.equalizer.clone(),
            passthrough: self.passthrough,
        }
    }

    /// Read settings; missing or unknown values keep their defaults.
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let d = Self::default();
        let get = |key| storage.get_string(key);
        let flag = |key, default| match get(key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let number = |key, range: std::ops::RangeInclusive<f64>, default| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| range.contains(v))
                .unwrap_or(default)
        };
        // The OSD's family and its file go together: one without the other
        // is the default.
        let osd_font = get(OSD_FONT).filter(|f| !f.is_empty()).and_then(|name| {
            let file = get(OSD_FONT_FILE)?;
            let (index, path) = file.split_once('|')?;
            Some((name, (PathBuf::from(path), index.parse().ok()?)))
        });
        Self {
            language: get(LANGUAGE)
                .as_deref()
                .map_or(d.language, LangChoice::from_name),
            check_updates: flag(CHECK_UPDATES, d.check_updates),
            last_update_check: get(LAST_UPDATE_CHECK)
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.last_update_check),
            volume: number(VOLUME, 0.0..=audio::MAX_VOLUME, d.volume),
            mute: flag(MUTE, d.mute),
            seek_short: number(SEEK_SHORT, MIN_STEP..=MAX_STEP, d.seek_short),
            seek_long: number(SEEK_LONG, MIN_STEP..=MAX_STEP, d.seek_long),
            hwdec: flag(HWDEC, d.hwdec),
            fill_playlist: flag(FILL_PLAYLIST, d.fill_playlist),
            channel_logos: flag(CHANNEL_LOGOS, d.channel_logos),
            playlist_visible: flag(PLAYLIST_VISIBLE, d.playlist_visible),
            playlist_detached: flag(PLAYLIST_DETACHED, d.playlist_detached),
            playlist_rect: get(PLAYLIST_RECT).and_then(|v| rect(&v)),
            chapter_marks: flag(CHAPTER_MARKS, d.chapter_marks),
            time_mode: get(TIME_DISPLAY)
                .as_deref()
                .and_then(TimeMode::from_name)
                .unwrap_or(d.time_mode),
            fullscreen_transparency: number(
                FULLSCREEN_TRANSPARENCY,
                0.0..=MAX_TRANSPARENCY,
                d.fullscreen_transparency,
            ),
            fullscreen_title: flag(FULLSCREEN_TITLE, d.fullscreen_title),
            info_colours: flag(INFO_COLOURS, d.info_colours),
            snap_to_centre: flag(SNAP_TO_CENTRE, d.snap_to_centre),
            repeat: get(REPEAT)
                .as_deref()
                .and_then(Repeat::from_name)
                .unwrap_or(d.repeat),
            on_top: get(ON_TOP)
                .as_deref()
                .and_then(OnTop::from_name)
                .unwrap_or(d.on_top),
            keys: get(KEYS).as_deref().map_or(d.keys, Keymap::from_setting),
            accent: get(ACCENT)
                .as_deref()
                .and_then(Accent::from_name)
                .unwrap_or(d.accent),
            audio_device: get(AUDIO_DEVICE).unwrap_or_default(),
            audio_channels: get(AUDIO_CHANNELS)
                .as_deref()
                .and_then(Channels::from_name)
                .unwrap_or(d.audio_channels),
            volume_max: get(VOLUME_MAX)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| audio::BOOSTS.contains(v))
                .unwrap_or(d.volume_max),
            normalize: flag(NORMALIZE, d.normalize),
            voice_boost: flag(VOICE_BOOST, d.voice_boost),
            eq_presets: get(EQ_PRESETS)
                .map(|v| audio::OwnEq::from_text(&v))
                .unwrap_or_default(),
            equalizer: audio::Equalizer {
                on: flag(EQ_ON, d.equalizer.on),
                gains: get(EQ_GAINS)
                    .and_then(|v| {
                        let gains: Vec<i64> = v
                            .split(',')
                            .filter_map(|g| g.trim().parse().ok())
                            .filter(|g: &i64| g.abs() <= audio::EQ_RANGE)
                            .collect();
                        <[i64; 10]>::try_from(gains).ok()
                    })
                    .unwrap_or(d.equalizer.gains),
            },
            passthrough: get(PASSTHROUGH)
                .as_deref()
                .and_then(Passthrough::from_name)
                .unwrap_or(d.passthrough),
            sub: SubStyle {
                font: get(SUB_FONT).unwrap_or_default(),
                scale: number(SUB_SCALE, sub_style::SCALE_RANGE, d.sub.scale),
                color: get(SUB_COLOR)
                    .as_deref()
                    .and_then(sub_style::parse_hex)
                    .unwrap_or(d.sub.color),
                outline_color: get(SUB_OUTLINE_COLOR)
                    .as_deref()
                    .and_then(sub_style::parse_hex)
                    .unwrap_or(d.sub.outline_color),
                outline: number(SUB_OUTLINE, sub_style::OUTLINE_RANGE, d.sub.outline),
                pos: number(SUB_POS, sub_style::POS_RANGE, d.sub.pos),
                bold: flag(SUB_BOLD, d.sub.bold),
                shadow: number(SUB_SHADOW, sub_style::SHADOW_RANGE, d.sub.shadow),
                boxed: flag(SUB_BOX, d.sub.boxed),
            },
            sub_presets: get(SUB_PRESETS)
                .map(|v| sub_style::Own::from_text(&v))
                .unwrap_or_default(),
            sub_ass_override: flag(SUB_ASS_OVERRIDE, d.sub_ass_override),
            sub_margins: flag(SUB_MARGINS, d.sub_margins),
            keep_adjust: flag(KEEP_ADJUST, d.keep_adjust),
            adjust: get(ADJUST)
                .filter(|_| flag(KEEP_ADJUST, d.keep_adjust))
                .map(|v| crate::adjust::Adjust::from_text(&v))
                .unwrap_or_default(),
            osd_font: osd_font
                .as_ref()
                .map(|(name, _)| name.clone())
                .unwrap_or_default(),
            osd_font_file: osd_font.map(|(_, file)| file),
            osd_scale: number(OSD_SCALE, OSD_SCALE_RANGE, d.osd_scale),
            window_normal: get(WINDOW_NORMAL).and_then(|v| rect(&v)),
            resume_playback: flag(RESUME_PLAYBACK, d.resume_playback),
            remember_tracks: flag(REMEMBER_TRACKS, d.remember_tracks),
            prefer_last_tracks: flag(PREFER_LAST_TRACKS, d.prefer_last_tracks),
            stay_in_incomplete: flag(STAY_IN_INCOMPLETE, d.stay_in_incomplete),
            last_audio: get(LAST_AUDIO).as_deref().and_then(Likeness::parse),
            last_sub: get(LAST_SUB).and_then(|v| match v.as_str() {
                "off" => Some(None),
                v => Likeness::parse(v).map(Some),
            }),
            remember_recent: flag(REMEMBER_RECENT, d.remember_recent),
            recent: get(RECENT)
                .map(|v| {
                    v.lines()
                        .filter(|l| !l.trim().is_empty())
                        .take(RECENT_COUNT)
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// `path` played: first in the recent files, if they are kept.
    pub fn played(&mut self, path: &Path) {
        if !self.remember_recent {
            return;
        }
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(RECENT_COUNT);
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(LANGUAGE, self.language.name().into());
        storage.set_string(CHECK_UPDATES, self.check_updates.to_string());
        storage.set_string(LAST_UPDATE_CHECK, self.last_update_check.to_string());
        storage.set_string(VOLUME, format!("{:.0}", self.volume));
        storage.set_string(MUTE, self.mute.to_string());
        storage.set_string(SEEK_SHORT, self.seek_short.to_string());
        storage.set_string(SEEK_LONG, self.seek_long.to_string());
        storage.set_string(HWDEC, self.hwdec.to_string());
        storage.set_string(FILL_PLAYLIST, self.fill_playlist.to_string());
        storage.set_string(CHANNEL_LOGOS, self.channel_logos.to_string());
        storage.set_string(PLAYLIST_VISIBLE, self.playlist_visible.to_string());
        storage.set_string(PLAYLIST_DETACHED, self.playlist_detached.to_string());
        match self.playlist_rect {
            Some(r) => storage.set_string(
                PLAYLIST_RECT,
                format!("{},{},{},{}", r[0], r[1], r[2], r[3]),
            ),
            None => storage.remove_string(PLAYLIST_RECT),
        }
        storage.set_string(CHAPTER_MARKS, self.chapter_marks.to_string());
        storage.set_string(TIME_DISPLAY, self.time_mode.name().into());
        storage.set_string(
            FULLSCREEN_TRANSPARENCY,
            format!("{:.0}", self.fullscreen_transparency),
        );
        storage.set_string(FULLSCREEN_TITLE, self.fullscreen_title.to_string());
        storage.set_string(INFO_COLOURS, self.info_colours.to_string());
        for key in DROPPED {
            storage.remove_string(key);
        }
        storage.set_string(SNAP_TO_CENTRE, self.snap_to_centre.to_string());
        storage.set_string(REPEAT, self.repeat.name().into());
        storage.set_string(ON_TOP, self.on_top.name().into());
        storage.set_string(KEYS, self.keys.to_setting());
        storage.set_string(ACCENT, self.accent.name());
        storage.set_string(AUDIO_DEVICE, self.audio_device.clone());
        storage.set_string(AUDIO_CHANNELS, self.audio_channels.name().into());
        storage.set_string(VOLUME_MAX, format!("{:.0}", self.volume_max));
        storage.set_string(NORMALIZE, self.normalize.to_string());
        storage.set_string(VOICE_BOOST, self.voice_boost.to_string());
        storage.set_string(EQ_PRESETS, audio::OwnEq::to_text(&self.eq_presets));
        storage.set_string(SUB_ASS_OVERRIDE, self.sub_ass_override.to_string());
        storage.set_string(SUB_MARGINS, self.sub_margins.to_string());
        storage.set_string(KEEP_ADJUST, self.keep_adjust.to_string());
        let adjust = if self.keep_adjust {
            self.adjust.to_text()
        } else {
            String::new()
        };
        storage.set_string(ADJUST, adjust);
        storage.set_string(EQ_ON, self.equalizer.on.to_string());
        let gains: Vec<String> = self.equalizer.gains.iter().map(i64::to_string).collect();
        storage.set_string(EQ_GAINS, gains.join(","));
        storage.set_string(PASSTHROUGH, self.passthrough.name().into());
        storage.set_string(SUB_FONT, self.sub.font.clone());
        storage.set_string(SUB_SCALE, format!("{:.0}", self.sub.scale));
        storage.set_string(SUB_COLOR, sub_style::hex(self.sub.color));
        storage.set_string(SUB_OUTLINE_COLOR, sub_style::hex(self.sub.outline_color));
        storage.set_string(SUB_OUTLINE, format!("{:.2}", self.sub.outline));
        storage.set_string(SUB_POS, format!("{:.0}", self.sub.pos));
        storage.set_string(SUB_BOLD, self.sub.bold.to_string());
        storage.set_string(SUB_SHADOW, format!("{:.2}", self.sub.shadow));
        storage.set_string(SUB_BOX, self.sub.boxed.to_string());
        storage.set_string(SUB_PRESETS, sub_style::Own::to_text(&self.sub_presets));
        match &self.osd_font_file {
            Some((path, index)) if !self.osd_font.is_empty() => {
                storage.set_string(OSD_FONT, self.osd_font.clone());
                storage.set_string(OSD_FONT_FILE, format!("{index}|{}", path.display()));
            }
            _ => {
                storage.remove_string(OSD_FONT);
                storage.remove_string(OSD_FONT_FILE);
            }
        }
        storage.set_string(OSD_SCALE, format!("{:.0}", self.osd_scale));
        match self.window_normal {
            Some(r) => storage.set_string(
                WINDOW_NORMAL,
                format!("{},{},{},{}", r[0], r[1], r[2], r[3]),
            ),
            None => storage.remove_string(WINDOW_NORMAL),
        }
        storage.set_string(RESUME_PLAYBACK, self.resume_playback.to_string());
        storage.set_string(REMEMBER_TRACKS, self.remember_tracks.to_string());
        storage.set_string(PREFER_LAST_TRACKS, self.prefer_last_tracks.to_string());
        storage.set_string(STAY_IN_INCOMPLETE, self.stay_in_incomplete.to_string());
        match &self.last_audio {
            Some(like) => storage.set_string(LAST_AUDIO, like.to_text()),
            None => storage.remove_string(LAST_AUDIO),
        }
        match &self.last_sub {
            Some(Some(like)) => storage.set_string(LAST_SUB, like.to_text()),
            Some(None) => storage.set_string(LAST_SUB, "off".into()),
            None => storage.remove_string(LAST_SUB),
        }
        storage.set_string(REMEMBER_RECENT, self.remember_recent.to_string());
        let recent: Vec<String> = self
            .recent
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        storage.set_string(RECENT, recent.join("\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemStorage(HashMap<String, String>);

    impl Storage for MemStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn round_trip() {
        let s = Settings {
            language: LangChoice::Ru,
            check_updates: true,
            last_update_check: 1_790_000_000,
            volume: 35.0,
            mute: true,
            seek_short: 5.0,
            seek_long: 30.5,
            hwdec: false,
            fill_playlist: true,
            channel_logos: false,
            playlist_visible: true,
            playlist_detached: true,
            playlist_rect: Some([-1200, 40, -900, 700]),
            chapter_marks: false,
            time_mode: TimeMode::Remaining,
            fullscreen_transparency: 40.0,
            fullscreen_title: false,
            info_colours: false,
            snap_to_centre: false,
            repeat: Repeat::All,
            on_top: OnTop::Playing,
            keys: Keymap::from_setting("pause\tF\nstop\tSpace Ctrl+MouseBack\nmute\t"),
            accent: Accent::Custom(0x12ab9f),
            audio_device: "wasapi/{0.0.0.00000000}.{abc}".into(),
            audio_channels: Channels::Surround51,
            volume_max: 150.0,
            normalize: true,
            voice_boost: true,
            eq_presets: vec![audio::OwnEq {
                name: "Ночь".into(),
                gains: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            }],
            equalizer: audio::Equalizer {
                on: true,
                gains: [5, 4, 3, 1, -1, -1, 1, 3, 4, -12],
            },
            passthrough: Passthrough::Hdmi,
            sub: SubStyle {
                font: "Arial".into(),
                scale: 150.0,
                color: [255, 230, 0],
                outline_color: [20, 20, 60],
                outline: 2.5,
                pos: 90.0,
                bold: true,
                shadow: 1.5,
                boxed: true,
            },
            sub_ass_override: true,
            sub_margins: false,
            keep_adjust: true,
            adjust: crate::adjust::Adjust {
                zoom: 150,
                denoise: 30,
                ..crate::adjust::Adjust::default()
            },
            sub_presets: vec![sub_style::Own {
                name: "Вечер".into(),
                style: SubStyle {
                    scale: 120.0,
                    ..SubStyle::default()
                },
            }],
            osd_font: "Segoe UI".into(),
            osd_font_file: Some((PathBuf::from(r"C:\Windows\Fonts\segoeui.ttf"), 0)),
            osd_scale: 80.0,
            window_normal: Some([10, 20, 1010, 720]),
            resume_playback: false,
            remember_tracks: false,
            prefer_last_tracks: false,
            stay_in_incomplete: false,
            last_audio: Some(Likeness {
                lang: Some("russian".into()),
                title: Some("многоголосый".into()),
            }),
            last_sub: Some(None),
            remember_recent: false,
            recent: vec![PathBuf::from(r"D:\Films\a.mkv"), PathBuf::from(r"E:\b.mp4")],
        };
        let mut storage = MemStorage::default();
        s.save(&mut storage);
        assert_eq!(Settings::load(&storage), s);
    }

    #[test]
    fn missing_or_unknown_values_use_defaults() {
        assert_eq!(Settings::load(&MemStorage::default()), Settings::default());
        let mut storage = MemStorage::default();
        storage.set_string(LANGUAGE, "de".into());
        storage.set_string(CHECK_UPDATES, "yes".into());
        storage.set_string(LAST_UPDATE_CHECK, "soon".into());
        storage.set_string(VOLUME, "250".into());
        storage.set_string(VOLUME_MAX, "120".into());
        storage.set_string(AUDIO_CHANNELS, "9.1".into());
        storage.set_string(SEEK_SHORT, "0".into());
        storage.set_string(SEEK_LONG, "NaN".into());
        storage.set_string(HWDEC, "maybe".into());
        storage.set_string(TIME_DISPLAY, "total".into());
        storage.set_string(FULLSCREEN_TRANSPARENCY, "95".into());
        storage.set_string(SUB_SCALE, "10".into());
        storage.set_string(OSD_FONT, "Segoe UI".into());
        storage.set_string(OSD_FONT_FILE, "x|C:/f.ttf".into());
        storage.set_string(WINDOW_NORMAL, "1,2,3".into());
        assert_eq!(Settings::load(&storage), Settings::default());
    }

    #[test]
    fn dropped_keys_are_removed() {
        let mut storage = MemStorage::default();
        for key in DROPPED {
            storage.set_string(key, "false".into());
        }
        Settings::load(&storage).save(&mut storage);
        for key in DROPPED {
            assert_eq!(storage.get_string(key), None);
        }
    }

    #[test]
    fn portable_file_only_when_present_and_writable() {
        let dir = std::env::temp_dir().join(format!("portable_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(PORTABLE_FILE);
        assert_eq!(portable_file_in(&dir), None);
        std::fs::write(&file, "").unwrap();
        assert_eq!(portable_file_in(&dir), Some(file.clone()));
        let mut perm = std::fs::metadata(&file).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&file, perm.clone()).unwrap();
        assert_eq!(portable_file_in(&dir), None);
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(&file, perm).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
