//! Saving a stretch of the file playing (the A-B repeat's) as a file of its
//! own, by a second mpv core without a window on a thread of its own, while
//! the player goes on: mpv's encoding mode (`o`) decodes the stretch and
//! encodes it, exact to the frame, H.264 or HEVC on the processor (x264,
//! x265) or on an NVIDIA card (NVENC), AAC sound, in an MP4; the subtitles
//! shown burnt into the picture when wanted (mpv's encoder draws them).
//!
//! No copy without encoding: mpv's `dump-cache` and `stream-record` write
//! through its recorder, which drops the packets without a decoding time,
//! the B-frames of a Matroska file (a third of the frames, the picture
//! broken); and no FFmpeg is at hand but inside libmpv.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use crate::mpv::{EndReason, Event, Mpv, sys};

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
            quality: 22,
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

/// The subtitles shown in the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Subs {
    None,
    /// A track of the file (mpv's id).
    Own(i64),
    /// A file of their own, after the file's `embedded` subtitle tracks
    /// (its id in a core that loads it alone is the next).
    File {
        path: PathBuf,
        embedded: i64,
    },
}

impl Encode {
    /// The setting `clip`: `h264|cpu|22|subs`, `hevc|nvenc|24|-`.
    pub fn to_text(self) -> String {
        format!(
            "{}|{}|{}|{}",
            if self.codec == Codec::Hevc {
                "hevc"
            } else {
                "h264"
            },
            if self.nvenc { "nvenc" } else { "cpu" },
            self.quality,
            if self.burn_subs { "subs" } else { "-" }
        )
    }

    /// What [`Encode::to_text`] wrote (and a first field of the way, as a
    /// version that also copied wrote); the default for what is not
    /// understood.
    pub fn from_text(text: &str) -> Self {
        let mut f: Vec<&str> = text.split('|').collect();
        if f.len() == 5 {
            f.remove(0);
        }
        let [codec, device, quality, subs] = f[..] else {
            return Self::default();
        };
        Self {
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
                .unwrap_or(Self::default().quality),
            burn_subs: subs == "subs",
        }
    }
}

/// What to save: `from`..`to` seconds of `input` into `output`, with the
/// audio track playing (mpv's id; `None` for none; its channels) and the
/// subtitles; `sub_style`, mpv's options of the subtitles' look, for
/// burning them in; the delays and the deinterlacing of the player, so that
/// the stretch is as it was seen.
#[derive(Clone, Debug)]
pub struct Request {
    pub input: PathBuf,
    pub from: f64,
    pub to: f64,
    pub output: PathBuf,
    /// `output` may be written over (its Save dialog asked).
    pub replace: bool,
    pub encode: Encode,
    pub audio: Option<i64>,
    pub channels: Option<i64>,
    pub subs: Subs,
    pub sub_style: Vec<(&'static str, String)>,
    pub audio_delay: f64,
    pub sub_delay: f64,
    /// mpv's `deinterlace`.
    pub deinterlace: &'static str,
}

/// Why a stretch was not saved.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Cancelled,
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
        // A file that was there is not deleted for a failure, unless the
        // Save dialog asked to replace it (it may be half written over).
        let ours = request.replace || !output.exists();
        let (p, c) = (progress.clone(), cancel.clone());
        crate::worker::spawn("clip", move || {
            let result = crate::worker::guarded(|| run(&request, &p, &c, &wake))
                .unwrap_or_else(|| Err(Failure::Mpv("the encoding failed".into())));
            if result.is_err() && ours {
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

    /// Cancel it and wait up to `wait` for its thread to delete the file
    /// half written (the program closes: the process would end first).
    pub fn cancel_and_wait(&self, wait: Duration) {
        self.cancel();
        let _ = self.done.recv_timeout(wait);
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

/// The reply of `loadfile`, and the position observed.
const LOAD: u64 = 1;
const TIME_POS: u64 = 2;

/// No window, no sound, nothing of the user's.
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
        // An encoder that cannot start (NVENC without its card) ends the
        // file with an error instead of going on without the picture.
        ("stop-playback-on-init-failure", "yes"),
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
    let encode = request.encode;
    let (codec, codec_options) = encode.encoder();
    options.extend([
        ("o", request.output.to_string_lossy().into_owned()),
        ("ovc", codec.to_owned()),
        ("ovcopts", codec_options),
        ("oac", "aac".to_owned()),
        ("oacopts", format!("b={}k", aac_kbps(request.channels))),
        // Frames decoded on the card come back for the encoder.
        ("hwdec", "auto-copy-safe".to_owned()),
        ("end", format!("{:.3}", request.to)),
        ("audio-delay", format!("{:.3}", request.audio_delay)),
        ("deinterlace", request.deinterlace.to_owned()),
    ]);
    let sid = match &request.subs {
        Subs::Own(id) if encode.burn_subs => Some(*id),
        Subs::File { path, embedded } if encode.burn_subs => {
            // `sub-files` is a list cut at `;` (and `sub-files-append` is
            // refused as an option): a path with one goes by its short name.
            let path = if path.to_string_lossy().contains(';') {
                crate::win::short_path(path).unwrap_or_else(|| path.clone())
            } else {
                path.clone()
            };
            options.push(("sub-files", path.to_string_lossy().into_owned()));
            Some(embedded + 1)
        }
        _ => None,
    };
    options.push(("sid", track(sid)));
    if sid.is_some() {
        options.extend(request.sub_style.iter().cloned());
        options.push(("sub-delay", format!("{:.3}", request.sub_delay)));
    }
    // Even sizes, as the encoders want for 4:2:0 (a picture 1441 wide was
    // refused); for H.264 8 bits, as a 10-bit one plays almost nowhere.
    let even = "scale=trunc(iw/2)*2:trunc(ih/2)*2";
    let graph = if encode.codec == Codec::H264 {
        format!("lavfi=[{even},format=yuv420p]")
    } else {
        format!("lavfi=[{even}]")
    };
    options.push(("vf", graph));
    let options: Vec<(&str, &str)> = options.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let mut mpv = Mpv::new(api, &options).map_err(Failure::Mpv)?;
    mpv.request_log_messages("info");
    mpv.observe(TIME_POS, "time-pos", sys::MPV_FORMAT_DOUBLE);
    let input = request.input.to_string_lossy();
    let start = format!("start={:.3}", request.from);
    mpv.command(LOAD, &["loadfile", &input, "replace", "-1", &start]);
    log::info!(
        "saving {:.1}–{:.1} s of {} as {:?} into {}",
        request.from,
        request.to,
        request.input.display(),
        request.encode,
        request.output.display()
    );
    let set = |share: f64| {
        let value = (share.clamp(0.0, 1.0) * 1000.0) as u32;
        if progress.swap(value, Ordering::Relaxed) != value {
            wake();
        }
    };
    let mut loaded = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Failure::Cancelled);
        }
        while let Some(event) = mpv.next_event() {
            match event {
                Event::FileLoaded => loaded = true,
                Event::CommandReply {
                    id: LOAD,
                    error: Some(e),
                } => return Err(Failure::Mpv(e)),
                Event::EndFile {
                    reason: EndReason::Error,
                    error,
                } => return Err(Failure::Mpv(error.unwrap_or_else(|| "cannot open".into()))),
                Event::EndFile {
                    reason: EndReason::Eof,
                    ..
                } if loaded => {
                    // The encoder finishes the file as the core is destroyed.
                    drop(mpv);
                    let size = std::fs::metadata(&request.output).map_or(0, |m| m.len());
                    if size == 0 {
                        return Err(Failure::Mpv("nothing was written".into()));
                    }
                    set(1.0);
                    return Ok(());
                }
                Event::EndFile { reason, .. } if loaded => {
                    return Err(Failure::Mpv(format!("the file ended ({reason:?})")));
                }
                Event::Property {
                    id: TIME_POS,
                    value,
                } => {
                    if let Some(t) = value.double() {
                        set((t - request.from) / span);
                    }
                }
                Event::Log {
                    prefix,
                    level,
                    text,
                } => log::info!("clip mpv {level} [{prefix}] {}", text.trim_end()),
                Event::Shutdown => return Err(Failure::Mpv("the core stopped".into())),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// AAC's bitrate for `channels`: 192 kb/s for stereo or mono, 64 kb/s a
/// channel for more (384 for 5.1, 512 for 7.1).
fn aac_kbps(channels: Option<i64>) -> i64 {
    match channels {
        Some(n) if n > 2 => 64 * n.min(8),
        _ => 192,
    }
}

/// A file for the stretch `from`..`to` of `input` in `folder`: the name
/// with the times (`Film 0-12-34–0-13-10.mp4`), never one that is there.
/// What Windows does not take in a name (an address's `?` or `:`) becomes
/// `_`, and a long name is cut short.
pub fn file_name(folder: &Path, input: &Path, from: f64, to: f64) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| safe_name(&s.to_string_lossy()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "clip".into());
    let time = |t: f64| crate::format::time(Some(t), true).replace(':', "-");
    let base = format!("{stem} {}–{}", time(from), time(to));
    (1..)
        .map(|n| {
            let name = if n == 1 {
                format!("{base}.mp4")
            } else {
                format!("{base} ({n}).mp4")
            };
            folder.join(name)
        })
        .find(|p| !p.exists())
        .expect("a free name")
}

/// `name` as a file name Windows takes: `<>:"/\|?*` and control characters
/// as `_`, no dots or spaces at the end, at most 150 characters.
fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(150)
        .collect();
    name.trim_end_matches(['.', ' ']).to_owned()
}

/// The file to write for `typed` (the window's field): a relative path is
/// taken in `folder`, and `.mp4` is added unless it names MP4, MKV or MOV.
pub fn output_path(typed: &str, folder: &Path) -> PathBuf {
    let path = PathBuf::from(typed.trim());
    let path = if path.is_absolute() {
        path
    } else {
        folder.join(path)
    };
    let known = path.extension().is_some_and(|e| {
        ["mp4", "mkv", "mov"]
            .iter()
            .any(|k| e.eq_ignore_ascii_case(k))
    });
    if known {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".mp4");
        PathBuf::from(name)
    }
}

/// `a` and `b` are the same file (Windows' names regardless of case).
pub fn same_file(a: &Path, b: &Path) -> bool {
    let full = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
    };
    full(a) == full(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_the_times() {
        let dir = std::env::temp_dir().join(format!("metalmpv_clip_names_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let film = Path::new(r"D:\Films\Film.mkv");
        let first = file_name(&dir, film, 754.0, 790.5);
        assert_eq!(
            first.file_name().unwrap().to_string_lossy(),
            "Film 0-12-34–0-13-10.mp4"
        );
        std::fs::write(&first, b"").unwrap();
        let second = file_name(&dir, film, 754.0, 790.5);
        assert!(second.to_string_lossy().ends_with("(2).mp4"));
        // An address's name, as Windows takes it.
        let stream = Path::new("https://cdn.example/live/stream?id=5");
        let named = file_name(&dir, stream, 0.0, 10.0);
        assert_eq!(
            named.file_name().unwrap().to_string_lossy(),
            "stream_id=5 0-00-00–0-00-10.mp4"
        );
        assert_eq!(safe_name("host:8080"), "host_8080");
        assert_eq!(safe_name("a. . "), "a");
        assert_eq!(safe_name(&"я".repeat(300)).chars().count(), 150);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_subtitle_path_with_a_semicolon() {
        let dir = std::env::temp_dir().join(format!("metalmpv a;b {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let srt = dir.join("x.srt");
        std::fs::write(&srt, b"").unwrap();
        // Volumes without short names give none; the path stays as it is.
        if let Some(short) = crate::win::short_path(&srt) {
            assert!(
                !short.to_string_lossy().contains(';'),
                "{}",
                short.display()
            );
            assert!(short.exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paths_typed() {
        let folder = Path::new(r"D:\Clips");
        assert_eq!(output_path("clip", folder), Path::new(r"D:\Clips\clip.mp4"));
        assert_eq!(output_path(" a.MKV ", folder), Path::new(r"D:\Clips\a.MKV"));
        assert_eq!(output_path(r"E:\x.v1", folder), Path::new(r"E:\x.v1.mp4"));
        assert!(same_file(
            Path::new(r"C:\NoSuch\A.mp4"),
            Path::new(r"c:\nosuch\a.MP4")
        ));
        assert!(!same_file(Path::new(r"C:\a.mp4"), Path::new(r"C:\b.mp4")));
        assert_eq!(aac_kbps(Some(2)), 192);
        assert_eq!(aac_kbps(Some(6)), 384);
        assert_eq!(aac_kbps(None), 192);
    }

    #[test]
    fn kept_as_text() {
        let e = Encode {
            codec: Codec::Hevc,
            nvenc: true,
            quality: 24,
            burn_subs: true,
        };
        assert_eq!(e.to_text(), "hevc|nvenc|24|subs");
        assert_eq!(Encode::from_text(&e.to_text()), e);
        // As a version that also copied wrote it.
        assert_eq!(Encode::from_text("encode|hevc|nvenc|24|subs"), e);
        assert_eq!(Encode::from_text("junk"), Encode::default());
        assert_eq!(Encode::from_text("h264|cpu|99|-").quality, 22);
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

    /// A stretch encoded on a real file without a window, and subtitles
    /// burnt in: `cargo test clip_of_sample -- --ignored --nocapture` (a
    /// video with sound in METALMPV_SAMPLE, or the default below; another
    /// stretch than 2–5 s in METALMPV_CLIP_FROM and _TO; METALMPV_NVENC=1
    /// encodes on the card too).
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn clip_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test2.mp4".into());
        let dir = std::env::temp_dir().join(format!("metalmpv_clip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let at = |name: &str, or: f64| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(or)
        };
        let (from, to) = (at("METALMPV_CLIP_FROM", 2.0), at("METALMPV_CLIP_TO", 5.0));
        let mut ways = vec![Encode::default()];
        if std::env::var("METALMPV_NVENC").is_ok() {
            ways.push(Encode {
                nvenc: true,
                codec: Codec::Hevc,
                ..Encode::default()
            });
        }
        let request = |output: &Path, encode: Encode, subs: Subs| Request {
            input: sample.clone().into(),
            from,
            to,
            output: output.to_path_buf(),
            replace: false,
            encode,
            audio: Some(1),
            channels: Some(2),
            subs,
            sub_style: crate::sub_style::SubStyle::default().options().to_vec(),
            audio_delay: 0.0,
            sub_delay: 0.0,
            deinterlace: "auto",
        };
        let (progress, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
        for (i, encode) in ways.into_iter().enumerate() {
            let output = dir.join(format!("clip {i}.mp4"));
            let started = std::time::Instant::now();
            run(
                &request(&output, encode, Subs::None),
                &progress,
                &cancel,
                &|| {},
            )
            .unwrap();
            let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
            let duration = duration_of(&output).unwrap();
            println!(
                "{encode:?}: {size} bytes, {duration:.3} s, in {:?}",
                started.elapsed()
            );
            assert_eq!(progress.load(Ordering::Relaxed), 1000);
            // Exact to the frame.
            assert!((duration - (to - from)).abs() < 0.2, "{duration}");
        }
        // Subtitles of a file of their own burnt in, in a look of the
        // user's: the picture differs from the same stretch without them at
        // the bottom (the line), hardly at the top.
        let srt = dir.join("subs.srt");
        let stamp = |t: f64| {
            let ms = (t * 1000.0).round() as u64;
            format!(
                "{:02}:{:02}:{:02},{:03}",
                ms / 3_600_000,
                ms / 60_000 % 60,
                ms / 1000 % 60,
                ms % 1000
            )
        };
        std::fs::write(
            &srt,
            format!("1\n{} --> {}\nBurnt in\n", stamp(from), stamp(to)),
        )
        .unwrap();
        let (burnt, plain) = (dir.join("burnt.mp4"), dir.join("plain.mp4"));
        let with_subs = Encode {
            burn_subs: true,
            ..Encode::default()
        };
        let subs = Subs::File {
            path: srt,
            embedded: embedded_subs(Path::new(&sample)),
        };
        run(
            &request(&burnt, with_subs, subs),
            &progress,
            &cancel,
            &|| {},
        )
        .unwrap();
        run(
            &request(&plain, with_subs, Subs::None),
            &progress,
            &cancel,
            &|| {},
        )
        .unwrap();
        let middle = (to - from) / 2.0;
        let (with, without) = (
            frame_at(&burnt, middle, &dir),
            frame_at(&plain, middle, &dir),
        );
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
        assert!(bottom > top * 2.0 + 1.0, "no subtitles burnt in");
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

    /// How many subtitle tracks `path` has of its own.
    fn embedded_subs(path: &Path) -> i64 {
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(
            api,
            &[
                ("vo", "null"),
                ("ao", "null"),
                ("idle", "yes"),
                ("pause", "yes"),
                ("sub-auto", "no"),
            ],
        )
        .unwrap();
        mpv.command(0, &["loadfile", &path.to_string_lossy()]);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            while let Some(event) = mpv.next_event() {
                match event {
                    Event::FileLoaded => mpv.get(1, "track-list", sys::MPV_FORMAT_NODE),
                    Event::Property { id: 1, value } => {
                        let tracks = crate::tracks::from_value(&value);
                        return crate::tracks::of_kind(&tracks, crate::tracks::Kind::Sub).len()
                            as i64;
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        0
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
