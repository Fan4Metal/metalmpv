//! The player: an mpv core configured for playback in the app's window, the
//! state the interface shows (kept up to date from observed properties) and
//! the commands of the interface. No OpenGL here: the picture is
//! `video.rs`'s.
//!
//! Every command is asynchronous (see `mpv/mod.rs`): the UI thread never
//! waits for the core, and the core works through the commands in the order
//! they were given, so a volume set before `loadfile` applies to that file.

use std::path::Path;

use crate::chapters::{self, Chapter};
use crate::mpv::{EndReason, Event, Mpv, Value, sys};
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
    use std::sync::OnceLock;
    static FORCED: OnceLock<Option<&'static str>> = OnceLock::new();
    let forced = FORCED.get_or_init(|| {
        std::env::var("METALMPV_HWDEC")
            .ok()
            .filter(|v| !v.is_empty())
            .map(|v| &*Box::leak(v.into_boxed_str()))
    });
    forced.unwrap_or(if on { "auto-safe" } else { "no" })
}

/// Start an mpv core with [`options`]. Slow enough (loading the DLL, the
/// core's threads, audio) to be run on a thread of its own while the window
/// is being made.
pub fn boot(hwdec: bool) -> Result<Mpv, String> {
    let api = sys::api().map_err(str::to_owned)?;
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
    let mpv = Mpv::new(api, &options)?;
    Ok(mpv)
}

/// Observed properties, by their number in [`Event::Property`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
enum Prop {
    Pause = 1,
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
}

const PROPS: [(Prop, &str, sys::mpv_format); 20] = [
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
];

/// The reply to [`Player::ask_scan`]'s `video-frame-info`, past the
/// observed properties' numbers.
const FRAME_INFO: u64 = 100;

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
    /// 0 to 100.
    pub volume: f64,
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
        };
        (params != Self::default()).then_some(params)
    }
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
    /// The file could not be played.
    Failed(String),
    /// A seek finished: the position is the new one.
    Restarted,
    Shutdown,
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
}

impl Player {
    /// Take a started core: observe what the interface shows and wake the
    /// UI when events wait. `volume` and `mute` come from the settings.
    pub fn new(mut mpv: Mpv, ctx: &egui::Context, volume: f64, mute: bool) -> Self {
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
        for (prop, name, format) in PROPS {
            mpv.observe(prop as u64, name, format);
        }
        let player = Self {
            mpv,
            state: State {
                volume,
                mute,
                ..State::default()
            },
            scan_asked: None,
        };
        player
            .mpv
            .set("volume", &Value::Double(volume.clamp(0.0, 100.0)));
        player.mpv.set("mute", &Value::Flag(mute));
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
                } => self.state.scan = scan_of(&value),
                Event::Property { id, value } => {
                    if let Some((prop, _, _)) = PROPS.iter().find(|(p, _, _)| *p as u64 == id) {
                        let was_eof = self.state.eof;
                        self.state.apply(*prop, &value);
                        if *prop == Prop::EofReached && self.state.eof && !was_eof {
                            out.push(Happened::Eof);
                        }
                    }
                }
                Event::FileLoaded => {
                    self.state.scan = None;
                    self.scan_asked = None;
                    out.push(Happened::FileLoaded);
                }
                Event::PlaybackRestart => out.push(Happened::Restarted),
                Event::EndFile {
                    reason: EndReason::Error,
                    error,
                } => {
                    out.push(Happened::Failed(
                        error.unwrap_or_else(|| "unknown error".into()),
                    ));
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
                    for (prop, name, format) in PROPS {
                        self.mpv.get(prop as u64, name, format);
                    }
                }
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

    /// Play `path` from the start, replacing the current file.
    pub fn load(&mut self, path: &Path) {
        self.mpv.set("pause", &Value::Flag(false));
        self.mpv
            .command(0, &["loadfile", &path.to_string_lossy(), "replace"]);
        self.state.paused = false;
        self.state.eof = false;
    }

    /// Pause or go on; at the end of the file, play it again from the start.
    pub fn toggle_pause(&mut self) {
        if self.state.idle {
            return;
        }
        if self.state.eof {
            self.seek_to(0.0, Seek::Keyframe);
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
    }

    /// Move by `seconds` (negative: back).
    pub fn seek_by(&mut self, seconds: f64, how: Seek) {
        if self.state.idle {
            return;
        }
        let flags = format!("relative+{}", how.flag());
        self.mpv
            .command(0, &["seek", &format!("{seconds:.3}"), &flags]);
    }

    /// Move to `seconds` from the start.
    pub fn seek_to(&mut self, seconds: f64, how: Seek) {
        if self.state.idle {
            return;
        }
        let flags = format!("absolute+{}", how.flag());
        self.mpv
            .command(0, &["seek", &format!("{:.3}", seconds.max(0.0)), &flags]);
        self.state.time = Some(seconds.max(0.0));
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

    /// Volume, 0 to 100; unmutes.
    pub fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 100.0);
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

    /// Hardware decoding on (`auto-safe`) or off; mpv applies it to the
    /// playing file at once.
    pub fn set_hwdec(&mut self, on: bool) {
        self.mpv.set("hwdec", &Value::Str(hwdec_option(on).into()));
    }

    /// The font family of text subtitles (empty: mpv's, `sans-serif`) and
    /// the size of all subtitles in percent. ASS subtitles keep their
    /// own fonts.
    pub fn set_sub_style(&mut self, font: &str, scale: f64) {
        let font = if font.is_empty() { "sans-serif" } else { font };
        self.mpv.set("sub-font", &Value::Str(font.into()));
        self.mpv.set("sub-scale", &Value::Double(scale / 100.0));
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
    fn boot_without_the_dll_fails_cleanly() {
        // Test builds run from target\debug\deps; the DLL may be found in
        // vendor\libmpv, in which case the core starts.
        match boot(false) {
            Ok(_) => {}
            Err(e) => assert!(e.contains(sys::DLL_NAME), "{e}"),
        }
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
