//! Saving a stretch of the file playing (the A-B repeat's) as a file of its
//! own, by a second mpv core without a window on a thread of its own, while
//! the player goes on.
//!
//! As it is ([`Mode::Copy`]): mpv's `dump-cache` writes the packets read
//! between the points into a Matroska file without decoding them, from the
//! keyframe at or before the start: quick, in the quality it was. The core
//! opens the file paused at the start with its cache on and reads ahead to
//! the end, so the stretch must fit in the cache ([`MAX_CACHE`]).
//!
//! Encoded ([`Mode::Encode`]): mpv's encoding mode (`o`) decodes the
//! stretch and encodes it, exact to the frame: H.264 or HEVC on the
//! processor (x264, x265) or on an NVIDIA card (NVENC), AAC sound, in an
//! MP4; the subtitles shown burnt into the picture when wanted (mpv's
//! encoder draws them).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use crate::mpv::{EndReason, Event, Mpv, sys};

/// The most a stretch saved as it is may take in memory.
const MAX_CACHE: u64 = 3 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    pub const ALL: [Codec; 2] = [Codec::H264, Codec::Hevc];

    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC (H.265)",
        }
    }
}

/// How the stretch is encoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Encode {
    pub codec: Codec,
    /// On an NVIDIA card (NVENC) rather than the processor.
    pub nvenc: bool,
    /// x264's and x265's CRF, NVENC's CQ: lower is better and larger.
    pub quality: u8,
    /// The subtitles shown are drawn into the picture.
    pub burn_subs: bool,
}

impl Default for Encode {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            nvenc: false,
            quality: 20,
            burn_subs: false,
        }
    }
}

/// The range of [`Encode::quality`].
pub const QUALITY_RANGE: std::ops::RangeInclusive<u8> = 14..=34;

impl Encode {
    /// mpv's encoder and its options.
    fn encoder(&self) -> (&'static str, String) {
        let q = self.quality;
        match (self.codec, self.nvenc) {
            (Codec::H264, false) => ("libx264", format!("crf={q},preset=veryfast")),
            (Codec::Hevc, false) => ("libx265", format!("crf={q},preset=fast")),
            (Codec::H264, true) => ("h264_nvenc", format!("rc=vbr,cq={q},b=0,preset=p5")),
            (Codec::Hevc, true) => ("hevc_nvenc", format!("rc=vbr,cq={q},b=0,preset=p5")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Copy,
    Encode(Encode),
}

impl Mode {
    /// The extension of the file it writes.
    pub fn extension(self) -> &'static str {
        match self {
            Mode::Copy => "mkv",
            Mode::Encode(_) => "mp4",
        }
    }
}

/// The subtitles shown in the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Subs {
    None,
    /// A track of the file (mpv's id).
    Own(i64),
    /// A file of their own, after the file's `embedded` subtitle tracks
    /// (its id in a core that loads it alone is the next): saved as it is,
    /// it is left out; encoded, it can be burnt in.
    File {
        path: PathBuf,
        embedded: i64,
    },
}

/// The way chosen last, kept in the settings (`clip`): as it is or
/// encoded, and how it is encoded either way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub copy: bool,
    pub encode: Encode,
}

impl Default for Choice {
    fn default() -> Self {
        Self {
            copy: true,
            encode: Encode::default(),
        }
    }
}

impl Choice {
    pub fn mode(self) -> Mode {
        if self.copy {
            Mode::Copy
        } else {
            Mode::Encode(self.encode)
        }
    }

    /// `copy|h264|cpu|20|subs`, `encode|hevc|nvenc|24|-`.
    pub fn to_text(self) -> String {
        let e = self.encode;
        format!(
            "{}|{}|{}|{}|{}",
            if self.copy { "copy" } else { "encode" },
            if e.codec == Codec::Hevc {
                "hevc"
            } else {
                "h264"
            },
            if e.nvenc { "nvenc" } else { "cpu" },
            e.quality,
            if e.burn_subs { "subs" } else { "-" }
        )
    }

    /// What [`Choice::to_text`] wrote; the default for what is not
    /// understood.
    pub fn from_text(text: &str) -> Self {
        let f: Vec<&str> = text.split('|').collect();
        let [copy, codec, device, quality, subs] = f[..] else {
            return Self::default();
        };
        let d = Encode::default();
        Self {
            copy: copy != "encode",
            encode: Encode {
                codec: if codec == "hevc" {
                    Codec::Hevc
                } else {
                    Codec::H264
                },
                nvenc: device == "nvenc",
                quality: quality
                    .parse::<u8>()
                    .ok()
                    .filter(|q| QUALITY_RANGE.contains(q))
                    .unwrap_or(d.quality),
                burn_subs: subs == "subs",
            },
        }
    }
}

/// What to save: `from`..`to` seconds of `input` into `output`, with the
/// audio track playing (mpv's id; `None` for none) and the subtitles;
/// `sub_style`, mpv's options of the subtitles' look, for burning them in.
#[derive(Clone, Debug)]
pub struct Request {
    pub input: PathBuf,
    pub from: f64,
    pub to: f64,
    pub output: PathBuf,
    pub mode: Mode,
    pub audio: Option<i64>,
    pub subs: Subs,
    pub sub_style: Vec<(&'static str, String)>,
}

/// Why a stretch was not saved.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Cancelled,
    /// Saved as it is, the stretch did not fit in the cache.
    TooLong,
    /// mpv's reason.
    Mpv(String),
}

/// A stretch being saved.
pub struct Job {
    pub output: PathBuf,
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    done: Receiver<Result<(), Failure>>,
}

impl Job {
    /// Start saving; `wake` is called as it goes on and when it ends.
    pub fn start(request: Request, wake: impl Fn() + Send + 'static) -> Result<Job, String> {
        let progress = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (send, done) = channel();
        let output = request.output.clone();
        let (p, c) = (progress.clone(), cancel.clone());
        crate::worker::spawn("clip", move || {
            let result = run(&request, &p, &c, &wake);
            if result.is_err() {
                // Nothing half written is left behind.
                let _ = std::fs::remove_file(&request.output);
            }
            let _ = send.send(result);
            wake();
        })
        .map_err(|e| e.to_string())?;
        Ok(Job {
            output,
            progress,
            cancel,
            done,
        })
    }

    /// How far it went, 0 to 1.
    pub fn progress(&self) -> f32 {
        self.progress.load(Ordering::Relaxed) as f32 / 1000.0
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// How it ended, once it has.
    pub fn take(&self) -> Option<Result<(), Failure>> {
        match self.done.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(Failure::Mpv("the job ended".into()))),
        }
    }
}

/// The reply of `loadfile` and of `dump-cache`.
const LOAD: u64 = 1;
const DUMP: u64 = 2;
/// The observed properties.
const CACHE_STATE: u64 = 10;
const TIME_POS: u64 = 11;

/// The options of both ways: no window, no sound, nothing of the user's.
fn quiet() -> Vec<(&'static str, String)> {
    [
        ("terminal", "no"),
        ("config", "no"),
        ("load-scripts", "no"),
        ("ytdl", "no"),
        ("input-default-bindings", "no"),
        ("audio-display", "no"),
        ("keep-open", "no"),
        ("idle", "yes"),
        // Only the subtitles asked for.
        ("sub-auto", "no"),
    ]
    .into_iter()
    .map(|(n, v)| (n, v.to_owned()))
    .collect()
}

/// Do `request` on this thread; `progress` in thousandths.
fn run(
    request: &Request,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    wake: &dyn Fn(),
) -> Result<(), Failure> {
    let api = sys::api().map_err(|e| Failure::Mpv(e.to_string()))?;
    let span = (request.to - request.from).max(0.1);
    let track = |id: Option<i64>| id.map_or("no".to_owned(), |id| id.to_string());
    let mut options = quiet();
    options.push(("aid", track(request.audio)));
    match request.mode {
        Mode::Copy => {
            // Paused at the start, the cache filled to the end.
            let ahead = format!("{:.0}", span + 10.0);
            options.extend([
                ("vo", "null".to_owned()),
                ("ao", "null".to_owned()),
                ("pause", "yes".to_owned()),
                ("cache", "yes".to_owned()),
                ("cache-secs", ahead.clone()),
                ("demuxer-readahead-secs", ahead),
                ("demuxer-max-bytes", MAX_CACHE.to_string()),
                // What the decoder took for the first frame stays too.
                ("demuxer-max-back-bytes", MAX_CACHE.to_string()),
                // The file's own subtitles go with it; a file of their own
                // is not in the cache.
                (
                    "sid",
                    track(match request.subs {
                        Subs::Own(id) => Some(id),
                        _ => None,
                    }),
                ),
            ]);
        }
        Mode::Encode(encode) => {
            let (codec, codec_options) = encode.encoder();
            options.extend([
                ("o", request.output.to_string_lossy().into_owned()),
                ("ovc", codec.to_owned()),
                ("ovcopts", codec_options),
                ("oac", "aac".to_owned()),
                ("oacopts", "b=192k".to_owned()),
                // Frames decoded on the card come back for the encoder.
                ("hwdec", "auto-copy-safe".to_owned()),
                ("end", format!("{:.3}", request.to)),
            ]);
            let sid = match &request.subs {
                Subs::Own(id) if encode.burn_subs => Some(*id),
                Subs::File { path, embedded } if encode.burn_subs => {
                    options.push(("sub-files", path.to_string_lossy().into_owned()));
                    Some(embedded + 1)
                }
                _ => None,
            };
            options.push(("sid", track(sid)));
            if sid.is_some() {
                options.extend(request.sub_style.iter().cloned());
            }
            // Even sizes, as the encoders want for 4:2:0 (a picture 1441
            // wide was refused); for H.264 8 bits, as a 10-bit one plays
            // almost nowhere.
            let even = "scale=trunc(iw/2)*2:trunc(ih/2)*2";
            let graph = if encode.codec == Codec::H264 {
                format!("lavfi=[{even},format=yuv420p]")
            } else {
                format!("lavfi=[{even}]")
            };
            options.push(("vf", graph));
        }
    }
    let options: Vec<(&str, &str)> = options.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let mut mpv = Mpv::new(api, &options).map_err(Failure::Mpv)?;
    mpv.request_log_messages("info");
    mpv.observe(CACHE_STATE, "demuxer-cache-state", sys::MPV_FORMAT_NODE);
    mpv.observe(TIME_POS, "time-pos", sys::MPV_FORMAT_DOUBLE);
    let input = request.input.to_string_lossy();
    let start = format!("start={:.3}", request.from);
    mpv.command(LOAD, &["loadfile", &input, "replace", "-1", &start]);
    log::info!(
        "saving {:.1}–{:.1} s of {} as {:?} into {}",
        request.from,
        request.to,
        request.input.display(),
        request.mode,
        request.output.display()
    );
    let set = |share: f64| {
        let value = (share.clamp(0.0, 1.0) * 1000.0) as u32;
        if progress.swap(value, Ordering::Relaxed) != value {
            wake();
        }
    };
    let mut loaded = false;
    let mut dumping = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        while let Some(event) = mpv.next_event() {
            match event {
                Event::FileLoaded => loaded = true,
                Event::CommandReply {
                    id: LOAD | DUMP,
                    error: Some(e),
                } => return Err(Failure::Mpv(e)),
                Event::CommandReply { id: DUMP, .. } => {
                    set(1.0);
                    return Ok(());
                }
                Event::EndFile {
                    reason: EndReason::Error,
                    error,
                } => return Err(Failure::Mpv(error.unwrap_or_else(|| "cannot open".into()))),
                Event::EndFile { .. } if matches!(request.mode, Mode::Encode(_)) && loaded => {
                    // The encoder finishes the file as the core is destroyed.
                    drop(mpv);
                    set(1.0);
                    return Ok(());
                }
                Event::Property {
                    id: CACHE_STATE,
                    value,
                } if request.mode == Mode::Copy && loaded && !dumping => {
                    let end = value.get("cache-end").and_then(|v| v.double());
                    let eof = value.get("eof").is_some_and(|v| v.flag());
                    let bytes = value.get("total-bytes").and_then(|v| v.int()).unwrap_or(0);
                    if let Some(end) = end {
                        set((end - request.from) / span * 0.95);
                    }
                    if eof || end.is_some_and(|e| e >= request.to) {
                        dumping = true;
                        let (from, to) =
                            (format!("{:.3}", request.from), format!("{:.3}", request.to));
                        let output = request.output.to_string_lossy();
                        mpv.command(DUMP, &["dump-cache", &from, &to, &output]);
                    } else if bytes as u64 >= MAX_CACHE / 100 * 95 {
                        return Err(Failure::TooLong);
                    }
                }
                Event::Property {
                    id: TIME_POS,
                    value,
                } => {
                    if let (Mode::Encode(_), Some(t)) = (request.mode, value.double()) {
                        set((t - request.from) / span);
                    }
                }
                Event::Log {
                    prefix,
                    level,
                    text,
                } => {
                    log::info!("clip mpv {level} [{prefix}] {}", text.trim_end());
                }
                Event::Shutdown => return Err(Failure::Mpv("the core stopped".into())),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A file for the stretch `from`..`to` of `input` in `folder`: the name
/// with the times (`Film 00-12-34–00-13-10.mkv`), never one that is there.
pub fn file_name(folder: &Path, input: &Path, from: f64, to: f64, extension: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "clip".into());
    let time = |t: f64| crate::format::time(Some(t), true).replace(':', "-");
    let base = format!("{stem} {}–{}", time(from), time(to));
    (1..)
        .map(|n| {
            let name = if n == 1 {
                format!("{base}.{extension}")
            } else {
                format!("{base} ({n}).{extension}")
            };
            folder.join(name)
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_the_times() {
        let dir = std::env::temp_dir().join(format!("metalmpv_clip_names_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let film = Path::new(r"D:\Films\Film.mkv");
        let first = file_name(&dir, film, 754.0, 790.5, "mkv");
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            "Film 0-12-34–0-13-10.mkv"
        );
        std::fs::write(&first, b"").unwrap();
        let second = file_name(&dir, film, 754.0, 790.5, "mkv");
        assert!(second.to_string_lossy().ends_with("(2).mkv"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn choice_kept() {
        let c = Choice {
            copy: false,
            encode: Encode {
                codec: Codec::Hevc,
                nvenc: true,
                quality: 24,
                burn_subs: true,
            },
        };
        assert_eq!(c.to_text(), "encode|hevc|nvenc|24|subs");
        assert_eq!(Choice::from_text(&c.to_text()), c);
        assert_eq!(Choice::from_text("junk"), Choice::default());
        assert_eq!(Choice::from_text("copy|h264|cpu|99|-").encode.quality, 20);
    }

    #[test]
    fn encoders() {
        let mut e = Encode::default();
        assert_eq!(e.encoder().0, "libx264");
        e.codec = Codec::Hevc;
        e.nvenc = true;
        e.quality = 24;
        assert_eq!(
            e.encoder(),
            ("hevc_nvenc", "rc=vbr,cq=24,b=0,preset=p5".to_owned())
        );
    }

    /// Both ways on a real file without a window: `cargo test clip_of_sample
    /// -- --ignored --nocapture` (a video with sound in METALMPV_SAMPLE, or
    /// the default below; METALMPV_NVENC=1 encodes on the card too).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn clip_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let dir = std::env::temp_dir().join(format!("metalmpv_clip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        println!("sample: {:?}", duration_of(Path::new(&sample)));
        let mut modes = vec![Mode::Copy, Mode::Encode(Encode::default())];
        if std::env::var("METALMPV_NVENC").is_ok() {
            modes.push(Mode::Encode(Encode {
                nvenc: true,
                codec: Codec::Hevc,
                ..Encode::default()
            }));
        }
        for mode in modes {
            let output = dir.join(
                format!("clip {:?}.{}", mode, mode.extension())
                    .replace(['{', '}', ':', ' ', ','], "_"),
            );
            let request = Request {
                input: sample.clone().into(),
                from: 2.0,
                to: 5.0,
                output: output.clone(),
                mode,
                audio: Some(1),
                subs: Subs::None,
                sub_style: Vec::new(),
            };
            let (progress, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
            let started = std::time::Instant::now();
            run(&request, &progress, &cancel, &|| {}).unwrap();
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            println!("{mode:?}: {size} bytes in {:?}", started.elapsed());
            assert!(size > 10_000, "{mode:?} wrote {size} bytes");
            assert_eq!(progress.load(Ordering::Relaxed), 1000);
            // The duration, as mpv reads it.
            let duration = duration_of(&output);
            println!("  duration {duration:?}");
            let duration = duration.unwrap();
            // As it is, from the keyframe before the start to the one after
            // the end; encoded, exact.
            let (low, high) = if mode == Mode::Copy {
                (2.9, 15.0)
            } else {
                (2.8, 3.3)
            };
            assert!(duration > low && duration < high, "{mode:?}: {duration}");
        }
        // Subtitles of a file of their own burnt in, in a look of the user's.
        let srt = dir.join("subs.srt");
        std::fs::write(
            &srt,
            "1
00:00:02,500 --> 00:00:04,000
Burnt in
",
        )
        .unwrap();
        let output = dir.join("burnt.mp4");
        let request = Request {
            input: sample.clone().into(),
            from: 2.0,
            to: 5.0,
            output: output.clone(),
            mode: Mode::Encode(Encode {
                burn_subs: true,
                ..Encode::default()
            }),
            audio: Some(1),
            subs: Subs::File {
                path: srt,
                embedded: 0,
            },
            sub_style: crate::sub_style::SubStyle::default().options().to_vec(),
        };
        let (progress, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
        run(&request, &progress, &cancel, &|| {}).unwrap();
        assert!(duration_of(&output).is_some_and(|d| (d - 3.0).abs() < 0.2));
        // The same moment without them: the picture differs at the bottom
        // (the line), hardly at the top.
        let plain = dir.join("plain.mp4");
        let request = Request {
            output: plain.clone(),
            subs: Subs::None,
            ..request
        };
        run(&request, &progress, &cancel, &|| {}).unwrap();
        let (with, without) = (frame_at(&output, 1.5, &dir), frame_at(&plain, 1.5, &dir));
        let differ = |rows: std::ops::Range<u32>| {
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in rows {
                for x in 0..with.width() {
                    let (a, b) = (with.get_pixel(x, y), without.get_pixel(x, y));
                    sum +=
                        a.0.iter()
                            .zip(b.0)
                            .map(|(a, b)| u64::from(a.abs_diff(b)))
                            .sum::<u64>();
                    n += 1;
                }
            }
            sum as f64 / n as f64
        };
        let h = with.height();
        let (top, bottom) = (differ(0..h / 4), differ(h * 3 / 4..h));
        println!("burnt in: the top differs by {top:.2}, the bottom by {bottom:.2}");
        assert!(bottom > top * 3.0 + 1.0, "no subtitles burnt in");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The frame at `time` of `path`, decoded.
    fn frame_at(path: &Path, time: f64, dir: &Path) -> image::RgbImage {
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("pause", "yes"),
            ],
        )
        .unwrap();
        let png = dir.join(format!(
            "{}.png",
            path.file_stem().unwrap().to_string_lossy()
        ));
        let start = format!("start={time}");
        mpv.command(
            0,
            &["loadfile", &path.to_string_lossy(), "replace", "-1", &start],
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut asked = false;
        while std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::PlaybackRestart if !asked => {
                        asked = true;
                        mpv.command(9, &["screenshot-to-file", &png.to_string_lossy(), "video"]);
                    }
                    Event::CommandReply { id: 9, .. } => {
                        return image::open(&png).unwrap().to_rgb8();
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("no frame of {}", path.display());
    }

    fn duration_of(path: &Path) -> Option<f64> {
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("pause", "yes"),
            ],
        )
        .unwrap();
        mpv.observe(1, "duration", sys::MPV_FORMAT_DOUBLE);
        mpv.command(0, &["loadfile", &path.to_string_lossy()]);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                if let Event::Property { id: 1, value } = event
                    && let Some(d) = value.double()
                {
                    return Some(d);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }
}
