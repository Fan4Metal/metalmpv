//! The video map: frames spread over a file in one picture, a grid of them
//! under a line of what the file is, each with its time, to show a model
//! (or a person) what the video holds at a glance.
//!
//! A second mpv core without a window or sound (`vo=null`, paused) on a
//! thread of its own, as `clip.rs`'s, seeks exactly to each frame's time
//! and gives the frame with `screenshot-raw` (RGBA bytes); the frames are
//! scaled to the tiles here (by the display size, so an anamorphic picture
//! has its shape, and turned as the file says), the text drawn with
//! skrifa's outlines filled by vello_cpu (both egui's already), the whole
//! written as a JPG or PNG.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use image::RgbaImage;
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use vello_cpu::kurbo::{self, Shape};

use crate::mpv::{EndReason, Event, Mpv, Value, sys};
use crate::settings::FrameFormat;

/// The ranges of [`Layout`]'s columns and rows.
pub const COLUMNS: std::ops::RangeInclusive<u32> = 2..=8;
pub const ROWS: std::ops::RangeInclusive<u32> = 1..=10;
/// The widths offered, in pixels: 1536 is about the most a model takes in
/// without scaling the picture down (Claude's 1568, GPT's 1536 or 2048).
pub const WIDTHS: [u32; 6] = [1024, 1280, 1536, 1920, 2560, 3840];

/// How the map is laid out, kept as the setting `video_map`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub columns: u32,
    pub rows: u32,
    /// The picture's width in pixels.
    pub width: u32,
    /// A line over the frames: the name, the duration, the picture, the size.
    pub header: bool,
    pub format: FrameFormat,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            columns: 4,
            rows: 4,
            width: 1536,
            header: true,
            format: FrameFormat::Jpg,
        }
    }
}

impl Layout {
    pub fn frames(self) -> u32 {
        self.columns * self.rows
    }

    /// The layout for a picture of `shown` display size: an upright one
    /// in twice the columns and half the rows, else the map would be
    /// twice as tall as wide (models scale such a picture down).
    fn for_shape(self, shown: (u32, u32)) -> Layout {
        if shown.1 <= shown.0 {
            return self;
        }
        let frames = self.frames();
        let columns = (self.columns * 2).min(frames);
        Layout {
            columns,
            rows: frames.div_ceil(columns),
            ..self
        }
    }

    /// `4|4|1536|info|jpg`; `-` for no header.
    pub fn to_text(self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.columns,
            self.rows,
            self.width,
            if self.header { "info" } else { "-" },
            self.format.name()
        )
    }

    /// What [`Layout::to_text`] wrote; the default for what is not understood.
    pub fn from_text(text: &str) -> Self {
        let d = Self::default();
        let f: Vec<&str> = text.split('|').collect();
        let [columns, rows, width, header, format] = f[..] else {
            return d;
        };
        let within = |v: &str, r: std::ops::RangeInclusive<u32>, d: u32| {
            v.parse().ok().filter(|v| r.contains(v)).unwrap_or(d)
        };
        Self {
            columns: within(columns, COLUMNS, d.columns),
            rows: within(rows, ROWS, d.rows),
            width: width
                .parse()
                .ok()
                .filter(|w| WIDTHS.contains(w))
                .unwrap_or(d.width),
            header: header != "-",
            format: FrameFormat::from_name(format).unwrap_or(d.format),
        }
    }
}

/// The times of `count` frames of a file of `duration` seconds: the middles
/// of equal parts, so neither the first black frame nor the credits' last.
pub fn times(duration: f64, count: u32) -> Vec<f64> {
    (0..count)
        .map(|i| duration * (f64::from(i) + 0.5) / f64::from(count))
        .collect()
}

/// What to make: a map of `input` laid out as `layout`, written to `output`;
/// `title` heads it (a channel's name, else the file's).
#[derive(Clone, Debug)]
pub struct Request {
    pub input: PathBuf,
    pub title: String,
    pub layout: Layout,
    pub output: PathBuf,
    /// mpv's `hwdec`: `no` or `auto-copy-safe` (the frames come back for
    /// the processor).
    pub hwdec: &'static str,
}

/// A map made: where it was written and the picture (for the clipboard).
pub struct Made {
    pub output: PathBuf,
    pub image: RgbaImage,
}

/// A map being made.
pub struct Job {
    pub frames: u32,
    done_frames: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    done: Receiver<Result<Made, String>>,
}

impl Job {
    /// Start making it; `wake` is called after each frame and at the end.
    pub fn start(request: Request, wake: impl Fn() + Send + 'static) -> Result<Job, String> {
        let done_frames = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, done) = channel();
        let frames = request.layout.frames();
        let (p, c) = (done_frames.clone(), cancel.clone());
        crate::worker::spawn("video map", move || {
            let result = crate::worker::guarded(|| make(&request, &p, &c, &wake))
                .unwrap_or_else(|| Err("the map failed".into()));
            let _ = send.send(result);
            wake();
        })
        .map_err(|e| e.to_string())?;
        Ok(Job {
            frames,
            done_frames,
            cancel,
            done,
        })
    }

    /// The frames taken so far.
    pub fn progress(&self) -> u32 {
        self.done_frames.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// How it ended, once it has.
    pub fn take(&self) -> Option<Result<Made, String>> {
        match self.done.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the job ended".into())),
        }
    }
}

/// Make the map on this thread and write it; `progress` counts the frames.
pub fn make(
    request: &Request,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    wake: &dyn Fn(),
) -> Result<Made, String> {
    let started = Instant::now();
    let layout = request.layout;
    let grabbed = grab(request, progress, cancel, wake)?;
    let image = compose(&request.title, &grabbed);
    write(&image, &request.output, layout.format)?;
    log::info!(
        "video map of {} ({}×{} frames) in {:.0} ms: {}",
        request.input.display(),
        layout.columns,
        layout.rows,
        started.elapsed().as_secs_f64() * 1e3,
        request.output.display()
    );
    Ok(Made {
        output: request.output.clone(),
        image,
    })
}

/// `metalmpv --map`: the maps of the files named, each beside its file
/// unless `-o` names the picture (one file) or a folder; their paths on
/// stdout, one a line. The exit code: 0, 1 when a map failed, 2 for
/// arguments not understood.
pub fn command_line(args: impl Iterator<Item = std::ffi::OsString>) -> i32 {
    use std::io::Write;
    const USAGE: &str =
        "metalmpv --map VIDEO... [-o FILE|FOLDER] [--grid 4x4] [--width 1536] [--png] [--no-info]";
    let mut layout = Layout::default();
    let mut inputs = Vec::new();
    let mut out: Option<PathBuf> = None;
    let mut args = args.peekable();
    let bad = |what: &str| {
        let _ = writeln!(std::io::stderr(), "{what}\nusage: {USAGE}");
        2
    };
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-o" | "--out") => match args.next() {
                Some(o) => out = Some(PathBuf::from(o)),
                None => return bad("-o needs a path"),
            },
            Some("--grid") => {
                let grid = args.next().map(|g| g.to_string_lossy().to_lowercase());
                let parsed = grid.as_deref().and_then(|g| {
                    let (c, r) = g.split_once('x')?;
                    Some((c.parse().ok()?, r.parse().ok()?))
                });
                match parsed {
                    Some((c, r)) if COLUMNS.contains(&c) && ROWS.contains(&r) => {
                        layout.columns = c;
                        layout.rows = r;
                    }
                    _ => return bad("--grid takes COLUMNSxROWS, 2-8 by 1-10"),
                }
            }
            Some("--width") => match args.next().and_then(|w| w.to_str()?.parse::<u32>().ok()) {
                Some(w) if (640..=7680).contains(&w) => layout.width = w,
                _ => return bad("--width takes 640 to 7680 pixels"),
            },
            Some("--png") => layout.format = FrameFormat::Png,
            Some("--jpg") => layout.format = FrameFormat::Jpg,
            Some("--no-info") => layout.header = false,
            Some(flag) if flag.starts_with("--") => return bad(&format!("unknown option {flag}")),
            _ => inputs.push(PathBuf::from(arg)),
        }
    }
    if inputs.is_empty() {
        return bad("no video named");
    }
    let to_file = out
        .as_ref()
        .is_some_and(|o| o.extension().is_some() && !o.is_dir());
    if to_file && inputs.len() > 1 {
        return bad("-o names one picture, but several videos were named");
    }
    let mut code = 0;
    for input in inputs {
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "video".into());
        let output = match &out {
            Some(o) if to_file => {
                let png = o.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"));
                layout.format = if png {
                    FrameFormat::Png
                } else {
                    FrameFormat::Jpg
                };
                o.clone()
            }
            Some(folder) => file_name(folder, &stem, layout.format),
            None => file_name(
                input
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
                &stem,
                layout.format,
            ),
        };
        let request = Request {
            title: input
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            input: input.clone(),
            layout,
            output,
            hwdec: "auto-copy-safe",
        };
        match make(
            &request,
            &AtomicU32::new(0),
            &AtomicBool::new(false),
            &|| {},
        ) {
            Ok(made) => {
                let _ = writeln!(std::io::stdout(), "{}", made.output.display());
            }
            Err(e) => {
                let _ = writeln!(std::io::stderr(), "{}: {e}", input.display());
                code = 1;
            }
        }
    }
    code
}

/// A free name for the map of `input` in `folder`: `Film map.jpg`, else
/// `Film map (2).jpg`…
pub fn file_name(folder: &Path, stem: &str, format: FrameFormat) -> PathBuf {
    let ext = format.name();
    (1..)
        .map(|n| {
            folder.join(if n == 1 {
                format!("{stem} map.{ext}")
            } else {
                format!("{stem} map ({n}).{ext}")
            })
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// What the file is, for the header.
#[derive(Clone, Debug, Default)]
struct About {
    duration: f64,
    /// The picture's display size, turned as shown.
    size: Option<(u32, u32)>,
    codec: Option<String>,
    fps: Option<f64>,
    bytes: Option<i64>,
}

/// The frames taken (a `None` for one that could not be), in their
/// tiles' size, with their times.
struct Grabbed {
    about: About,
    /// The layout for the picture's shape.
    layout: Layout,
    tile: (u32, u32),
    frames: Vec<(f64, Option<RgbaImage>)>,
}

/// The picture's parameters that place a frame in its tile.
#[derive(Clone, Copy, Debug)]
struct Params {
    /// The display size, before the turn.
    dw: u32,
    dh: u32,
    /// Clockwise degrees.
    rotate: u32,
}

impl Params {
    fn of(value: &Value) -> Option<Params> {
        let int = |k: &str| value.get(k)?.int().filter(|&v| v > 0);
        Some(Params {
            dw: int("dw").or_else(|| int("w"))? as u32,
            dh: int("dh").or_else(|| int("h"))? as u32,
            rotate: value
                .get("rotate")
                .and_then(Value::int)
                .unwrap_or(0)
                .rem_euclid(360) as u32,
        })
    }

    fn turned(self) -> bool {
        self.rotate == 90 || self.rotate == 270
    }

    /// The display size as shown.
    fn shown(self) -> (u32, u32) {
        if self.turned() {
            (self.dh, self.dw)
        } else {
            (self.dw, self.dh)
        }
    }
}

const LOAD: u64 = 1;
const DURATION: u64 = 2;
const PARAMS: u64 = 3;
const SEEK: u64 = 4;
const SHOT: u64 = 5;
const CODEC: u64 = 6;
const FPS: u64 = 7;
const BYTES: u64 = 8;

/// How long mpv may stay silent (a frame of a broken file, a share gone).
const STALL: Duration = Duration::from_secs(20);

/// The space between the tiles and round them, a share of the width.
fn gap(width: u32) -> u32 {
    (width / 256).max(4)
}

/// The tiles' size for a picture of `shown` display size.
fn tile_size(layout: Layout, shown: (u32, u32)) -> (u32, u32) {
    let g = gap(layout.width);
    let w = (layout.width - g * (layout.columns + 1)) / layout.columns;
    let h = (f64::from(w) * f64::from(shown.1) / f64::from(shown.0.max(1))).round();
    (w, (h as u32).clamp(8, w * 4))
}

/// Take the frames with a second core.
fn grab(
    request: &Request,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    wake: &dyn Fn(),
) -> Result<Grabbed, String> {
    let api = sys::api().map_err(|e| e.to_string())?;
    let options = [
        ("terminal", "no"),
        ("config", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("input-default-bindings", "no"),
        ("vo", "null"),
        ("ao", "null"),
        ("aid", "no"),
        ("sid", "no"),
        ("pause", "yes"),
        ("keep-open", "always"),
        ("idle", "yes"),
        ("hr-seek", "yes"),
        ("hwdec", request.hwdec),
        // Nothing read ahead past the frame wanted.
        ("cache", "no"),
    ];
    let mut mpv = Mpv::new(api, &options)?;
    mpv.request_log_messages("warn");
    let input = request.input.to_string_lossy();
    mpv.command(LOAD, &["loadfile", &input, "replace"]);

    let layout = request.layout;
    let mut about = About::default();
    let mut params: Option<Params> = None;
    let mut times: Vec<f64> = Vec::new();
    let mut frames: Vec<(f64, Option<RgbaImage>)> = Vec::new();
    // Waiting for: the first frame shown; then each seek's frame.
    let mut shown_first = false;
    let mut seeking = false;
    let mut seek_done = false;
    let mut last_event = Instant::now();

    let next = |mpv: &Mpv, frames: &[(f64, Option<RgbaImage>)], times: &[f64]| {
        if let Some(&t) = times.get(frames.len()) {
            mpv.command(SEEK, &["seek", &format!("{t:.3}"), "absolute+exact"]);
        }
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if last_event.elapsed() > STALL {
            return Err("mpv gave no frame".into());
        }
        let Some(event) = mpv.next_event() else {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        };
        last_event = Instant::now();
        match event {
            Event::CommandReply {
                id: LOAD,
                error: Some(e),
                ..
            } => return Err(e),
            Event::EndFile {
                reason: EndReason::Error,
                error,
                ..
            } => return Err(error.unwrap_or_else(|| "cannot open".into())),
            Event::EndFile { reason, .. } if reason != EndReason::Redirect => {
                return Err(format!("the file ended ({reason:?})"));
            }
            Event::FileLoaded => {
                mpv.get(DURATION, "duration", sys::MPV_FORMAT_DOUBLE);
                mpv.get(CODEC, "video-format", sys::MPV_FORMAT_STRING);
                mpv.get(FPS, "container-fps", sys::MPV_FORMAT_DOUBLE);
                mpv.get(BYTES, "file-size", sys::MPV_FORMAT_INT64);
            }
            Event::Property { id, value } => match id {
                DURATION => {
                    let duration = value.double().filter(|d| d.is_finite() && *d > 0.0);
                    let Some(duration) = duration else {
                        return Err("the file has no duration".into());
                    };
                    about.duration = duration;
                    times = self::times(duration, layout.frames());
                }
                CODEC => about.codec = value.string().map(codec_name),
                FPS => about.fps = value.double().filter(|f| *f > 0.0),
                BYTES => about.bytes = value.int().filter(|b| *b > 0),
                PARAMS => {
                    let Some(p) = Params::of(&value) else {
                        return Err("the file has no picture".into());
                    };
                    params = Some(p);
                    about.size = Some(p.shown());
                    if times.is_empty() {
                        return Err("the file has no duration".into());
                    }
                    next(&mpv, &frames, &times);
                }
                _ => {}
            },
            Event::PlaybackRestart if !shown_first => {
                // The first frame is decoded: the picture's size is known.
                shown_first = true;
                mpv.get(PARAMS, "video-params", sys::MPV_FORMAT_NODE);
            }
            Event::CommandReply {
                id: SEEK, error, ..
            } => {
                if let Some(e) = error {
                    log::warn!("video map: seek to frame {}: {e}", frames.len() + 1);
                    frames.push((times[frames.len()], None));
                    next(&mpv, &frames, &times);
                } else {
                    seeking = true;
                    seek_done = false;
                }
            }
            Event::Seek if seeking => seek_done = true,
            Event::PlaybackRestart if seeking && seek_done => {
                seeking = false;
                mpv.command(SHOT, &["screenshot-raw", "video", "rgba"]);
            }
            Event::CommandReply {
                id: SHOT,
                error,
                result,
            } => {
                let t = times[frames.len()];
                let p = params.expect("params before the frames");
                let frame = match error {
                    Some(e) => {
                        log::warn!("video map: frame at {t:.1} s: {e}");
                        None
                    }
                    None => fitted(
                        &result,
                        p,
                        tile_size(layout.for_shape(p.shown()), p.shown()),
                    ),
                };
                frames.push((t, frame));
                progress.store(frames.len() as u32, Ordering::Relaxed);
                wake();
                if frames.len() == times.len() {
                    break;
                }
                next(&mpv, &frames, &times);
            }
            Event::Log {
                prefix,
                level,
                text,
            } => log::info!("video map mpv {level} [{prefix}] {text}"),
            Event::Shutdown => return Err("the core stopped".into()),
            _ => {}
        }
    }
    drop(mpv);
    if frames.iter().all(|(_, f)| f.is_none()) {
        return Err("no frame could be taken".into());
    }
    let p = params.expect("params");
    Ok(Grabbed {
        about,
        layout: layout.for_shape(p.shown()),
        tile: tile_size(layout.for_shape(p.shown()), p.shown()),
        frames,
    })
}

/// `screenshot-raw`'s answer (`w`, `h`, `stride`, `format`, `data`) scaled
/// to `tile` and turned as shown.
fn fitted(shot: &Value, p: Params, tile: (u32, u32)) -> Option<RgbaImage> {
    let int = |k: &str| shot.get(k)?.int().filter(|&v| v > 0);
    let (w, h, stride) = (int("w")? as u32, int("h")? as u32, int("stride")? as usize);
    let Some(Value::Bytes(data)) = shot.get("data") else {
        return None;
    };
    let row = w as usize * 4;
    if stride < row || data.len() < stride * (h as usize - 1) + row {
        return None;
    }
    let bgr = shot.get("format").and_then(Value::string) != Some("rgba");
    let mut pixels = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        pixels.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    for px in pixels.as_chunks_mut::<4>().0 {
        if bgr {
            px.swap(0, 2);
        }
        px[3] = 255;
    }
    let frame = RgbaImage::from_raw(w, h, pixels)?;
    // mpv gives the frame as decoded, before the turn; the tile is the
    // display size, so scaling to it also gives an anamorphic picture its
    // shape. Should mpv have turned it already, it is only scaled.
    let turned_already = p.turned() && (w > h) != (p.dw > p.dh);
    let (tw, th) = if p.turned() && !turned_already {
        (tile.1, tile.0)
    } else {
        tile
    };
    let scaled = image::imageops::resize(&frame, tw, th, image::imageops::FilterType::Triangle);
    Some(match p.rotate {
        _ if turned_already => scaled,
        90 => image::imageops::rotate90(&scaled),
        180 => image::imageops::rotate180(&scaled),
        270 => image::imageops::rotate270(&scaled),
        _ => scaled,
    })
}

/// mpv's `video-format` as people name it.
fn codec_name(format: &str) -> String {
    match format {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp8" => "VP8".into(),
        "vp9" => "VP9".into(),
        "mpeg2video" => "MPEG-2".into(),
        "mpeg4" => "MPEG-4".into(),
        "vc1" => "VC-1".into(),
        "prores" => "ProRes".into(),
        other => other.to_uppercase(),
    }
}

/// The line under the title: duration, picture, codec, frame rate, size.
fn details(about: &About) -> String {
    let mut parts = vec![crate::format::time(Some(about.duration), false)];
    if let Some((w, h)) = about.size {
        parts.push(format!("{w}×{h}"));
    }
    parts.extend(about.codec.clone());
    if let Some(fps) = about.fps {
        let fps = format!("{fps:.3}");
        parts.push(format!(
            "{} fps",
            fps.trim_end_matches('0').trim_end_matches('.')
        ));
    }
    if let Some(bytes) = about.bytes {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        parts.push(if mb >= 1024.0 {
            format!("{:.2} GB", mb / 1024.0)
        } else {
            format!("{mb:.1} MB")
        });
    }
    parts.join("  ·  ")
}

const BACKGROUND: [u8; 4] = [20, 20, 22, 255];
const MISSING: [u8; 4] = [44, 44, 48, 255];

/// The picture: the header, then the frames in rows with their times.
fn compose(title: &str, grabbed: &Grabbed) -> RgbaImage {
    let layout = grabbed.layout;
    let width = layout.width;
    let g = gap(width);
    let (tw, th) = grabbed.tile;
    let title_px = (width as f32 / 52.0).max(14.0);
    let line_px = (width as f32 / 76.0).max(11.0);
    let header = if layout.header {
        (g as f32 * 2.0 + title_px * 1.35 + line_px * 1.5).round() as u32
    } else {
        0
    };
    let rows = (grabbed.frames.len() as u32).div_ceil(layout.columns);
    let height = header + g + rows * (th + g);
    let mut canvas = RgbaImage::from_pixel(width, height, image::Rgba(BACKGROUND));
    let place = |i: u32| {
        let (c, r) = (i % layout.columns, i / layout.columns);
        (g + c * (tw + g), header + g + r * (th + g))
    };
    for (i, (_, frame)) in grabbed.frames.iter().enumerate() {
        let (x, y) = place(i as u32);
        match frame {
            Some(f) => image::imageops::replace(&mut canvas, f, i64::from(x), i64::from(y)),
            None => {
                let blank = RgbaImage::from_pixel(tw, th, image::Rgba(MISSING));
                image::imageops::replace(&mut canvas, &blank, i64::from(x), i64::from(y));
            }
        }
    }

    let mut text = Text::new(width, height);
    if layout.header {
        let x = g as f64;
        let room = f64::from(width - 2 * g);
        let baseline = g as f64 + f64::from(title_px) * 1.05;
        text.line(
            title,
            x,
            baseline,
            title_px,
            room,
            [255, 255, 255, 255],
            true,
        );
        let baseline = baseline + f64::from(line_px) * 1.55;
        let details = details(&grabbed.about);
        text.line(
            &details,
            x,
            baseline,
            line_px,
            room,
            [170, 170, 178, 255],
            false,
        );
    }
    // The times at each frame's bottom right, white on a dark ground.
    let long = grabbed.about.duration >= 3600.0;
    let time_px = (tw as f32 / 13.0).clamp(11.0, 34.0);
    for (i, (t, _)) in grabbed.frames.iter().enumerate() {
        let (x, y) = place(i as u32);
        let label = crate::format::time(Some(*t), long);
        let w = text.width(&label, time_px, true);
        let pad = f64::from(time_px) * 0.35;
        let inset = f64::from(time_px) * 0.3;
        let x1 = f64::from(x + tw) - inset;
        let y1 = f64::from(y + th) - inset;
        let x0 = x1 - w - 2.0 * pad;
        let y0 = y1 - f64::from(time_px) * 1.3;
        text.rect(x0, y0, x1, y1, f64::from(time_px) * 0.25, [0, 0, 0, 170]);
        let baseline = y1 - f64::from(time_px) * 0.33;
        text.line(
            &label,
            x0 + pad,
            baseline,
            time_px,
            w + 1.0,
            [255, 255, 255, 255],
            true,
        );
    }
    text.draw_onto(&mut canvas);
    canvas
}

/// Write `image` as `format`; nothing is left of a file not written whole.
fn write(image: &RgbaImage, path: &Path, format: FrameFormat) -> Result<(), String> {
    use image::ImageEncoder;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    let (w, h) = image.dimensions();
    let result = match format {
        FrameFormat::Jpg => {
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).into_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).write_image(
                rgb.as_raw(),
                w,
                h,
                image::ExtendedColorType::Rgb8,
            )
        }
        FrameFormat::Png => image::codecs::png::PngEncoder::new(&mut out).write_image(
            image.as_raw(),
            w,
            h,
            image::ExtendedColorType::Rgba8,
        ),
    }
    .map_err(|e| e.to_string())
    .and_then(|()| std::io::Write::flush(&mut out).map_err(|e| e.to_string()));
    drop(out);
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}

/// A font's bytes and its face's number in a collection.
struct Face {
    data: Vec<u8>,
    index: u32,
}

impl Face {
    fn read(path: &Path, index: u32) -> Option<Face> {
        let data = std::fs::read(path).ok()?;
        skrifa::FontRef::from_index(&data, index).ok()?;
        Some(Face { data, index })
    }

    fn font(&self) -> skrifa::FontRef<'_> {
        skrifa::FontRef::from_index(&self.data, self.index).expect("checked when read")
    }
}

/// The faces text is drawn with: Segoe UI (semibold for the bold), egui's
/// own if it is missing, then faces of other scripts for the letters it
/// lacks (a file named in Chinese, Japanese, Korean), read when first needed.
struct Faces {
    regular: Vec<Face>,
    bold: Vec<Face>,
    others: Option<Vec<Face>>,
}

impl Faces {
    fn load() -> Faces {
        let dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
        let read = |names: &[&str]| -> Vec<Face> {
            names
                .iter()
                .filter_map(|n| Face::read(&dir.join(n), 0))
                .take(1)
                .collect()
        };
        let mut regular = read(&["segoeui.ttf"]);
        let mut bold = read(&["seguisb.ttf", "segoeuib.ttf"]);
        if regular.is_empty() {
            let egui_font = egui::FontDefinitions::default()
                .font_data
                .get("Ubuntu-Light")
                .map(|f| f.font.to_vec());
            regular.extend(egui_font.map(|data| Face { data, index: 0 }));
        }
        if bold.is_empty() {
            bold.extend(regular.first().map(|f| Face {
                data: f.data.clone(),
                index: f.index,
            }));
        }
        Faces {
            regular,
            bold,
            others: None,
        }
    }

    fn others(&mut self) -> &[Face] {
        self.others.get_or_insert_with(|| {
            let dir =
                PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
                    .join("Fonts");
            [
                ("msyh.ttc", 0),
                ("YuGothM.ttc", 0),
                ("malgun.ttf", 0),
                ("Nirmala.ttc", 0),
                ("seguisym.ttf", 0),
            ]
            .iter()
            .filter_map(|(n, i)| Face::read(&dir.join(n), *i))
            .collect()
        })
    }

    /// The face of `c` (`None`: no face has it) and its glyph.
    fn glyph(&mut self, c: char, bold: bool) -> Option<(&Face, skrifa::GlyphId)> {
        let own = if bold { &self.bold } else { &self.regular };
        let at = own.iter().position(|f| f.font().charmap().map(c).is_some());
        if let Some(at) = at {
            let own = if bold { &self.bold } else { &self.regular };
            let face = &own[at];
            return Some((face, face.font().charmap().map(c)?));
        }
        let others = self.others();
        let face = others
            .iter()
            .find(|f| f.font().charmap().map(c).is_some())?;
        Some((face, face.font().charmap().map(c)?))
    }
}

/// Text and its grounds drawn over the picture.
struct Text {
    ctx: vello_cpu::RenderContext,
    faces: Faces,
}

type Rgba8 = [u8; 4];

fn paint(c: Rgba8) -> vello_cpu::color::AlphaColor<vello_cpu::color::Srgb> {
    vello_cpu::color::AlphaColor::from_rgba8(c[0], c[1], c[2], c[3])
}

/// A glyph's outline as a path, from `(x, baseline)`.
struct Pen<'a> {
    path: &'a mut kurbo::BezPath,
    x: f64,
    y: f64,
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path
            .move_to((self.x + f64::from(x), self.y - f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path
            .line_to((self.x + f64::from(x), self.y - f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path.quad_to(
            (self.x + f64::from(cx0), self.y - f64::from(cy0)),
            (self.x + f64::from(x), self.y - f64::from(y)),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            (self.x + f64::from(cx0), self.y - f64::from(cy0)),
            (self.x + f64::from(cx1), self.y - f64::from(cy1)),
            (self.x + f64::from(x), self.y - f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

impl Text {
    fn new(width: u32, height: u32) -> Text {
        Text {
            ctx: vello_cpu::RenderContext::new(width as u16, height as u16),
            faces: Faces::load(),
        }
    }

    fn rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, radius: f64, colour: Rgba8) {
        self.ctx.set_paint(paint(colour));
        let path = kurbo::RoundedRect::new(x0, y0, x1, y1, radius).to_path(0.1);
        self.ctx.fill_path(&path);
    }

    /// The width of `text` at `px` pixels.
    fn width(&mut self, text: &str, px: f32, bold: bool) -> f64 {
        text.chars().map(|c| self.advance(c, px, bold)).sum()
    }

    fn advance(&mut self, c: char, px: f32, bold: bool) -> f64 {
        match self.faces.glyph(c, bold) {
            Some((face, id)) => face
                .font()
                .glyph_metrics(Size::new(px), LocationRef::default())
                .advance_width(id)
                .map_or(0.0, f64::from),
            None => f64::from(px) * 0.5,
        }
    }

    /// `text` from `(x, baseline)`, cut short with `…` to `room` pixels.
    #[allow(clippy::too_many_arguments)]
    fn line(
        &mut self,
        text: &str,
        x: f64,
        baseline: f64,
        px: f32,
        room: f64,
        colour: Rgba8,
        bold: bool,
    ) {
        let mut chars: Vec<char> = text.chars().collect();
        if self.width(text, px, bold) > room {
            let ellipsis = self.advance('…', px, bold);
            while !chars.is_empty() {
                chars.pop();
                let w: f64 = chars.iter().map(|&c| self.advance(c, px, bold)).sum();
                if w + ellipsis <= room {
                    break;
                }
            }
            chars.push('…');
        }
        self.ctx.set_paint(paint(colour));
        let mut pen_x = x;
        for c in chars {
            let Some((face, id)) = self.faces.glyph(c, bold) else {
                pen_x += f64::from(px) * 0.5;
                continue;
            };
            let font = face.font();
            let advance = font
                .glyph_metrics(Size::new(px), LocationRef::default())
                .advance_width(id)
                .map_or(0.0, f64::from);
            let mut path = kurbo::BezPath::new();
            if let Some(outline) = font.outline_glyphs().get(id) {
                let mut pen = Pen {
                    path: &mut path,
                    x: pen_x,
                    y: baseline,
                };
                let settings = DrawSettings::unhinted(Size::new(px), LocationRef::default());
                if outline.draw(settings, &mut pen).is_ok() && !path.elements().is_empty() {
                    self.ctx.fill_path(&path);
                }
            }
            pen_x += advance;
        }
    }

    /// Draw what was asked over `canvas` (opaque, so premultiplied as it is).
    fn draw_onto(mut self, canvas: &mut RgbaImage) {
        let (w, h) = canvas.dimensions();
        self.ctx.flush();
        let Some(target) = vello_cpu::PixmapMut::new(w as u16, h as u16, canvas) else {
            return;
        };
        let settings = vello_cpu::RasterizerSettings {
            composite_mode: vello_cpu::CompositeMode::SrcOver,
            ..Default::default()
        };
        self.ctx
            .render_with(target, &mut vello_cpu::Resources::new(), settings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_round_trip() {
        let l = Layout {
            columns: 5,
            rows: 3,
            width: 1920,
            header: false,
            format: FrameFormat::Png,
        };
        assert_eq!(Layout::from_text(&l.to_text()), l);
        assert_eq!(Layout::from_text(""), Layout::default());
        assert_eq!(Layout::default().to_text(), "4|4|1536|info|jpg");
        // Out of range: the default's.
        let odd = Layout::from_text("40|0|1000|info|gif");
        assert_eq!(odd, Layout::default());
    }

    #[test]
    fn frames_in_the_middles() {
        assert_eq!(times(100.0, 4), vec![12.5, 37.5, 62.5, 87.5]);
    }

    #[test]
    fn tiles_fill_the_width() {
        let l = Layout::default();
        let (w, h) = tile_size(l, (1920, 1080));
        let g = gap(l.width);
        assert_eq!(w * 4 + g * 5, l.width - (l.width - g * 5) % 4);
        assert_eq!(h, (f64::from(w) * 9.0 / 16.0).round() as u32);
        // Upright: taller than wide, in twice the columns.
        let upright = l.for_shape((1080, 1920));
        assert_eq!((upright.columns, upright.rows), (8, 2));
        assert_eq!(l.for_shape((1920, 1080)), l);
        let (w, h) = tile_size(upright, (1080, 1920));
        assert!(h > w);
        let odd = Layout {
            columns: 3,
            rows: 3,
            ..l
        };
        let odd = odd.for_shape((1080, 1920));
        assert_eq!((odd.columns, odd.rows), (6, 2));
    }

    #[test]
    fn turned_frames() {
        let p = Params {
            dw: 1920,
            dh: 1080,
            rotate: 90,
        };
        assert_eq!(p.shown(), (1080, 1920));
        let tile = (90, 160);
        let mut data = vec![0u8; 1920 * 1080 * 4];
        data[0] = 255; // the top left pixel red
        let shot = Value::Map(vec![
            ("w".into(), Value::Int(1920)),
            ("h".into(), Value::Int(1080)),
            ("stride".into(), Value::Int(1920 * 4)),
            ("format".into(), Value::Str("rgba".into())),
            ("data".into(), Value::Bytes(data)),
        ]);
        let f = fitted(&shot, p, tile).unwrap();
        assert_eq!(f.dimensions(), tile);
        // Turned clockwise: the top left corner goes to the top right.
        assert!(f.get_pixel(89, 0)[0] > 0);
        assert_eq!(f.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn details_line() {
        let about = About {
            duration: 5025.0,
            size: Some((1920, 1080)),
            codec: Some(codec_name("hevc")),
            fps: Some(23.976),
            bytes: Some(4_500_000_000),
        };
        assert_eq!(
            details(&about),
            "1:23:45  ·  1920×1080  ·  HEVC  ·  23.976 fps  ·  4.19 GB"
        );
    }

    /// `cargo test video_map_of_sample -- --ignored --nocapture` with
    /// METALMPV_SAMPLE (a video): writes `metalmpv_map.jpg` in the temp
    /// folder (METALMPV_MAP_OUT elsewhere) and says how long it took.
    #[test]
    #[ignore]
    fn video_map_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE").expect("METALMPV_SAMPLE is not set");
        let input = PathBuf::from(sample);
        let output = std::env::var_os("METALMPV_MAP_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("metalmpv_map.jpg"));
        let request = Request {
            title: input.file_name().unwrap().to_string_lossy().into_owned(),
            input,
            layout: Layout::default(),
            output: output.clone(),
            hwdec: if std::env::var_os("METALMPV_MAP_SW").is_some() {
                "no"
            } else {
                "auto-copy-safe"
            },
        };
        let progress = AtomicU32::new(0);
        let started = Instant::now();
        let made = make(&request, &progress, &AtomicBool::new(false), &|| {}).unwrap();
        println!(
            "{} ({}×{}) in {:.0} ms",
            made.output.display(),
            made.image.width(),
            made.image.height(),
            started.elapsed().as_secs_f64() * 1e3
        );
        assert_eq!(progress.load(Ordering::Relaxed), 16);
        assert!(std::fs::metadata(&output).unwrap().len() > 10_000);
    }
}
