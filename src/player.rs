//! The player: an mpv core configured for playback in the app's window, the
//! state the interface shows (kept up to date from observed properties) and
//! the commands of the interface. No OpenGL here: the picture is
//! `video.rs`'s.
//!
//! Every command is asynchronous (see `mpv/mod.rs`): the UI thread never
//! waits for the core, and the core works through the commands in the order
//! they were given, so a volume set before `loadfile` applies to that file.

use std::path::Path;

use crate::adjust::{Adjust, Color};
use crate::audio::{self, Channels, Device, Passthrough};
use crate::chapters::{self, Chapter};
use crate::incomplete;
use crate::mpv::{EndReason, Event, Mpv, Value, sys};
use crate::resume::Tracks;
use crate::sub_style::SubStyle;
use crate::tracks::{self, Kind, Track};

/// Options of the core, set before it starts. The picture goes through the
/// render API (`vo=libmpv`); mpv's own interface, key bindings, scripts and
/// configuration files are off, the app has its own.
pub fn options<'a>(hwdec: bool) -> Vec<(&'a str, &'a str)> {
    vec![
        ("vo", "libmpv"),
        ("hwdec", hwdec_option(hwdec)),
        // The last frame stays on screen at the end; the playlist
        // moves on when `eof-reached` turns true.
        ("keep-open", "yes"),
        ("idle", "yes"),
        // Rendering on the UI thread must not wait for a frame's display
        // time (render.h, MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME).
        ("video-timing-offset", "0"),
        // Audio files without a cover-art video stream: they open faster
        // and do not reconfigure the renderer.
        ("audio-display", "no"),
        // As `adjust::Deinterlace`'s default: frames marked interlaced.
        ("deinterlace", "auto"),
        ("input-default-bindings", "no"),
        ("input-vo-keyboard", "no"),
        ("osc", "no"),
        ("osd-level", "0"),
        ("config", "no"),
        ("terminal", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("load-stats-overlay", "no"),
        ("load-osd-console", "no"),
        ("load-auto-profiles", "no"),
        ("load-select", "no"),
        ("load-console", "no"),
        ("load-commands", "no"),
        ("load-positioning", "no"),
        ("audio-client-name", crate::APP_NAME),
        ("volume-max", "100"),
        ("msg-level", "all=warn"),
    ]
}

/// The `hwdec` option for the setting; `METALMPV_HWDEC` overrides it
/// for measurements ("nvdec", "d3d11va-copy", "no", …).
fn hwdec_option(on: bool) -> &'static str {
    hwdec_value(on, false)
}

/// The `hwdec` option; `copy` while there are filters on the processor:
/// mpv does not download frames for them (`adjust.rs`).
fn hwdec_value(on: bool, copy: bool) -> &'static str {
    use std::sync::OnceLock;
    static FORCED: OnceLock<Option<&'static str>> = OnceLock::new();
    let forced = FORCED.get_or_init(|| {
        std::env::var("METALMPV_HWDEC")
            .ok()
            .filter(|v| !v.is_empty())
            .map(|v| &*Box::leak(v.into_boxed_str()))
    });
    forced.unwrap_or(match (on, copy) {
        (false, _) => "no",
        (true, false) => "auto-safe",
        (true, true) => "auto-copy-safe",
    })
}

/// Start an mpv core with [`options`]. Slow enough (loading the DLL, the
/// core's threads, audio) to be run on a thread of its own while the window
/// is being made.
pub fn boot(hwdec: bool) -> Result<Mpv, EngineError> {
    let api = sys::api().map_err(|e| match e {
        sys::LoadError::NotFound { .. } => EngineError::NoDll(e.to_string()),
        sys::LoadError::Unusable(reason) => EngineError::BadDll(reason.clone()),
    })?;
    let mut options = options(hwdec);
    // METALMPV_MPV="name=value;name=value": more options, for
    // measurements and diagnostics; later ones win.
    let extra = std::env::var("METALMPV_MPV").unwrap_or_default();
    let extra: Vec<(String, String)> = extra
        .split(';')
        .filter_map(|o| o.split_once('='))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    options.extend(extra.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    let mpv = Mpv::new(api, &options).map_err(EngineError::Failed)?;
    Ok(mpv)
}

/// Why nothing can be played: each with its technical detail (English,
/// for the log) and a message for the window ([`EngineError::message`]).
#[derive(Clone, Debug, PartialEq)]
pub enum EngineError {
    /// No libmpv-2.dll in the program's folder.
    NoDll(String),
    /// A libmpv-2.dll that cannot be used (another build, a damaged file).
    BadDll(String),
    /// The core or its video output did not start, or the core stopped.
    Failed(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NoDll(detail)
            | EngineError::BadDll(detail)
            | EngineError::Failed(detail) => f.write_str(detail),
        }
    }
}

impl EngineError {
    /// What the window says: what happened and what puts it right; the
    /// paths looked in stay in the log.
    pub fn message(&self) -> String {
        match self {
            EngineError::NoDll(_) => tr!(
                "The playback engine, libmpv-2.dll, was not found.\nIt belongs in the same folder as metalmpv.exe; reinstalling the program or unpacking the archive again restores it.",
                "Не найден движок воспроизведения libmpv-2.dll.\nФайл должен находиться в одной папке с metalmpv.exe; переустановка программы или повторная распаковка архива восстановит его."
            )
            .to_owned(),
            // The reason (a path, a Windows error, a version) is the log's.
            EngineError::BadDll(_) => tr!(
                "The playback engine, libmpv-2.dll, is damaged or does not suit this version of the program.\nReinstalling the program or unpacking the archive again restores the right one.",
                "Движок воспроизведения libmpv-2.dll повреждён или не подходит к этой версии программы.\nПереустановка программы или повторная распаковка архива восстановит нужный файл."
            )
            .to_owned(),
            EngineError::Failed(detail) => tr!(
                format!("The playback engine did not work: {detail}."),
                format!("Движок воспроизведения не работает: {detail}.")
            ),
        }
    }
}

/// Observed properties; their number in [`Event::Property`] is their
/// place in [`PROPS`] from 1 ([`Prop::of`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prop {
    Pause,
    TimePos,
    Duration,
    Volume,
    Mute,
    MediaTitle,
    Path,
    EofReached,
    Seeking,
    PausedForCache,
    IdleActive,
    Width,
    Height,
    Hwdec,
    TrackList,
    ChapterList,
    Metadata,
    FileSize,
    FileFormat,
    VideoParams,
    AudioDevices,
    LoopA,
    LoopB,
}

const PROPS: [(Prop, &str, sys::mpv_format); 23] = [
    (Prop::Pause, "pause", sys::MPV_FORMAT_FLAG),
    (Prop::TimePos, "time-pos", sys::MPV_FORMAT_DOUBLE),
    (Prop::Duration, "duration", sys::MPV_FORMAT_DOUBLE),
    (Prop::Volume, "volume", sys::MPV_FORMAT_DOUBLE),
    (Prop::Mute, "mute", sys::MPV_FORMAT_FLAG),
    (Prop::MediaTitle, "media-title", sys::MPV_FORMAT_STRING),
    (Prop::Path, "path", sys::MPV_FORMAT_STRING),
    (Prop::EofReached, "eof-reached", sys::MPV_FORMAT_FLAG),
    (Prop::Seeking, "seeking", sys::MPV_FORMAT_FLAG),
    (
        Prop::PausedForCache,
        "paused-for-cache",
        sys::MPV_FORMAT_FLAG,
    ),
    (Prop::IdleActive, "idle-active", sys::MPV_FORMAT_FLAG),
    (Prop::Width, "dwidth", sys::MPV_FORMAT_INT64),
    (Prop::Height, "dheight", sys::MPV_FORMAT_INT64),
    (Prop::Hwdec, "hwdec-current", sys::MPV_FORMAT_STRING),
    (Prop::TrackList, "track-list", sys::MPV_FORMAT_NODE),
    (Prop::ChapterList, "chapter-list", sys::MPV_FORMAT_NODE),
    // For the file's information (changed only when a file is loaded).
    (Prop::Metadata, "metadata", sys::MPV_FORMAT_NODE),
    (Prop::FileSize, "file-size", sys::MPV_FORMAT_INT64),
    (Prop::FileFormat, "file-format", sys::MPV_FORMAT_STRING),
    (Prop::VideoParams, "video-params", sys::MPV_FORMAT_NODE),
    // The output devices; mpv tells again when Windows' devices change.
    (
        Prop::AudioDevices,
        "audio-device-list",
        sys::MPV_FORMAT_NODE,
    ),
    // A-B repeat: a time, or "no" (a node gives either).
    (Prop::LoopA, "ab-loop-a", sys::MPV_FORMAT_NODE),
    (Prop::LoopB, "ab-loop-b", sys::MPV_FORMAT_NODE),
];

impl Prop {
    /// The property of an observed number.
    fn of(id: u64) -> Option<Prop> {
        let i = usize::try_from(id.checked_sub(1)?).ok()?;
        PROPS.get(i).map(|(prop, _, _)| *prop)
    }
}

/// Observe all of [`PROPS`], each by its number.
fn observe_all(mpv: &Mpv) {
    for (i, (_, name, format)) in PROPS.iter().enumerate() {
        mpv.observe(i as u64 + 1, name, *format);
    }
}

/// The reply to [`Player::ask_scan`]'s `video-frame-info`, past the
/// observed properties' numbers.
const FRAME_INFO: u64 = 100;
/// The reply to [`Player::load`]'s `loadfile`.
const LOAD: u64 = 101;
/// mpv's `playlist`, asked for when a file turned out to be a list.
const PLAYLIST: u64 = 102;
/// The reply to [`Player::save_frame`]'s `screenshot-to-file`.
const SCREENSHOT: u64 = 103;
/// The reply to [`Player::sub_step`]'s `sub-step`, and the delay it made.
const SUB_STEP: u64 = 104;
const SUB_DELAY: u64 = 105;
/// The figures of the playback, asked for while the information window is
/// open ([`Player::ask_live`]): the replies' numbers from [`LIVE_FIRST`].
const LIVE_FIRST: u64 = 110;
const LIVE: [(&str, sys::mpv_format); 10] = [
    ("video-bitrate", sys::MPV_FORMAT_DOUBLE),
    ("audio-bitrate", sys::MPV_FORMAT_DOUBLE),
    ("frame-drop-count", sys::MPV_FORMAT_INT64),
    ("decoder-frame-drop-count", sys::MPV_FORMAT_INT64),
    ("estimated-vf-fps", sys::MPV_FORMAT_DOUBLE),
    ("display-fps", sys::MPV_FORMAT_DOUBLE),
    ("audio-out-params", sys::MPV_FORMAT_NODE),
    ("demuxer-cache-duration", sys::MPV_FORMAT_DOUBLE),
    ("cache-speed", sys::MPV_FORMAT_INT64),
    ("demuxer-via-network", sys::MPV_FORMAT_FLAG),
];
/// How often the figures are asked for.
const LIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(1);
/// The seconds of bitrate the information window draws.
pub const HISTORY: usize = 60;

/// The entries of mpv's `playlist` (files or addresses with the titles
/// the list gave them) and the one it plays.
fn playlist_of(value: &Value) -> (Vec<(String, Option<String>)>, usize) {
    let mut playing = 0;
    let entries = value
        .list()
        .iter()
        .filter_map(|entry| {
            let file = entry.get("filename")?.string()?.to_owned();
            let title = entry
                .get("title")
                .and_then(Value::string)
                .map(str::to_owned);
            Some((entry, file, title))
        })
        .enumerate()
        .map(|(i, (entry, file, title))| {
            let current = |key| entry.get(key).is_some_and(Value::flag);
            if current("current") || current("playing") {
                playing = i;
            }
            (file, title)
        })
        .collect();
    (entries, playing)
}

/// The scan of a `video-frame-info` value.
fn scan_of(value: &Value) -> Option<Scan> {
    let interlaced = value.get("interlaced")?.flag();
    Some(if interlaced {
        Scan::Interlaced {
            top_first: value.get("tff").is_some_and(Value::flag),
        }
    } else {
        Scan::Progressive
    })
}

/// What the interface shows of the playback.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub paused: bool,
    /// Position in seconds, `None` without a file.
    pub time: Option<f64>,
    pub duration: Option<f64>,
    /// 0 to `volume_max`.
    pub volume: f64,
    /// The volume's ceiling in percent: above 100 it is boosted.
    pub volume_max: f64,
    /// The sound against the picture, in seconds (later when positive).
    pub audio_delay: f64,
    /// The subtitles against the picture, in seconds (later when positive).
    pub sub_delay: f64,
    /// The A-B repeat's points in seconds (mpv's `ab-loop-a`, `-b`); it
    /// repeats while both are set.
    pub loop_a: Option<f64>,
    pub loop_b: Option<f64>,
    /// The playback speed (1: normal).
    pub speed: f64,
    /// The output devices there are.
    pub audio_devices: Vec<Device>,
    pub mute: bool,
    pub title: Option<String>,
    pub path: Option<String>,
    /// Stopped at the end of the file (`keep-open`).
    pub eof: bool,
    pub seeking: bool,
    pub buffering: bool,
    /// No file is loaded.
    pub idle: bool,
    /// The picture's display width and height; `None` for audio.
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// The hardware decoder in use ("d3d11va-copy"), `None` for software.
    pub hwdec: Option<String>,
    /// The file's video, audio and subtitle tracks.
    pub tracks: Vec<Track>,
    /// The file's chapters, by start.
    pub chapters: Vec<Chapter>,
    /// The file's tags, in the file's order, as mpv names them.
    pub metadata: Vec<(String, String)>,
    pub file_size: Option<i64>,
    /// FFmpeg's container name ("matroska,webm", "mov,mp4,m4a,3gp,3g2,mj2").
    pub file_format: Option<String>,
    /// The decoded picture's format.
    pub video_params: Option<VideoParams>,
    /// Asked for while the information window is open
    /// ([`Player::ask_scan`]): the frame changes every moment.
    pub scan: Option<Scan>,
    /// The frame on screen has HDR10+ metadata (asked with the scan).
    pub hdr10_plus: bool,
    /// The playback's figures now, asked for while the information window
    /// is open ([`Player::ask_live`]).
    pub live: Live,
}

/// How the playback goes, from mpv's figures of the moment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    /// Bits per second over the last packets (between two keyframes).
    pub video_bitrate: Option<f64>,
    pub audio_bitrate: Option<f64>,
    /// The bitrate of the last [`HISTORY`] seconds, a sample a second, the
    /// latest last: the video's, else the sound's. Not while paused.
    pub history: Vec<f32>,
    /// Frames dropped by the video output and by the decoder.
    pub dropped: Option<i64>,
    pub decoder_dropped: Option<i64>,
    /// The frame rate after the filters, as measured.
    pub fps: Option<f64>,
    /// The screen's refresh rate.
    pub display_fps: Option<f64>,
    /// What goes to the sound device.
    pub audio_out: Option<AudioOut>,
    /// Seconds of data read ahead, and how fast it is read (bytes per
    /// second).
    pub cache: Option<f64>,
    pub cache_speed: Option<i64>,
    /// The file comes over the network.
    pub network: bool,
}

/// What mpv gives the sound device (`audio-out-params`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioOut {
    /// mpv's sample format: `floatp`, `s16`; `spdif-eac3` passes the
    /// compressed sound through.
    pub format: Option<String>,
    pub samplerate: Option<i64>,
    /// `5.1`, `stereo`.
    pub channels: Option<String>,
    pub channel_count: Option<i64>,
}

impl Live {
    fn apply(&mut self, index: usize, value: &Value) {
        match index {
            0 => self.video_bitrate = value.double().filter(|b| *b > 0.0),
            1 => self.audio_bitrate = value.double().filter(|b| *b > 0.0),
            2 => self.dropped = value.int(),
            3 => self.decoder_dropped = value.int(),
            4 => self.fps = value.double().filter(|f| *f > 0.0),
            5 => self.display_fps = value.double().filter(|f| *f > 0.0),
            6 => {
                let text = |key| value.get(key).and_then(Value::string).map(str::to_owned);
                self.audio_out = (*value != Value::None).then(|| AudioOut {
                    format: text("format"),
                    samplerate: value.get("samplerate").and_then(Value::int),
                    channels: text("channels"),
                    channel_count: value.get("channel-count").and_then(Value::int),
                });
            }
            7 => self.cache = value.double().filter(|c| *c >= 0.0),
            8 => self.cache_speed = value.int(),
            9 => self.network = value.flag(),
            _ => {}
        }
    }

    /// Note the bitrate of this second.
    fn sample(&mut self) {
        let Some(b) = self.video_bitrate.or(self.audio_bitrate) else {
            return;
        };
        if self.history.len() >= HISTORY {
            self.history.remove(0);
        }
        self.history.push(b as f32);
    }
}

/// What mpv says of the decoded picture (`video-params`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoParams {
    /// `yuv420p10`.
    pub pixel_format: Option<String>,
    /// `bt.2020`.
    pub primaries: Option<String>,
    /// The transfer function: `pq` and `hlg` are HDR.
    pub gamma: Option<String>,
    /// The YUV matrix: `bt.709`, `bt.2020-ncl`.
    pub matrix: Option<String>,
    /// `limited` or `full`.
    pub levels: Option<String>,
    /// The display aspect ratio.
    pub aspect: Option<f64>,
    /// The HDR metadata in cd/m²: content light levels and the mastering
    /// display's peak.
    pub max_cll: Option<f64>,
    pub max_fall: Option<f64>,
    pub max_luma: Option<f64>,
    /// The mastering display's black level in cd/m², and its gamut when
    /// it is a known one ("Display P3").
    pub min_luma: Option<f64>,
    pub mastering: Option<&'static str>,
}

impl VideoParams {
    fn from_value(value: &Value) -> Option<Self> {
        let text = |key| {
            value
                .get(key)
                .and_then(Value::string)
                .filter(|s| !s.is_empty() && *s != "auto")
                .map(str::to_owned)
        };
        let positive = |key| value.get(key).and_then(Value::double).filter(|v| *v > 0.0);
        let params = Self {
            pixel_format: text("hw-pixelformat").or_else(|| text("pixelformat")),
            primaries: text("primaries"),
            gamma: text("gamma"),
            matrix: text("colormatrix"),
            levels: text("colorlevels"),
            aspect: positive("aspect"),
            max_cll: positive("max-cll"),
            max_fall: positive("max-fall"),
            max_luma: positive("max-luma"),
            min_luma: positive("min-luma"),
            mastering: gamut_of(value),
        };
        (params != Self::default()).then_some(params)
    }
}

/// The known gamut of the mastering display `video-params` names by its
/// primaries (`prim-red-x` …), within 0.005.
fn gamut_of(value: &Value) -> Option<&'static str> {
    const GAMUTS: [(&str, [f64; 6]); 3] = [
        ("BT.2020", [0.708, 0.292, 0.170, 0.797, 0.131, 0.046]),
        ("Display P3", [0.680, 0.320, 0.265, 0.690, 0.150, 0.060]),
        ("BT.709", [0.640, 0.330, 0.300, 0.600, 0.150, 0.060]),
    ];
    let keys = [
        "prim-red-x",
        "prim-red-y",
        "prim-green-x",
        "prim-green-y",
        "prim-blue-x",
        "prim-blue-y",
    ];
    let mut got = [0.0; 6];
    for (g, key) in got.iter_mut().zip(keys) {
        *g = value.get(key)?.double()?;
    }
    GAMUTS
        .iter()
        .find(|(_, p)| p.iter().zip(got).all(|(a, b)| (a - b).abs() < 0.005))
        .map(|(name, _)| *name)
}

/// How the picture is scanned, from the frame on screen (`video-frame-info`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scan {
    Progressive,
    /// Interlaced; whether the top field comes first.
    Interlaced {
        top_first: bool,
    },
}

impl Default for State {
    fn default() -> Self {
        Self {
            paused: false,
            time: None,
            duration: None,
            volume: 100.0,
            volume_max: 100.0,
            audio_delay: 0.0,
            speed: 1.0,
            sub_delay: 0.0,
            audio_devices: Vec::new(),
            mute: false,
            title: None,
            path: None,
            eof: false,
            seeking: false,
            buffering: false,
            idle: true,
            width: None,
            height: None,
            hwdec: None,
            tracks: Vec::new(),
            chapters: Vec::new(),
            metadata: Vec::new(),
            file_size: None,
            file_format: None,
            video_params: None,
            scan: None,
            hdr10_plus: false,
            live: Live::default(),
            loop_a: None,
            loop_b: None,
        }
    }
}

impl State {
    /// The position as a share of the duration, 0 to 1.
    pub fn fraction(&self) -> Option<f64> {
        match (self.time, self.duration) {
            (Some(t), Some(d)) if d > 0.0 => Some((t / d).clamp(0.0, 1.0)),
            _ => None,
        }
    }

    fn apply(&mut self, prop: Prop, value: &Value) {
        match prop {
            Prop::Pause => self.paused = value.flag(),
            Prop::TimePos => self.time = value.double(),
            Prop::Duration => self.duration = value.double().filter(|d| *d > 0.0),
            Prop::Volume => self.volume = value.double().unwrap_or(self.volume),
            Prop::Mute => self.mute = value.flag(),
            Prop::MediaTitle => self.title = value.string().map(str::to_owned),
            Prop::Path => self.path = value.string().map(str::to_owned),
            Prop::EofReached => self.eof = value.flag(),
            Prop::Seeking => self.seeking = value.flag(),
            Prop::PausedForCache => self.buffering = value.flag(),
            Prop::IdleActive => self.idle = value.flag(),
            Prop::Width => self.width = value.int().filter(|w| *w > 0),
            Prop::Height => self.height = value.int().filter(|h| *h > 0),
            Prop::Hwdec => {
                self.hwdec = value
                    .string()
                    .filter(|s| !s.is_empty() && *s != "no")
                    .map(str::to_owned)
            }
            Prop::TrackList => self.tracks = tracks::from_value(value),
            Prop::ChapterList => self.chapters = chapters::from_value(value),
            Prop::Metadata => {
                self.metadata = match value {
                    Value::Map(entries) => entries
                        .iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.string()?.trim().to_owned())))
                        .filter(|(_, v)| !v.is_empty())
                        .collect(),
                    _ => Vec::new(),
                }
            }
            Prop::FileSize => self.file_size = value.int().filter(|s| *s >= 0),
            Prop::FileFormat => {
                self.file_format = value.string().filter(|s| !s.is_empty()).map(str::to_owned)
            }
            Prop::VideoParams => self.video_params = VideoParams::from_value(value),
            Prop::AudioDevices => self.audio_devices = audio::devices_from_value(value),
            Prop::LoopA => self.loop_a = value.double().filter(|t| t.is_finite()),
            Prop::LoopB => self.loop_b = value.double().filter(|t| t.is_finite()),
        }
    }

    /// The picture's display size, if there is a picture to show.
    pub fn video(&self) -> Option<(i64, i64)> {
        if self.idle {
            None
        } else {
            Some((self.width?, self.height?))
        }
    }
}

/// What happened since the previous [`Player::poll`], for the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Happened {
    FileLoaded,
    /// The end of the file was reached (the last frame stays on screen).
    Eof,
    /// The file's data ended well before its duration (a download not
    /// finished, a preallocated file with zeros at the end): mpv stopped
    /// at the end of what is there. The app stays there, paused, or treats
    /// it as [`Happened::Eof`].
    NoData(DataEnd),
    /// The file could not be played.
    Failed(String),
    /// The file was a list mpv opened (an M3U at an address, or one
    /// inside a list): its entries with their titles, and the one mpv
    /// plays now. mpv keeps only that one in its playlist, the app's has
    /// the rest.
    Expanded {
        entries: Vec<(String, Option<String>)>,
        playing: usize,
    },
    /// A seek finished: the position is the new one.
    Restarted,
    /// A frame asked for by [`Player::save_frame`] was written, or why
    /// not; in the order asked.
    FrameSaved(Option<String>),
    /// [`Player::sub_step`] set the subtitle delay to this, or found no
    /// line there.
    SubStepped(Option<f64>),
    Shutdown,
}

/// Where the data of a file ran out ([`Happened::NoData`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DataEnd {
    /// A seek to this target found nothing there.
    Seek(f64),
    /// Playback reached the end of the data here.
    Played(f64),
}

/// The last seek, to tell an end of the file it ran into from one reached
/// by playing.
#[derive(Clone, Copy, Debug)]
struct SeekMark {
    at: std::time::Instant,
    /// The position before the seek (or before the first of a run of
    /// them, as the seek bar's drag makes): its data is there.
    from: f64,
    target: f64,
}

/// An end of the file this soon after a seek is the seek's.
const SEEK_EOF_WINDOW: std::time::Duration = std::time::Duration::from_secs(3);
/// Seconds before the duration an end of the file must come to mean
/// missing data: a seek on keyframes may land a GOP past its target, and
/// playing may reach a real end meanwhile.
const END_SLACK: f64 = 15.0;

/// Whether an end of the file means its data ran out, and where.
///
/// After a forward seek: the end came within [`SEEK_EOF_WINDOW`] of it,
/// and its target was further before the duration than playback at
/// `speed` could have gone since (mpv's position then is no guide: an MP4
/// says the duration). Otherwise the position `time` is [`END_SLACK`]
/// before the duration.
///
/// The duration of a file with an index (`indexed`: Matroska, MP4) is
/// right. Elsewhere it may be estimated from the bitrate (an MP3 without
/// a VBR header, raw AAC) and be longer than the file, so the data ran
/// out only where the file lacks data (`missing`, `incomplete.rs`): at
/// the seek's target, or halfway from where playback stopped to the
/// duration.
fn data_end(
    seek: Option<&SeekMark>,
    now: std::time::Instant,
    time: Option<f64>,
    duration: Option<f64>,
    speed: f64,
    indexed: bool,
    missing: &dyn Fn(f64) -> bool,
) -> Option<DataEnd> {
    let duration = duration?;
    // Where playback stopped, if well before the duration.
    let early = time.filter(|t| duration - t > END_SLACK);
    let end = match seek {
        Some(seek)
            if now.saturating_duration_since(seek.at) < SEEK_EOF_WINDOW
                && seek.target > seek.from =>
        {
            let elapsed = now.saturating_duration_since(seek.at);
            // Landed, and played on to the end of the data.
            match early.filter(|t| *t >= seek.target) {
                Some(t) => Some(DataEnd::Played(t)),
                None => {
                    let covered = elapsed.as_secs_f64() * speed.max(1.0);
                    (duration - seek.target > END_SLACK + covered)
                        .then_some(DataEnd::Seek(seek.target))
                }
            }
        }
        _ => early.map(DataEnd::Played),
    };
    end.filter(|end| {
        indexed
            || missing(match end {
                DataEnd::Seek(target) => *target,
                DataEnd::Played(t) => (t + duration) / 2.0,
            })
    })
}

/// Whether mpv's `file-format` is of a format whose duration is in its
/// index: Matroska ("mkv" is mpv's own demuxer, "matroska,webm" FFmpeg's),
/// MP4 and QuickTime.
fn indexed(file_format: Option<&str>) -> bool {
    file_format.is_some_and(|f| {
        matches!(f, "mkv" | "webm") || f.starts_with("matroska") || f.starts_with("mov")
    })
}

/// How a seek lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seek {
    /// On the nearest keyframe: instant, the default of the arrow keys.
    Keyframe,
    /// On the exact frame: decodes from the keyframe before it.
    Exact,
}

impl Seek {
    fn flag(self) -> &'static str {
        match self {
            Seek::Keyframe => "keyframes",
            Seek::Exact => "exact",
        }
    }
}

pub struct Player {
    mpv: Mpv,
    pub state: State,
    /// When `video-frame-info` was last asked for.
    scan_asked: Option<std::time::Instant>,
    /// When the playback's figures were last asked for.
    live_asked: Option<std::time::Instant>,
    /// A file was asked for and has not loaded or failed yet: the end of
    /// the one before, reported late, is not this one's (it would skip
    /// the file just chosen).
    loading: bool,
    /// Files asked for whose start mpv has not reported yet: an error
    /// ending a file meanwhile is the one's before (it failed as another
    /// was chosen), not the file chosen.
    starts_due: u32,
    /// Stopped by [`Player::stop`]: an end of the file reported late is
    /// not one to move on from.
    stopped: bool,
    /// Hardware decoding is on (the setting).
    hwdec: bool,
    /// Filters on the processor: hardware decoding copies the frames.
    hwdec_copy: bool,
    /// Filters on the processor are set (`vf` not empty).
    filters_on: bool,
    /// A new file starts without subtitles (`tracks::SubMode::Off`).
    subs_off: bool,
    last_seek: Option<SeekMark>,
    /// The audio filters mpv has, and those before the last change
    /// ([`Player::resync_audio`]).
    af: String,
    af_before: String,
    /// Stopped where the file's data ran out ([`Happened::NoData`]): Play
    /// goes on from here (the data there was played; the download may
    /// have gone on meanwhile), not from the start.
    data_end_at: Option<f64>,
    /// Where the data ran out, kept when that end is taken as the file's
    /// (`forget_data_end`): the position remembered there, not the file
    /// watched to its end. Until the next load.
    ran_out_at: Option<f64>,
}

impl Player {
    /// Take a started core: observe what the interface shows and wake the
    /// UI when events wait. The sound's settings are set before any file
    /// is loaded, so the first one plays with them; the device is
    /// Windows' default until the app picks one ([`audio::device_to_use`]).
    pub fn new(mut mpv: Mpv, ctx: &egui::Context, setup: &audio::Setup) -> Self {
        let wake = ctx.clone();
        mpv.set_wakeup(move || wake.request_repaint());
        // METALMPV_TRACE=2: mpv's verbose messages too.
        if std::env::var("METALMPV_TRACE").is_ok_and(|v| v == "2") {
            mpv.request_log_messages("v");
        } else if log::log_enabled!(log::Level::Info) {
            mpv.request_log_messages("info");
        } else {
            mpv.request_log_messages("warn");
        }
        observe_all(&mpv);
        let mut player = Self {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        player.set_channels(setup.channels);
        player.set_audio_filters(&audio::filters(
            setup.normalize,
            &setup.equalizer,
            None,
            None,
        ));
        player.set_passthrough(setup.passthrough);
        player.set_volume_max(setup.volume_max);
        let volume = setup.volume.clamp(0.0, player.state.volume_max);
        player.mpv.set("volume", &Value::Double(volume));
        player.state.volume = volume;
        player.set_mute(setup.mute);
        player
    }

    pub fn mpv(&self) -> &Mpv {
        &self.mpv
    }

    /// Take the waiting events: update [`Player::state`] and return what
    /// the app acts on. Call it every frame (and while minimised).
    pub fn poll(&mut self) -> Vec<Happened> {
        let mut out = Vec::new();
        while let Some(event) = self.mpv.next_event() {
            match event {
                Event::Property {
                    id: FRAME_INFO,
                    value,
                } => {
                    self.state.scan = scan_of(&value);
                    // Dynamic HDR10+ metadata: the scene's light levels.
                    self.state.hdr10_plus = value.get("scene-avg").is_some();
                }
                Event::Property { id, value }
                    if (LIVE_FIRST..LIVE_FIRST + LIVE.len() as u64).contains(&id) =>
                {
                    self.state.live.apply((id - LIVE_FIRST) as usize, &value);
                }
                Event::Property {
                    id: SUB_DELAY,
                    value,
                } => {
                    if let Some(delay) = value.double() {
                        self.state.sub_delay = delay;
                        out.push(Happened::SubStepped(Some(delay)));
                    }
                }
                Event::Property {
                    id: PLAYLIST,
                    value,
                } => {
                    let (entries, playing) = playlist_of(&value);
                    if !entries.is_empty() {
                        // The app's playlist moves on at the end of a file;
                        // mpv's would too.
                        self.mpv.command(0, &["playlist-clear"]);
                        out.push(Happened::Expanded { entries, playing });
                    }
                }
                Event::Property { id, value } => {
                    if let Some(prop) = Prop::of(id) {
                        let was_eof = self.state.eof;
                        self.state.apply(prop, &value);
                        if prop == Prop::EofReached && self.state.eof && !was_eof {
                            if self.loading || self.stopped {
                                log::debug!("end of the file before the one loading: ignored");
                                self.state.eof = false;
                            } else {
                                let seek = self.last_seek.take();
                                let s = &self.state;
                                let path = s.path.clone().unwrap_or_default();
                                let duration = s.duration.unwrap_or(0.0);
                                let mp3 = s.file_format.as_deref() == Some("mp3");
                                let end = data_end(
                                    seek.as_ref(),
                                    std::time::Instant::now(),
                                    s.time,
                                    s.duration,
                                    s.speed,
                                    indexed(s.file_format.as_deref()),
                                    &|at| {
                                        incomplete::unfilled(&path, at, duration)
                                            || incomplete::truncated(&path, duration, mp3)
                                    },
                                );
                                match end {
                                    Some(end) => {
                                        log::info!(
                                            "the data ends early ({end:?} of {:?})",
                                            s.duration
                                        );
                                        // Play goes on from where there was data.
                                        self.data_end_at = match end {
                                            DataEnd::Seek(_) => seek.map(|s| s.from),
                                            DataEnd::Played(t) => Some(t),
                                        };
                                        self.ran_out_at = self.data_end_at;
                                        out.push(Happened::NoData(end));
                                    }
                                    None => out.push(Happened::Eof),
                                }
                            }
                        }
                    }
                }
                Event::FileLoaded => {
                    self.loading = false;
                    self.state.scan = None;
                    self.state.hdr10_plus = false;
                    self.scan_asked = None;
                    self.state.live = Live::default();
                    self.live_asked = None;
                    out.push(Happened::FileLoaded);
                }
                Event::PlaybackRestart => out.push(Happened::Restarted),
                Event::StartFile => self.starts_due = self.starts_due.saturating_sub(1),
                Event::EndFile {
                    reason: EndReason::Error,
                    error,
                } if self.starts_due > 0 || self.stopped => {
                    log::debug!("the file before the one chosen failed: {error:?}");
                }
                Event::EndFile {
                    reason: EndReason::Error,
                    error,
                } => {
                    self.loading = false;
                    out.push(Happened::Failed(
                        error.unwrap_or_else(|| "unknown error".into()),
                    ));
                }
                Event::EndFile {
                    reason: EndReason::Redirect,
                    ..
                } => {
                    log::debug!("the file was a list: taking its entries");
                    self.mpv.get(PLAYLIST, "playlist", sys::MPV_FORMAT_NODE);
                }
                Event::EndFile { .. } => {}
                Event::Log {
                    prefix,
                    level,
                    text,
                } => match level.as_str() {
                    "fatal" | "error" => log::error!("mpv [{prefix}] {text}"),
                    "warn" => log::warn!("mpv [{prefix}] {text}"),
                    _ => log::info!("mpv [{prefix}] {text}"),
                },
                Event::QueueOverflow => {
                    log::warn!("mpv event queue overflowed; asking for the properties again");
                    self.state.scan = None;
                    self.scan_asked = None;
                    for (i, (_, name, format)) in PROPS.iter().enumerate() {
                        self.mpv.get(i as u64 + 1, name, *format);
                    }
                }
                Event::CommandReply {
                    id: LOAD,
                    error: Some(e),
                } => {
                    // Refused before anything was opened: no end-file comes,
                    // nor a start.
                    log::warn!("mpv loadfile failed: {e}");
                    self.loading = false;
                    self.starts_due = self.starts_due.saturating_sub(1);
                    out.push(Happened::Failed(e));
                }
                Event::CommandReply {
                    id: SCREENSHOT,
                    error,
                } => out.push(Happened::FrameSaved(error)),
                Event::CommandReply {
                    id: SUB_STEP,
                    error,
                } => match error {
                    // The delay mpv made, asked for after the command.
                    None => self.mpv.get(SUB_DELAY, "sub-delay", sys::MPV_FORMAT_DOUBLE),
                    Some(e) => {
                        log::debug!("sub-step: {e}");
                        out.push(Happened::SubStepped(None));
                    }
                },
                Event::CommandReply { error: Some(e), .. } => log::warn!("mpv command failed: {e}"),
                Event::Shutdown => out.push(Happened::Shutdown),
                _ => {}
            }
        }
        out
    }

    /// Ask how the picture is scanned, until it is known (the reply comes
    /// with the events; a file without a frame yet is asked again after a
    /// while).
    pub fn ask_scan(&mut self) {
        if self.state.scan.is_some() || self.state.video().is_none() {
            return;
        }
        let now = std::time::Instant::now();
        if self
            .scan_asked
            .is_some_and(|t| now.duration_since(t).as_millis() < 500)
        {
            return;
        }
        self.scan_asked = Some(now);
        self.mpv
            .get(FRAME_INFO, "video-frame-info", sys::MPV_FORMAT_NODE);
    }

    /// Ask for the playback's figures every second; the bitrate of the
    /// second before is noted for the graph (not while paused).
    pub fn ask_live(&mut self) {
        if self.state.idle {
            return;
        }
        let now = std::time::Instant::now();
        if self
            .live_asked
            .is_some_and(|t| now.duration_since(t) < LIVE_EVERY)
        {
            return;
        }
        if self.live_asked.is_some() && !self.state.paused && !self.state.buffering {
            self.state.live.sample();
        }
        self.live_asked = Some(now);
        for (i, (name, format)) in LIVE.iter().enumerate() {
            self.mpv.get(LIVE_FIRST + i as u64, name, *format);
        }
    }

    /// Play `path` from `start` seconds (`None`: the beginning) with
    /// `tracks` (`None`: mpv's choice), replacing the current file.
    pub fn load(&mut self, path: &Path, start: Option<f64>, tracks: Option<Tracks>) {
        self.mpv.set("pause", &Value::Flag(false));
        // The A-B points are the file's: mpv would keep them for the next.
        self.set_loop(None, None);
        let path = path.to_string_lossy();
        // File-local options: the next file starts afresh.
        let id = |n: Option<i64>| n.map_or("no".to_owned(), |n| n.to_string());
        let mut options = Vec::new();
        if let Some(t) = start {
            options.push(format!("start={t:.3}"));
        }
        // Always given: setting `aid`/`sid`/`vid` during a file sets mpv's
        // option too, and the next file would start with that track number
        // (or none) instead of mpv's choice.
        match tracks {
            Some(t) => {
                options.push(format!("aid={}", id(t.audio)));
                options.push(format!("sid={}", id(t.sub)));
            }
            None => {
                let sid = if self.subs_off { "no" } else { "auto" };
                options.extend(["aid=auto".to_owned(), format!("sid={sid}")]);
            }
        }
        options.push("vid=auto".to_owned());
        if self.hwdec_copy && !self.filters_on {
            self.hwdec_copy = false;
            self.apply_hwdec();
        }
        // The index (-1) comes before the options since mpv 0.38.
        let options = options.join(",");
        self.mpv
            .command(LOAD, &["loadfile", &path, "replace", "-1", &options]);
        self.state.paused = false;
        self.state.eof = false;
        self.loading = true;
        self.starts_due += 1;
        self.stopped = false;
        self.last_seek = None;
        self.data_end_at = None;
        self.ran_out_at = None;
    }

    /// Where the data ran out while playback stays at the end (mpv's
    /// position then may be the duration), even when that end was taken as
    /// the file's; `None` at the file's own end.
    pub fn ran_out_at(&self) -> Option<f64> {
        self.ran_out_at.filter(|_| self.state.eof)
    }

    /// The end of the data taken as the file's end: Play starts it again.
    pub fn forget_data_end(&mut self) {
        self.data_end_at = None;
    }

    /// Pause or go on; at the end of the file, play it again from the
    /// start, or where its data ran out if it stopped there.
    pub fn toggle_pause(&mut self) {
        if self.state.idle {
            return;
        }
        if self.state.eof {
            match self.data_end_at {
                Some(t) => self.seek_to(t, Seek::Exact),
                None => self.seek_to(0.0, Seek::Keyframe),
            }
            self.set_pause(false);
            return;
        }
        self.set_pause(!self.state.paused);
    }

    pub fn set_pause(&mut self, pause: bool) {
        self.mpv.set("pause", &Value::Flag(pause));
        // Shown at once; the property change confirms it.
        self.state.paused = pause;
    }

    /// Unload the file: the picture goes black.
    pub fn stop(&mut self) {
        self.mpv.command(0, &["stop"]);
        self.loading = false;
        self.stopped = true;
    }

    /// Move by `seconds` (negative: back).
    pub fn seek_by(&mut self, seconds: f64, how: Seek) {
        if self.state.idle {
            return;
        }
        let flags = format!("relative+{}", how.flag());
        self.mpv
            .command(0, &["seek", &format!("{seconds:.3}"), &flags]);
        if let Some(t) = self.state.time {
            self.mark_seek(t + seconds);
        }
    }

    /// Move to `seconds` from the start.
    pub fn seek_to(&mut self, seconds: f64, how: Seek) {
        if self.state.idle {
            return;
        }
        let flags = format!("absolute+{}", how.flag());
        self.mpv
            .command(0, &["seek", &format!("{:.3}", seconds.max(0.0)), &flags]);
        self.mark_seek(seconds.max(0.0));
        self.state.time = Some(seconds.max(0.0));
    }

    /// Remember a seek to `target` from the position now (where the data
    /// ran out, if playback stopped there); a run of seeks (the seek bar
    /// dragged, a key held) keeps the position before the first.
    fn mark_seek(&mut self, target: f64) {
        let now = std::time::Instant::now();
        let earlier = self
            .last_seek
            .filter(|s| now.saturating_duration_since(s.at) < SEEK_EOF_WINDOW)
            .map(|s| s.from);
        let here = if self.state.eof {
            self.data_end_at
        } else {
            self.state.time
        };
        // A new place chosen: Play no longer goes back to the data's end.
        self.data_end_at = None;
        self.last_seek = earlier.or(here).map(|from| SeekMark {
            at: now,
            from,
            target,
        });
    }

    /// One frame on (`forward`) or back; pauses.
    pub fn frame_step(&mut self, forward: bool) {
        if self.state.idle {
            return;
        }
        self.mpv.command(
            0,
            &[if forward {
                "frame-step"
            } else {
                "frame-back-step"
            }],
        );
        self.state.paused = true;
    }

    /// Volume, 0 to `volume_max`; unmutes.
    pub fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, self.state.volume_max);
        self.mpv.set("volume", &Value::Double(volume));
        self.state.volume = volume;
        if self.state.mute {
            self.set_mute(false);
        }
    }

    pub fn set_mute(&mut self, mute: bool) {
        self.mpv.set("mute", &Value::Flag(mute));
        self.state.mute = mute;
    }

    /// The volume's ceiling (100 to [`audio::MAX_VOLUME`]); a volume above
    /// it comes down to it.
    pub fn set_volume_max(&mut self, max: f64) {
        let max = max.clamp(100.0, audio::MAX_VOLUME);
        self.mpv.set("volume-max", &Value::Double(max));
        self.state.volume_max = max;
        if self.state.volume > max {
            self.mpv.set("volume", &Value::Double(max));
            self.state.volume = max;
        }
    }

    /// The output device (`audio-device`; empty: Windows' default); mpv
    /// moves the sound at once. It must be one mpv lists: mpv does not
    /// fall back from a device that is not there.
    pub fn set_audio_device(&mut self, name: &str) {
        let name = if name.is_empty() { "auto" } else { name };
        self.mpv.set("audio-device", &Value::Str(name.into()));
        self.resync_audio(false);
    }

    pub fn set_channels(&mut self, channels: Channels) {
        self.mpv
            .set("audio-channels", &Value::Str(channels.option().into()));
        self.resync_audio(false);
    }

    /// After the sound's chain was built anew (the device, the channels,
    /// the filters): play the sound on from where the picture is. The
    /// normalization's filter holds seconds of sound (its window), which
    /// is dropped with the chain, and mpv went on from where its decoder
    /// had got to, in silence until the picture caught up: eight seconds.
    /// An exact seek to the position decodes the sound from here again
    /// (0.2 s). Only with that filter, in the chain before or now
    /// (`filters_changed`): without it the gap is the device's reopening
    /// alone, and a seek would only make the picture stutter.
    fn resync_audio(&mut self, filters_changed: bool) {
        let holds = |af: &str| af.contains("dynaudnorm");
        let needed = holds(&self.af) || (filters_changed && holds(&self.af_before));
        // Not at the end (keep-open): a seek there ends the file again.
        let playing = !self.state.idle
            && !self.loading
            && !self.state.eof
            && tracks::selected(&self.state.tracks, Kind::Audio).is_some();
        if needed && playing {
            log::debug!("the sound's chain was rebuilt: seeking to the position");
            self.mpv.command(0, &["seek", "0", "relative+exact"]);
        }
    }

    /// The audio filters: `audio::filters`'s.
    pub fn set_audio_filters(&mut self, af: &str) {
        self.mpv.set("af", &Value::Str(af.to_owned()));
        self.af_before = std::mem::replace(&mut self.af, af.to_owned());
        self.resync_audio(true);
    }

    /// Give mpv what changed from `old` to `new` in the picture's
    /// adjustments.
    pub fn adjust(&mut self, old: &Adjust, new: &Adjust) {
        for c in Color::ALL {
            if old.get(c) != new.get(c) {
                self.mpv.set(c.property(), &Value::Int(new.get(c)));
            }
        }
        if old.rotation() != new.rotation() {
            self.mpv.set("video-rotate", &Value::Int(new.rotation()));
        }
        let filters = new.filters();
        if old.filters() != filters {
            // Copying first: the filters fail on frames in the graphics
            // card. Back to the card's frames only with the next file
            // (`load`): each switch starts the decoder again with a seek,
            // and a slider near zero, or Mirror, switched at each step.
            self.filters_on = !filters.is_empty();
            if self.filters_on && !self.hwdec_copy {
                self.hwdec_copy = true;
                self.apply_hwdec();
            }
            self.mpv.set("vf", &Value::Str(filters));
        }
        if old.deband != new.deband {
            self.mpv.set("deband", &Value::Flag(new.deband));
        }
        if old.aspect != new.aspect {
            self.mpv.set(
                "video-aspect-override",
                &Value::Str(new.aspect.option().into()),
            );
        }
        if old.zoom != new.zoom {
            self.mpv
                .set("video-zoom", &Value::Double(new.zoom_option()));
        }
        if old.pan_x != new.pan_x {
            self.mpv
                .set("video-pan-x", &Value::Double(new.pan_x as f64 / 100.0));
        }
        if old.pan_y != new.pan_y {
            self.mpv
                .set("video-pan-y", &Value::Double(new.pan_y as f64 / 100.0));
        }
        if old.deinterlace != new.deinterlace {
            self.mpv
                .set("deinterlace", &Value::Str(new.deinterlace.option().into()));
        }
        if old.tone_map != new.tone_map {
            self.mpv
                .set("tone-mapping", &Value::Str(new.tone_map.option().into()));
        }
    }

    /// Save the frame on screen as it was decoded (without subtitles) to
    /// `path`; the extension says the format.
    pub fn save_frame(&mut self, path: &Path) {
        let path = path.to_string_lossy();
        self.mpv
            .command(SCREENSHOT, &["screenshot-to-file", path.as_ref(), "video"]);
    }

    /// Compressed sound to a receiver as it is, or decoded here.
    pub fn set_passthrough(&mut self, passthrough: Passthrough) {
        self.mpv
            .set("audio-spdif", &Value::Str(passthrough.option().into()));
        self.resync_audio(false);
    }

    /// The sound against the picture, in seconds; kept between files.
    /// Play the file again and again without a gap (`loop-file`), or once.
    pub fn set_loop_file(&mut self, on: bool) {
        let value = if on { "inf" } else { "no" };
        self.mpv.set("loop-file", &Value::Str(value.into()));
    }

    /// The A-B repeat's points (`None`: not set): from `b` playback goes
    /// back to `a` while both are set; in order.
    pub fn set_loop(&mut self, a: Option<f64>, b: Option<f64>) {
        let (a, b) = match (a, b) {
            (Some(a), Some(b)) if b < a => (Some(b), Some(a)),
            other => other,
        };
        let value = |t: Option<f64>| t.map_or(Value::Str("no".into()), Value::Double);
        self.mpv.set("ab-loop-a", &value(a));
        self.mpv.set("ab-loop-b", &value(b));
        self.state.loop_a = a;
        self.state.loop_b = b;
    }

    /// The playback speed (mpv keeps the voice's pitch); kept between files.
    pub fn set_speed(&mut self, speed: f64) {
        self.mpv.set("speed", &Value::Double(speed));
        self.state.speed = speed;
    }

    /// The subtitles against the picture, in seconds; kept between files.
    pub fn set_sub_delay(&mut self, seconds: f64) {
        let seconds = (seconds * 1000.0).round() / 1000.0;
        self.mpv.set("sub-delay", &Value::Double(seconds));
        self.state.sub_delay = seconds;
    }

    /// Set the subtitle delay so that the line `lines` after the one on
    /// screen (before it, when negative) shows now: mpv's `sub-step`, for
    /// the lines it has read (all of a file of their own); the delay comes
    /// as [`Happened::SubStepped`].
    pub fn sub_step(&mut self, lines: i32) {
        self.mpv
            .command(SUB_STEP, &["sub-step", &lines.to_string()]);
    }

    pub fn set_audio_delay(&mut self, seconds: f64) {
        let seconds = (seconds * 1000.0).round() / 1000.0;
        self.mpv.set("audio-delay", &Value::Double(seconds));
        self.state.audio_delay = seconds;
    }

    /// Hardware decoding on (`auto-safe`) or off; mpv applies it to the
    /// playing file at once.
    pub fn set_hwdec(&mut self, on: bool) {
        self.hwdec = on;
        self.apply_hwdec();
    }

    fn apply_hwdec(&mut self) {
        let value = hwdec_value(self.hwdec, self.hwdec_copy);
        self.mpv.set("hwdec", &Value::Str(value.into()));
    }

    /// The languages and the subtitles a new file starts with, and the
    /// subtitle files loaded with it (`tracks::TrackPrefs`); from the next
    /// file on.
    pub fn set_track_prefs(&mut self, prefs: &tracks::TrackPrefs) {
        for (name, value) in prefs.options() {
            self.mpv.set(name, &Value::Str(value));
        }
        self.subs_off = prefs.sub_mode == tracks::SubMode::Off;
    }

    /// Enlarge the picture to fill the window, cropping what is beyond it
    /// (mpv's `panscan` 1), or show it whole (0).
    pub fn set_fill(&mut self, on: bool) {
        self.mpv
            .set("panscan", &Value::Double(if on { 1.0 } else { 0.0 }));
    }

    /// How subtitles look (`sub_style.rs`); ASS subtitles keep their own
    /// fonts and colours.
    pub fn set_sub_style(&mut self, style: &SubStyle) {
        for (name, value) in style.options() {
            self.mpv.set(name, &Value::Str(value));
        }
    }

    /// Whether the subtitles' style applies to ASS ones too (else mpv's
    /// `scale`: only the size), and whether subtitles go in the black bars
    /// (ASS ones too) or over the picture.
    pub fn set_sub_layout(&mut self, ass_override: bool, margins: bool) {
        let override_value = if ass_override { "force" } else { "scale" };
        self.mpv
            .set("sub-ass-override", &Value::Str(override_value.into()));
        self.mpv.set("sub-use-margins", &Value::Flag(margins));
        self.mpv.set("sub-ass-force-margins", &Value::Flag(margins));
    }

    /// Play audio track `id`, or show subtitle track `id` (`None`: none).
    /// Shown selected at once; `track-list` confirms it.
    pub fn select_track(&mut self, kind: Kind, id: Option<i64>) {
        let property = match kind {
            Kind::Audio => "aid",
            Kind::Sub => "sid",
            Kind::Video => "vid",
        };
        let value = match id {
            Some(id) => Value::Int(id),
            None => Value::Str("no".into()),
        };
        self.mpv.set(property, &value);
        for t in self.state.tracks.iter_mut().filter(|t| t.kind == kind) {
            t.selected = Some(t.id) == id;
        }
    }

    /// Load the subtitles of file `path` and show them.
    pub fn add_subtitles(&mut self, path: &Path) {
        self.mpv
            .command(0, &["sub-add", &path.to_string_lossy(), "select"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_are_numbered_by_their_place() {
        for (i, (prop, _, _)) in PROPS.iter().enumerate() {
            assert_eq!(Prop::of(i as u64 + 1), Some(*prop));
        }
        assert_eq!(Prop::of(0), None);
        assert_eq!(Prop::of(PROPS.len() as u64 + 1), None);
        // Below the other replies' numbers.
        assert!((PROPS.len() as u64) < FRAME_INFO.min(LOAD).min(LIVE_FIRST));
    }

    #[test]
    fn an_end_far_from_the_duration_is_missing_data() {
        use std::time::{Duration, Instant};
        let at = Instant::now();
        // An end of a 120 s MP4 `ms` after a seek from `from` to `target`,
        // mpv's position then `time`.
        let after_seek = |from, target, ms, speed, time| {
            let seek = SeekMark { at, from, target };
            let now = at + Duration::from_millis(ms);
            data_end(
                Some(&seek),
                now,
                Some(time),
                Some(120.0),
                speed,
                true,
                &|_| false,
            )
        };
        // Half the file there, a seek to 90 s: mpv stops at once (an MP4
        // says the duration as the position).
        assert_eq!(
            after_seek(10.0, 90.0, 100, 1.0, 120.0),
            Some(DataEnd::Seek(90.0))
        );
        // Near the end: a keyframe past it, or the end really reached.
        assert_eq!(after_seek(10.0, 110.0, 100, 1.0, 120.0), None);
        // Fast playback covers more of the file meanwhile.
        assert_eq!(
            after_seek(10.0, 100.0, 2000, 1.0, 120.0),
            Some(DataEnd::Seek(100.0))
        );
        assert_eq!(after_seek(10.0, 100.0, 2000, 4.0, 120.0), None);
        // Landed, and the data ended soon after.
        assert_eq!(
            after_seek(10.0, 90.0, 2000, 1.0, 92.0),
            Some(DataEnd::Played(92.0))
        );
        // Long after the seek, or after one back: the position tells.
        assert_eq!(
            after_seek(10.0, 30.0, 4000, 1.0, 60.0),
            Some(DataEnd::Played(60.0))
        );
        assert_eq!(
            after_seek(90.0, 10.0, 100, 1.0, 60.0),
            Some(DataEnd::Played(60.0))
        );
        assert_eq!(after_seek(10.0, 30.0, 4000, 1.0, 119.9), None);
        // Played to there without a seek.
        let played =
            |time, indexed| data_end(None, at, Some(time), Some(120.0), 1.0, indexed, &|_| false);
        assert_eq!(played(60.0, true), Some(DataEnd::Played(60.0)));
        assert_eq!(played(119.9, true), None);
        // A duration from the bitrate (an MP3) may be wrong: an end, unless
        // the file has no data there yet (zeros from 70 s on).
        assert_eq!(played(60.0, false), None);
        let zeros = |t: f64| t >= 70.0;
        let mp3 = |seek: Option<&SeekMark>, time| {
            data_end(seek, at, Some(time), Some(120.0), 1.0, false, &zeros)
        };
        assert_eq!(mp3(None, 60.0), Some(DataEnd::Played(60.0)));
        let seek = |target| SeekMark {
            at,
            from: 10.0,
            target,
        };
        assert_eq!(mp3(Some(&seek(90.0)), 120.0), Some(DataEnd::Seek(90.0)));
        // A whole file whose duration was over-estimated: data everywhere.
        let whole = |seek: Option<&SeekMark>, time| {
            data_end(seek, at, Some(time), Some(120.0), 1.0, false, &|_| false)
        };
        assert_eq!(whole(Some(&seek(90.0)), 120.0), None);
        assert_eq!(whole(None, 60.0), None);
        // No duration: nothing to compare.
        assert_eq!(
            data_end(None, at, Some(60.0), None, 1.0, true, &|_| false),
            None
        );
        assert!(indexed(Some("mkv")));
        assert!(indexed(Some("matroska,webm")));
        assert!(indexed(Some("mov,mp4,m4a,3gp,3g2,mj2")));
        assert!(!indexed(Some("mp3")));
        assert!(!indexed(None));
    }

    #[test]
    fn state_follows_properties() {
        let mut s = State::default();
        s.apply(Prop::TimePos, &Value::Double(30.0));
        s.apply(Prop::Duration, &Value::Double(120.0));
        assert_eq!(s.fraction(), Some(0.25));
        s.apply(Prop::Duration, &Value::None);
        assert_eq!(s.fraction(), None);
        s.apply(Prop::IdleActive, &Value::Flag(false));
        s.apply(Prop::Width, &Value::Int(1920));
        assert_eq!(s.video(), None);
        s.apply(Prop::Height, &Value::Int(1080));
        assert_eq!(s.video(), Some((1920, 1080)));
        s.apply(Prop::Width, &Value::None);
        assert_eq!(s.video(), None);
        s.apply(Prop::Hwdec, &Value::Str("no".into()));
        assert_eq!(s.hwdec, None);
        s.apply(Prop::Hwdec, &Value::Str("d3d11va-copy".into()));
        assert_eq!(s.hwdec.as_deref(), Some("d3d11va-copy"));
    }

    #[test]
    fn options_are_well_formed() {
        for (name, value) in options(true) {
            assert!(!name.is_empty() && !name.starts_with('-'), "{name}");
            assert!(!value.is_empty(), "{name}");
        }
        if std::env::var_os("METALMPV_HWDEC").is_none() {
            assert!(options(false).contains(&("hwdec", "no")));
        }
    }

    #[test]
    fn engine_messages_say_what_to_do() {
        // Whatever the language: the window names the file, not the
        // paths looked in or Windows' words, which stay in the log.
        let missing = EngineError::NoDll("libmpv-2.dll not found (looked in C:/x)".into());
        assert!(missing.message().contains("libmpv-2.dll"));
        assert!(!missing.message().contains("C:/x"));
        let bad = EngineError::BadDll("cannot load C:/x/libmpv-2.dll: os error 193".into());
        assert!(!bad.message().contains("os error"));
        assert!(bad.to_string().contains("os error 193"));
        let failed = EngineError::Failed("video output: no OpenGL".into());
        assert!(failed.message().contains("video output: no OpenGL"));
    }

    #[test]
    fn boot_without_the_dll_fails_cleanly() {
        // Test builds run from target\debug\deps; the DLL may be found in
        // vendor\libmpv, in which case the core starts.
        match boot(false) {
            Ok(_) => {}
            Err(EngineError::NoDll(detail)) => assert!(detail.contains(sys::DLL_NAME), "{detail}"),
            Err(e) => panic!("{e:?}"),
        }
    }

    /// The A-B repeat on a real file: mpv takes the points, playback goes
    /// back from B to A, and the points go with the next file:
    /// `cargo test ab_loop -- --ignored --nocapture` (a video of at least
    /// 3 s in METALMPV_SAMPLE or the default below).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn ab_loop_of_a_real_file() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("keep-open", "yes"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        observe_all(&player.mpv);
        let wait = |player: &mut Player, seconds: f64, until: &mut dyn FnMut(&State) -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < deadline {
                player.poll();
                if until(&player.state) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        player.load(Path::new(&sample), None, None);
        assert!(wait(&mut player, 5.0, &mut |s| s
            .time
            .is_some_and(|t| t > 0.2)));
        // Set while playing, in the wrong order: kept in order.
        player.set_loop(Some(1.5), Some(0.5));
        assert!(wait(&mut player, 2.0, &mut |s| s.loop_a == Some(0.5)
            && s.loop_b == Some(1.5)));
        // Played past B once, it is back before it.
        let (mut highest, mut passed, mut back) = (0.0f64, false, false);
        wait(&mut player, 3.5, &mut |s| {
            let t = s.time.unwrap_or(0.0);
            highest = highest.max(t);
            passed |= t > 1.3;
            back |= passed && t < 1.0;
            false
        });
        eprintln!("highest {highest:.2}, back {back}");
        assert!(back, "playback did not go back to A");
        assert!(highest < 2.0, "playback went on past B: {highest}");
        // Off.
        player.set_loop(None, None);
        assert!(wait(&mut player, 2.0, &mut |s| s.loop_a.is_none()
            && s.loop_b.is_none()));
        // The next file starts without them.
        player.set_loop(Some(0.5), Some(1.5));
        player.load(Path::new(&sample), None, None);
        assert!(wait(&mut player, 3.0, &mut |s| s.loop_a.is_none()
            && s.loop_b.is_none()
            && s.time.is_some_and(|t| t > 1.8)));
    }

    /// A file cut short (an interrupted download): a seek past its data and
    /// playing to its end are reported as missing data, mpv stays paused
    /// there, and Play goes on from where there was data:
    /// `cargo test end_of_the_data -- --ignored --nocapture` (a video of
    /// 60 s or more in METALMPV_SAMPLE, Matroska or an MP4 with its index
    /// first, or the default below; its first third is played).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn end_of_the_data_stops_there() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let bytes = std::fs::read(&sample).unwrap();
        let dir = std::env::temp_dir().join("metalmpv_cut_test");
        std::fs::create_dir_all(&dir).unwrap();
        let ext = Path::new(&sample).extension().unwrap().to_string_lossy();
        let cut = dir.join(format!("cut.{ext}"));
        std::fs::write(&cut, &bytes[..bytes.len() / 3]).unwrap();
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("keep-open", "yes"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        observe_all(&player.mpv);
        let wait = |player: &mut Player,
                    seconds: f64,
                    until: &mut dyn FnMut(&State, &[Happened]) -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < deadline {
                let happened = player.poll();
                if until(&player.state, &happened) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        player.load(&cut, None, None);
        assert!(wait(&mut player, 5.0, &mut |s, _| s
            .time
            .is_some_and(|t| t > 1.0)));
        let duration = player.state.duration.expect("no duration");
        assert!(duration >= 60.0, "the sample is too short: {duration}");
        // Waits for the data's end, which must not be taken for the file's.
        let no_data = |player: &mut Player, seconds: f64| {
            let mut end = None;
            wait(player, seconds, &mut |s, happened| {
                assert!(
                    !happened.contains(&Happened::Eof),
                    "taken for the end at {:?} of {:?}, {:?}",
                    s.time,
                    s.duration,
                    s.file_format
                );
                end = happened.iter().find_map(|h| match h {
                    Happened::NoData(e) => Some(*e),
                    _ => None,
                });
                end.is_some()
            });
            end.expect("no missing data reported")
        };
        // Stopped: paused at the end, the position still.
        let stays = |player: &mut Player| {
            let t = player.state.time;
            wait(player, 1.0, &mut |_, _| false);
            let s = &player.state;
            assert!(s.eof && s.paused, "eof {}, paused {}", s.eof, s.paused);
            assert_eq!(s.time, t, "the position moved");
        };
        let from = player.state.time.unwrap();
        let target = duration - 20.0;
        player.seek_to(target, Seek::Keyframe);
        assert_eq!(no_data(&mut player, 5.0), DataEnd::Seek(target));
        stays(&mut player);
        // Play: on from where it was before the seek.
        player.toggle_pause();
        assert!(wait(&mut player, 5.0, &mut |s, _| !s.eof
            && !s.paused
            && s.time.is_some_and(|t| t > from + 0.5 && t < from + 3.0)));
        // Played to the end of the data (about a third of the duration).
        player.seek_to(duration / 3.0 - 6.0, Seek::Exact);
        let end = no_data(&mut player, 15.0);
        eprintln!("{end:?} of {duration:.1} s");
        let DataEnd::Played(at) = end else {
            panic!("{end:?}")
        };
        assert!((at - duration / 3.0).abs() < 5.0, "{at}");
        stays(&mut player);
        // Play: no more data there, stopped again.
        player.toggle_pause();
        assert!(matches!(no_data(&mut player, 5.0), DataEnd::Played(t) if (t - at).abs() < 1.0));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An MP3 being downloaded, kept at its full size with zeros or cut
    /// short, stops where its data ends after a seek past it, and a whole
    /// one does not: `cargo test an_mp3_download -- --ignored --nocapture`
    /// with an MP3 of a minute or more in METALMPV_MP3.
    #[test]
    #[ignore = "needs vendor/libmpv and an MP3"]
    fn an_mp3_download_stops_where_its_data_ends() {
        let sample = std::env::var("METALMPV_MP3").unwrap_or_else(|_| {
            "D:/Downloads/Amorphis - Borderland (2025)/01 The Circle.mp3".into()
        });
        let bytes = std::fs::read(&sample).unwrap();
        let dir = std::env::temp_dir().join("metalmpv_mp3_test");
        std::fs::create_dir_all(&dir).unwrap();
        let had = bytes.len() * 2 / 5;
        let mut kept = bytes[..had].to_vec();
        kept.resize(bytes.len(), 0);
        let cases = [
            ("kept.mp3", kept, true),
            ("cut.mp3", bytes[..had].to_vec(), true),
            ("whole.mp3", bytes.clone(), false),
        ];
        for (name, content, stops) in cases {
            let file = dir.join(name);
            std::fs::write(&file, content).unwrap();
            let api = sys::api().unwrap();
            let mpv = Mpv::new(
                api,
                &[
                    ("vo", "null"),
                    ("ao", "null"),
                    ("idle", "yes"),
                    ("terminal", "no"),
                    ("keep-open", "yes"),
                ],
            )
            .unwrap();
            let mut player = Player {
                mpv,
                state: State::default(),
                scan_asked: None,
                live_asked: None,
                loading: false,
                starts_due: 0,
                stopped: false,
                hwdec: true,
                hwdec_copy: false,
                filters_on: false,
                subs_off: false,
                last_seek: None,
                data_end_at: None,
                ran_out_at: None,
                af: String::new(),
                af_before: String::new(),
            };
            observe_all(&player.mpv);
            player.load(&file, None, None);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while player.state.time.is_none_or(|t| t < 1.0) && std::time::Instant::now() < deadline
            {
                player.poll();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let duration = player.state.duration.expect("no duration");
            assert!(duration >= 60.0, "the sample is too short: {duration}");
            player.seek_to(duration * 0.7, Seek::Keyframe);
            let mut ended = None;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while ended.is_none() && std::time::Instant::now() < deadline {
                ended = player.poll().into_iter().find_map(|h| match h {
                    Happened::NoData(e) => Some(Some(e)),
                    Happened::Eof => Some(None),
                    _ => None,
                });
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            eprintln!("{name}: {duration:.1} s, {ended:?}");
            if stops {
                assert!(
                    matches!(ended, Some(Some(DataEnd::Seek(_)))),
                    "{name}: {ended:?}"
                );
            } else {
                // Plays on there.
                assert_eq!(ended, None, "{name}");
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A list mpv opens itself (as one at an address would be) gives its
    /// entries to the app, and mpv keeps only the one it plays:
    /// `cargo test list_mpv_opens -- --ignored` (a video in METALMPV_SAMPLE
    /// or the default below).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn list_mpv_opens_gives_its_entries() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let dir = std::env::temp_dir().join("metalmpv_list_test");
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("list.m3u");
        std::fs::write(
            &list,
            format!("#EXTM3U\n#EXTINF:-1,The sample\n{sample}\nhttps://example.invalid/2.ts\n"),
        )
        .unwrap();
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("keep-open", "yes"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        player
            .mpv
            .observe(1, "playlist-count", sys::MPV_FORMAT_INT64);
        player.load(&list, None, None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut expanded = None;
        let mut loaded = false;
        let mut count = None;
        while std::time::Instant::now() < deadline && !(loaded && count == Some(1)) {
            while let Some(event) = player.mpv.next_event() {
                match event {
                    Event::Property { id: 1, value } => count = value.int(),
                    Event::Property {
                        id: PLAYLIST,
                        value,
                    } => {
                        let (entries, playing) = playlist_of(&value);
                        player.mpv.command(0, &["playlist-clear"]);
                        expanded = Some((entries, playing));
                    }
                    Event::EndFile {
                        reason: EndReason::Redirect,
                        ..
                    } => player.mpv.get(PLAYLIST, "playlist", sys::MPV_FORMAT_NODE),
                    Event::FileLoaded => loaded = true,
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let (entries, playing) = expanded.expect("no redirect");
        eprintln!("{entries:?}, playing {playing}");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1.as_deref(), Some("The sample"));
        assert!(entries[1].0.ends_with("2.ts"));
        assert_eq!(playing, 0);
        assert!(loaded, "the first entry did not load");
        assert_eq!(count, Some(1), "mpv kept the other entries");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Plays a real file without a window and reads its tracks:
    /// `cargo test track_list -- --ignored` (vendor\libmpv and a video in
    /// METALMPV_SAMPLE, or the default below).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn track_list_of_a_real_file() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        mpv.observe(1, "track-list", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &sample]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut found = Vec::new();
        while found.is_empty() && std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                if let Event::Property { id: 1, value } = event {
                    found = tracks::from_value(&value);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for t in &found {
            eprintln!(
                "{:?} {} {} [{}]",
                t.kind,
                t.id,
                t.name_in(crate::i18n::Lang::Ru, 1),
                t.details_in(crate::i18n::Lang::Ru)
            );
        }
        assert!(found.iter().any(|t| t.kind == Kind::Video));
    }

    /// The languages and the subtitles' mode choose a new file's tracks:
    /// `cargo test track_prefs -- --ignored --nocapture` (a film with
    /// Ukrainian and English sound and English subtitles in METALMPV_SAMPLE,
    /// or the default below).
    #[test]
    #[ignore = "needs vendor/libmpv and a film with several languages"]
    fn track_prefs_choose_the_tracks() {
        // Highlander (TeamHD): sixteen Russian sound tracks, then two
        // Ukrainian, two English; Russian and English subtitles.
        let sample = std::env::var("METALMPV_SAMPLE").unwrap_or_else(|_| {
            "D:/Films/Highlander.1986.REMASTERED.1080p.BluRay.16xRus.2xUkr.2xEng.TeamHD-Атлас31.mkv"
                .into()
        });
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        observe_all(&player.mpv);
        let chosen = |player: &mut Player, prefs: tracks::TrackPrefs| {
            player.set_track_prefs(&prefs);
            player.load(Path::new(&sample), None, None);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                player.poll();
                if !player.loading && player.state.tracks.len() > 3 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            // The selection settles with the track list.
            std::thread::sleep(std::time::Duration::from_millis(300));
            player.poll();
            let lang = |kind| {
                tracks::selected(&player.state.tracks, kind)
                    .map(|t| t.lang.clone().unwrap_or_default())
            };
            (lang(Kind::Audio), lang(Kind::Sub))
        };
        use tracks::{SubMode, TrackPrefs};
        let (audio, sub) = chosen(
            &mut player,
            TrackPrefs {
                audio: vec!["ukr".into(), "eng".into()],
                sub_mode: SubMode::Languages,
                subs: vec!["eng".into()],
                ..TrackPrefs::default()
            },
        );
        eprintln!("ukr, eng / eng: {audio:?} {sub:?}");
        assert_eq!(audio.as_deref(), Some("ukr"));
        assert_eq!(sub.as_deref(), Some("eng"));
        let (audio, sub) = chosen(
            &mut player,
            TrackPrefs {
                audio: vec!["eng".into()],
                sub_mode: SubMode::Off,
                ..TrackPrefs::default()
            },
        );
        eprintln!("eng / off: {audio:?} {sub:?}");
        assert_eq!(audio.as_deref(), Some("eng"));
        assert_eq!(sub, None);
    }

    /// A track chosen in one file is not carried to the next:
    /// `cargo test tracks_start_afresh -- --ignored` (a video with sound in
    /// METALMPV_SAMPLE, or the default below).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video with sound"]
    fn tracks_start_afresh() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        observe_all(&player.mpv);
        let wait = |player: &mut Player, until: &dyn Fn(&State) -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                player.poll();
                if until(&player.state) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            false
        };
        let playing = |s: &State| tracks::selected(&s.tracks, Kind::Audio).is_some();
        player.load(Path::new(&sample), None, None);
        assert!(wait(&mut player, &playing), "no audio track at first");
        player.select_track(Kind::Audio, None);
        player.select_track(Kind::Video, None);
        assert!(wait(&mut player, &|s| s.tracks.iter().all(|t| !t.selected)));
        player.load(Path::new(&sample), None, None);
        assert!(
            wait(&mut player, &|s| playing(s)
                && tracks::selected(&s.tracks, Kind::Video).is_some()),
            "the next file kept the tracks turned off"
        );
        // A file that cannot be opened fails, once the start of it is in.
        player.load(Path::new("D:/metalmpv-missing/none.mkv"), None, None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut failed = false;
        while !failed && std::time::Instant::now() < deadline {
            failed = player
                .poll()
                .iter()
                .any(|h| matches!(h, Happened::Failed(_)));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(failed, "a missing file did not fail");
        assert_eq!(player.starts_due, 0);
        // A value refused is answered (and logged) like one taken.
        player
            .mpv
            .set("video-rotate", &Value::Str("sideways".into()));
        player.mpv.set("volume", &Value::Double(50.0));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while player.mpv.sets_waiting() > 0 && std::time::Instant::now() < deadline {
            player.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(player.mpv.sets_waiting(), 0);
    }

    /// `sub-step` moves a subtitle file's next line to the position and
    /// says the delay, or that there is no such line: `cargo test
    /// sub_step -- --ignored` (a video in METALMPV_SAMPLE, or the default
    /// below; the subtitles are written by the test).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn sub_step_of_mpv() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let srt = std::env::temp_dir().join(format!("metalmpv_step_{}.srt", std::process::id()));
        std::fs::write(
            &srt,
            "1\n00:00:01,000 --> 00:00:02,000\nOne\n\n2\n00:00:05,000 --> 00:00:06,000\nTwo\n",
        )
        .unwrap();
        let lines = crate::sub_lines::read(&srt).unwrap();
        assert_eq!(lines.len(), 2);
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        observe_all(&player.mpv);
        let until = |player: &mut Player, what: &dyn Fn(&Happened) -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < deadline {
                if let Some(h) = player.poll().into_iter().find(|h| what(h)) {
                    return Some(h);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            None
        };
        player.load(Path::new(&sample), None, None);
        assert!(until(&mut player, &|h| *h == Happened::FileLoaded).is_some());
        player.set_pause(true);
        player.add_subtitles(&srt);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while tracks::selected(&player.state.tracks, Kind::Sub).is_none()
            && std::time::Instant::now() < deadline
        {
            player.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let sub = tracks::selected(&player.state.tracks, Kind::Sub).expect("subtitles added");
        assert_eq!(
            sub.external_filename.as_deref().map(Path::new),
            Some(srt.as_path())
        );
        let stepped = |h: &Happened| matches!(h, Happened::SubStepped(_));
        player.sub_step(1);
        let Some(Happened::SubStepped(Some(delay))) = until(&mut player, &stepped) else {
            panic!("no delay after sub-step");
        };
        let time = player.state.time.unwrap_or(0.0);
        // A line's start is now at the position.
        assert!(
            lines.iter().any(|l| (l.start + delay - time).abs() < 0.1),
            "delay {delay} at {time}"
        );
        assert_eq!(player.state.sub_delay, delay);
        // Past the last line mpv stops at it.
        player.sub_step(50);
        let Some(Happened::SubStepped(Some(delay))) = until(&mut player, &stepped) else {
            panic!("no delay after a step past the end");
        };
        assert!((5.0 + delay - time).abs() < 0.1, "delay {delay} at {time}");
        let _ = std::fs::remove_file(&srt);
    }

    /// The playback's figures and the tracks' ids on a real file without a
    /// window: `cargo test live_figures -- --ignored --nocapture` with
    /// METALMPV_SAMPLE. With `vo=null` there is no screen to give a rate.
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn live_figures_of_a_real_file() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        mpv.observe(1, "track-list", sys::MPV_FORMAT_NODE);
        mpv.observe(2, "metadata", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &sample]);
        let mut live = Live::default();
        let mut tracks = Vec::new();
        let start = std::time::Instant::now();
        let mut asked = start;
        while start.elapsed() < std::time::Duration::from_secs(4) {
            if asked.elapsed() > std::time::Duration::from_millis(500) {
                asked = std::time::Instant::now();
                for (i, (name, format)) in LIVE.iter().enumerate() {
                    mpv.get(LIVE_FIRST + i as u64, name, *format);
                }
            }
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::Property { id: 1, value } => tracks = tracks::from_value(&value),
                    Event::Property { id: 2, value } => eprintln!("metadata: {value:?}"),
                    Event::Property { id, value } if id >= LIVE_FIRST => {
                        live.apply((id - LIVE_FIRST) as usize, &value)
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        eprintln!("{live:#?}");
        for t in &tracks {
            eprintln!(
                "{:?} {} src-id {:?} ff-index {:?} profile {:?}",
                t.kind, t.id, t.src_id, t.ff_index, t.profile
            );
        }
        assert!(live.video_bitrate.or(live.audio_bitrate).is_some());
        assert!(live.audio_out.is_some());
    }

    /// The sound's settings on a real file without a window or a sound
    /// card (`ao=null`): the normalization filter is taken, the volume
    /// goes past 100, the channels are mixed as asked, and mpv lists the
    /// devices. `cargo test audio_settings -- --ignored --nocapture` with
    /// METALMPV_SAMPLE.
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn audio_settings_of_a_real_file() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        mpv.request_log_messages("warn");
        let eq = audio::Equalizer {
            on: true,
            gains: audio::Preset::Rock.gains(),
        };
        mpv.set(
            "af",
            &Value::Str(audio::filters(
                Some(audio::Normalize::Medium),
                &eq,
                None,
                None,
            )),
        );
        mpv.set(
            "audio-channels",
            &Value::Str(Channels::Stereo.option().into()),
        );
        mpv.set("volume-max", &Value::Double(200.0));
        mpv.set("volume", &Value::Double(150.0));
        // Taken without a word; `ao=null` decodes as usual.
        mpv.set(
            "audio-spdif",
            &Value::Str(Passthrough::Hdmi.option().into()),
        );
        mpv.observe(1, "audio-out-params", sys::MPV_FORMAT_NODE);
        mpv.observe(2, "volume", sys::MPV_FORMAT_DOUBLE);
        mpv.observe(3, "audio-device-list", sys::MPV_FORMAT_NODE);
        mpv.observe(4, "af", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &sample]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut channels, mut volume, mut devices, mut errors) = (None, None, None, Vec::new());
        while channels.is_none() && std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::Property { id: 1, value } => {
                        channels = value
                            .get("hr-channels")
                            .and_then(Value::string)
                            .map(str::to_owned);
                    }
                    Event::Property { id: 2, value } => volume = value.double(),
                    Event::Property { id: 3, value } => {
                        devices = Some(audio::devices_from_value(&value))
                    }
                    Event::Property { id: 4, value } => eprintln!("af: {value:?}"),
                    Event::Log { prefix, text, .. } => {
                        eprint!("[{prefix}] {text}");
                        errors.push(text);
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let devices = devices.unwrap_or_default();
        for d in &devices {
            eprintln!("device {} = {}", d.name, d.description);
        }
        eprintln!("channels {channels:?}, volume {volume:?}");
        assert_eq!(channels.as_deref(), Some("stereo"));
        assert_eq!(volume, Some(150.0));
        assert!(!devices.is_empty());
        assert!(
            !errors
                .iter()
                .any(|e| e.contains("dynaudnorm") || e.contains("filter")),
            "{errors:?}"
        );
    }

    /// mpv knows the subtitles' properties and takes their values:
    /// `cargo test sub_style_options -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs vendor/libmpv"]
    fn sub_style_options_of_mpv() {
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        let style = SubStyle {
            font: "Arial".into(),
            scale: 150.0,
            color: [255, 255, 0],
            outline_color: [0, 0, 255],
            outline: 3.0,
            pos: 90.0,
            bold: true,
            shadow: 2.0,
            boxed: true,
        };
        player.set_sub_style(&style);
        let options = style.options();
        for (i, (name, _)) in options.iter().enumerate() {
            player
                .mpv
                .observe(i as u64 + 1, name, sys::MPV_FORMAT_STRING);
        }
        let mut seen = vec![None; options.len()];
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while seen.iter().any(Option::is_none) && std::time::Instant::now() < deadline {
            while let Some(event) = player.mpv.next_event() {
                if let Event::Property { id, value } = event
                    && let Some(slot) = seen.get_mut(id as usize - 1)
                {
                    *slot = Some(value);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for ((name, wanted), got) in options.iter().zip(&seen) {
            eprintln!("{name} = {got:?} (set {wanted})");
            assert!(
                matches!(got, Some(Value::Str(_))),
                "{name} is not a property: {got:?}"
            );
        }
        let text = |i: usize| match &seen[i] {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };
        assert_eq!(text(0), "Arial");
        assert!(text(1).starts_with("1.5"), "{}", text(1));
        assert!(text(4).starts_with("3."), "{}", text(4));
        assert!(text(5).starts_with("90"), "{}", text(5));
        // Colours come back with their alpha first.
        assert_eq!(text(2), "#FFFFFF00");
        // The box: the outline's colour, a little see-through.
        assert_eq!(text(3), "#C00000FF");
        assert_eq!(text(6), "yes");
        assert!(text(7).starts_with("2."), "{}", text(7));
        assert_eq!(text(8), "opaque-box");
    }

    /// The picture's adjustments on a real file without a window: mpv
    /// takes the colour, the rotation, the filters and the debanding, the
    /// filters run, and a frame is saved. `cargo test picture_adjustments
    /// -- --ignored --nocapture` with METALMPV_SAMPLE.
    /// How long the sound stops when the channels change: a real output
    /// (it plays aloud, quietly), mpv's verbose log with the time since
    /// the change. `cargo test channel_switch -- --ignored --nocapture`
    /// with METALMPV_SAMPLE and METALMPV_CHANNELS (default `mono`).
    #[test]
    #[ignore = "needs vendor/libmpv, a sample video and a sound card"]
    fn channel_switch_timing() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let to = std::env::var("METALMPV_CHANNELS").unwrap_or_else(|_| "mono".into());
        let api = sys::api().unwrap();
        // The app's options (METALMPV_APP_OPTIONS=1), the picture to null.
        let mut opts: Vec<(&str, &str)> = if std::env::var("METALMPV_APP_OPTIONS").is_ok() {
            options(true)
                .into_iter()
                .filter(|(n, _)| *n != "vo" && *n != "msg-level")
                .collect()
        } else {
            vec![("idle", "yes"), ("terminal", "no")]
        };
        opts.extend([
            ("vo", "null"),
            ("volume", "20"),
            ("audio-channels", "auto-safe"),
        ]);
        let mut mpv = Mpv::new(api, &opts).unwrap();
        if let Ok(buffer) = std::env::var("METALMPV_AUDIO_BUFFER") {
            mpv.set("audio-buffer", &Value::Str(buffer));
        }
        // METALMPV_NORMALIZE=1: with the normalization's filter, which
        // holds seconds of sound; METALMPV_RESYNC=1: seek to where the
        // picture is after the change, as the app does.
        if std::env::var("METALMPV_NORMALIZE").is_ok() {
            mpv.set("af", &Value::Str(audio::Normalize::Medium.filter().into()));
        }
        let resync = std::env::var("METALMPV_RESYNC").is_ok();
        mpv.request_log_messages("v");
        mpv.observe(1, "audio-out-params", sys::MPV_FORMAT_NODE);
        mpv.observe(2, "time-pos", sys::MPV_FORMAT_DOUBLE);
        mpv.command(0, &["loadfile", &sample]);
        let start = std::time::Instant::now();
        let mut switched: Option<std::time::Instant> = None;
        let wanted = |text: &str, prefix: &str| {
            ["ao", "ao/wasapi", "af", "cplayer", "ad"].contains(&prefix)
                && !text.contains("time-pos")
        };
        while start.elapsed() < std::time::Duration::from_secs(8) {
            if switched.is_none() && start.elapsed() > std::time::Duration::from_secs(3) {
                eprintln!("--- audio-channels={to}");
                mpv.set("audio-channels", &Value::Str(to.clone()));
                if resync {
                    mpv.command(0, &["seek", "0", "relative+exact"]);
                }
                switched = Some(std::time::Instant::now());
            }
            let ms = |t: Option<std::time::Instant>| {
                t.map_or(String::from("   -"), |t| {
                    format!("{:5.0}", t.elapsed().as_secs_f64() * 1000.0)
                })
            };
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::Log { prefix, text, .. }
                        if switched.is_some() && wanted(&text, &prefix) =>
                    {
                        eprintln!("{} ms [{prefix}] {text}", ms(switched));
                    }
                    Event::Property { id: 1, value } if switched.is_some() => {
                        eprintln!(
                            "{} ms audio-out-params {:?}",
                            ms(switched),
                            value.get("channels")
                        );
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// The panel's options mpv must take as given: each is set, read back
    /// and compared; the errors mpv logs fail it. `cargo test
    /// panel_options -- --ignored --nocapture` with METALMPV_SAMPLE.
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn panel_options_are_taken() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        mpv.request_log_messages("warn");
        let wanted: Vec<(&str, Value)> = vec![
            ("deinterlace", Value::Str("auto".into())),
            ("tone-mapping", Value::Str("bt.2390".into())),
            ("video-zoom", Value::Double(0.5)),
            ("video-pan-x", Value::Double(-0.1)),
            ("sub-ass-override", Value::Str("force".into())),
            ("sub-use-margins", Value::Flag(true)),
            ("sub-ass-force-margins", Value::Flag(true)),
        ];
        for (name, value) in &wanted {
            mpv.set(name, value);
        }
        // The voice's filter for the channels of the track playing, as the
        // app gives it once the track list is known.
        mpv.observe(100, "track-list", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &sample]);
        for (i, (name, value)) in wanted.iter().enumerate() {
            let format = match value {
                Value::Double(_) => sys::MPV_FORMAT_DOUBLE,
                Value::Flag(_) => sys::MPV_FORMAT_FLAG,
                _ => sys::MPV_FORMAT_STRING,
            };
            mpv.get(i as u64 + 1, name, format);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
        let mut problems = Vec::new();
        let mut af_set = false;
        while std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::Property { id: 100, value } if !af_set => {
                        let tracks = tracks::from_value(&value);
                        if let Some(n) = tracks::selected(&tracks, Kind::Audio)
                            .and_then(|t| audio::voice_layout(t.layout.as_deref(), t.channels))
                        {
                            let af = audio::filters(
                                None,
                                &audio::Equalizer::default(),
                                Some((&n, 7)),
                                None,
                            );
                            eprintln!("{n}, af: {af}");
                            mpv.set("af", &Value::Str(af));
                            af_set = true;
                        }
                    }
                    Event::Property { id, value } if id >= 1 && id as usize <= wanted.len() => {
                        let (name, want) = &wanted[id as usize - 1];
                        eprintln!("{name}: {value:?}");
                        let near = matches!((&value, want), (Value::Double(a), Value::Double(b)) if (a - b).abs() < 1e-6);
                        if value != *want && !near {
                            problems.push(format!("{name}: {value:?}, not {want:?}"));
                        }
                    }
                    // A file's own warnings are not the options'.
                    Event::Log { level, text, .. } if level == "error" || level == "fatal" => {
                        eprintln!("mpv {level}: {text}");
                        problems.push(text);
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(problems.is_empty(), "{problems:#?}");
    }

    /// Stereo spread over 5.1 comes out as six channels, the voice raised
    /// in the centre after it: a stereo WAV made here, played headless.
    /// `cargo test sound_filters -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs vendor/libmpv"]
    fn sound_filters_upmix_to_six_channels() {
        // The subwoofer's options other than FFmpeg's defaults: taken too.
        let upmix = audio::Upmix {
            lfe_cut: 120,
            bass_only: true,
            lfe_level: -6,
        };
        let af = audio::filters(
            Some(audio::Normalize::Strong),
            &audio::Equalizer::default(),
            Some(("stereo", 7)),
            Some(upmix),
        );
        assert_eq!(channels_out(af), Some(6));
    }

    /// Stereo's voice filter at its strongest and the soft normalization
    /// are taken.
    #[test]
    #[ignore = "needs vendor/libmpv"]
    fn sound_filters_of_stereo() {
        let af = audio::filters(
            Some(audio::Normalize::Soft),
            &audio::Equalizer::default(),
            Some(("stereo", *audio::VOICE_RANGE.end())),
            None,
        );
        assert!(channels_out(af).is_some());
    }

    /// The channels a stereo WAV made here comes out with through `af`,
    /// played headless; mpv's errors fail the test.
    fn channels_out(af: String) -> Option<i64> {
        let dir = std::env::temp_dir().join("metalmpv_upmix");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("stereo.wav");
        std::fs::write(&wav, stereo_wav(48_000, 2)).unwrap();
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("audio-channels", Channels::Auto.option()),
            ],
        )
        .unwrap();
        mpv.request_log_messages("warn");
        eprintln!("af: {af}");
        mpv.set("af", &Value::Str(af));
        mpv.observe(1, "audio-out-params", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &wav.to_string_lossy()]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut count, mut errors) = (None, Vec::new());
        while count.is_none() && std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::Property { id: 1, value } => {
                        count = value.get("channel-count").and_then(Value::int);
                        eprintln!("out: {value:?}");
                    }
                    Event::Log { level, text, .. } if level == "error" || level == "fatal" => {
                        errors.push(text);
                    }
                    _ => {}
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(errors.is_empty(), "{errors:#?}");
        count
    }

    /// `seconds` of a WAV, 16-bit stereo at `rate`: a different tone on
    /// each side and one common to both.
    fn stereo_wav(rate: u32, seconds: u32) -> Vec<u8> {
        let frames = rate * seconds;
        let data = frames * 4;
        let mut out = Vec::with_capacity(44 + data as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data.to_le_bytes());
        for i in 0..frames {
            let t = f64::from(i) / f64::from(rate);
            let tone = |hz: f64| (t * hz * std::f64::consts::TAU).sin();
            let common = tone(300.0);
            for side in [tone(440.0), tone(660.0)] {
                let sample = ((common + side) * 0.4 * f64::from(i16::MAX)) as i16;
                out.extend_from_slice(&sample.to_le_bytes());
            }
        }
        out
    }

    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn picture_adjustments_of_a_real_file() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let api = sys::api().unwrap();
        let mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
                ("pause", "yes"),
                // As the app: the filters must make it copy the frames.
                ("hwdec", "auto-safe"),
            ],
        )
        .unwrap();
        mpv.request_log_messages("warn");
        let mut player = Player {
            mpv,
            state: State::default(),
            scan_asked: None,
            live_asked: None,
            loading: false,
            starts_due: 0,
            stopped: false,
            hwdec: true,
            hwdec_copy: false,
            filters_on: false,
            subs_off: false,
            last_seek: None,
            data_end_at: None,
            ran_out_at: None,
            af: String::new(),
            af_before: String::new(),
        };
        let mut new = Adjust::default();
        new.set(Color::Brightness, 10);
        new.set(Color::Gamma, -20);
        new.turn(1);
        new.flip = true;
        new.sharpen = 60;
        new.denoise = 70;
        new.deband = true;
        new.aspect = crate::adjust::Aspect::R4x3;
        player.adjust(&Adjust::default(), &new);
        player.mpv.observe(1, "vf", sys::MPV_FORMAT_NODE);
        player
            .mpv
            .observe(2, "video-out-params", sys::MPV_FORMAT_NODE);
        player.mpv.observe(3, "brightness", sys::MPV_FORMAT_INT64);
        player
            .mpv
            .observe(4, "hwdec-current", sys::MPV_FORMAT_STRING);
        player.mpv.command(0, &["loadfile", &sample]);
        let shot = std::env::temp_dir().join(format!("metalmpv_frame_{}.png", std::process::id()));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut out, mut errors, mut shot_asked) = (None, Vec::new(), false);
        while !(out.is_some() && shot.exists()) && std::time::Instant::now() < deadline {
            while let Some(event) = player.mpv.next_event() {
                match event {
                    Event::Property { id: 1, value } => eprintln!("vf: {value:?}"),
                    Event::Property { id: 2, value } if value.get("w").is_some() => {
                        eprintln!("out: {value:?}");
                        out = Some(value);
                    }
                    Event::Property { id: 3, value } => eprintln!("brightness: {value:?}"),
                    Event::Property { id: 4, value } => eprintln!("hwdec: {value:?}"),
                    Event::Log { prefix, text, .. } => {
                        eprint!("[{prefix}] {text}");
                        errors.push(text);
                    }
                    _ => {}
                }
            }
            if out.is_some() && !shot_asked {
                player.save_frame(&shot);
                shot_asked = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(out.is_some(), "no picture came out of the filters");
        assert!(shot.exists(), "no frame saved");
        let _ = std::fs::remove_file(&shot);
        assert!(
            !errors
                .iter()
                .any(|e| e.contains("filter") || e.contains("Disabling")),
            "{errors:?}"
        );
    }

    /// Reads the chapters of a real file without a window:
    /// `cargo test chapter_list -- --ignored --nocapture` with a file that
    /// has chapters in METALMPV_SAMPLE.
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video with chapters"]
    fn chapter_list_of_a_real_file() {
        let Ok(sample) = std::env::var("METALMPV_SAMPLE") else {
            panic!("METALMPV_SAMPLE is not set");
        };
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("terminal", "no"),
            ],
        )
        .unwrap();
        mpv.observe(1, "chapter-list", sys::MPV_FORMAT_NODE);
        mpv.command(0, &["loadfile", &sample]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut found = Vec::new();
        while found.is_empty() && std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                if let Event::Property { id: 1, value } = event {
                    found = chapters::from_value(&value);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        for (i, c) in found.iter().enumerate() {
            eprintln!(
                "{} {}",
                crate::format::time(Some(c.time), true),
                c.name_in(crate::i18n::Lang::Ru, i + 1)
            );
        }
        assert!(!found.is_empty());
    }
}
