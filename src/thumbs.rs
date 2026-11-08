//! Frames over the seek bar: while the pointer is over it, the frame near
//! the time under the pointer (`settings.seek_preview`).
//!
//! A second mpv core without a window or sound (`vo=null`, paused, decoding
//! on the processor: a second hardware decoder would take the card's
//! memory beside the film's) on a thread of its own, as `video_map.rs`'s,
//! started the first time the bar asks. It seeks to keyframes only (a few
//! tens of milliseconds) and gives each frame with `screenshot-raw`, scaled
//! and turned here (`video_map::fitted`). The bar asks for a time; only the
//! latest asked is taken, the ones passed over meanwhile are forgotten.
//! The times are cut into [`STEPS`] steps of the duration, a texture kept
//! per step, the nearest one shown until the one asked for comes.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use image::RgbaImage;

use crate::mpv::{EndReason, Event, Mpv, sys};
use crate::video_map::{Params, fitted};

/// The steps of the duration a frame is taken for (one per second in a
/// shorter file).
const STEPS: f64 = 600.0;
/// The textures kept (each some 130 KB at 100 %).
const KEPT: usize = 160;
/// How many steps away a frame taken may stand in for the one asked for.
const NEAR: u32 = 12;
/// How long mpv may take for a frame before the core is given up.
const STALL: Duration = Duration::from_secs(10);

/// The frames' longer side in points.
pub const SIZE: f32 = 192.0;

/// What the frames are of: a file, the picture as shown (the turn, the
/// shape, a mirror) and their size in pixels. Another one makes new frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub input: PathBuf,
    pub picture: Vec<(&'static str, String)>,
    /// The frames' longer side in pixels.
    pub size: u32,
}

/// What the core's thread sends back.
enum Message {
    /// The frame of a step.
    Frame(u32, RgbaImage),
    /// No frame for a step (a seek refused).
    Missed(u32),
    /// The core gave up: no frames for this file.
    Failed(String),
}

/// Between the bar and the core's thread.
#[derive(Default)]
struct Shared {
    /// The step asked for last and its time; taken by the thread.
    wanted: Mutex<Option<(u32, f64)>>,
    asked: Condvar,
    stop: AtomicBool,
}

/// The frames of the file playing.
pub struct Thumbs {
    request: Request,
    duration: f64,
    /// Started at the first [`Thumbs::at`].
    worker: Option<(Arc<Shared>, Receiver<Message>)>,
    textures: HashMap<u32, egui::TextureHandle>,
    /// The steps with a texture, the one shown longest ago first.
    used: VecDeque<u32>,
    /// The step asked of the core and not answered yet.
    asked: Option<u32>,
    failed: bool,
}

/// What the bar shows: a frame near the time, if one is there yet, and
/// the size the frames have.
pub struct Preview {
    pub texture: Option<egui::TextureHandle>,
    /// In pixels, once a frame has come.
    pub size: Option<[usize; 2]>,
}

impl Thumbs {
    pub fn new(request: Request, duration: f64) -> Thumbs {
        Thumbs {
            request,
            duration,
            worker: None,
            textures: HashMap::new(),
            used: VecDeque::new(),
            asked: None,
            failed: false,
        }
    }

    /// Whether these are the frames of `request` over `duration`.
    pub fn is_for(&self, request: &Request, duration: f64) -> bool {
        self.request == *request && (self.duration - duration).abs() < 0.5
    }

    fn step(&self) -> f64 {
        (self.duration / STEPS).max(1.0)
    }

    fn step_of(&self, seconds: f64) -> u32 {
        (seconds.clamp(0.0, self.duration) / self.step()).round() as u32
    }

    /// The frame for `seconds`, asked of the core if it has not come; the
    /// nearest one taken meanwhile. `None` when there are none to show
    /// (the core failed: a file it cannot open, no picture).
    pub fn at(&mut self, ctx: &egui::Context, seconds: f64) -> Option<Preview> {
        if self.failed {
            return None;
        }
        if self.worker.is_none() {
            self.start(ctx);
        }
        self.receive(ctx);
        if self.failed {
            return None;
        }
        let step = self.step_of(seconds);
        let size = self.textures.values().next().map(|t| t.size());
        if self.textures.contains_key(&step) {
            self.used.retain(|s| *s != step);
            self.used.push_back(step);
            return Some(Preview {
                texture: self.textures.get(&step).cloned(),
                size,
            });
        }
        if self.asked != Some(step)
            && let Some((shared, _)) = &self.worker
        {
            let time = (f64::from(step) * self.step()).min(self.duration);
            if let Ok(mut wanted) = shared.wanted.lock() {
                *wanted = Some((step, time));
            }
            shared.asked.notify_one();
            self.asked = Some(step);
        }
        let near = self
            .textures
            .iter()
            .map(|(s, t)| (s.abs_diff(step), t))
            .filter(|(d, _)| *d <= NEAR)
            .min_by_key(|(d, _)| *d)
            .map(|(_, t)| t.clone());
        Some(Preview {
            texture: near,
            size,
        })
    }

    fn start(&mut self, ctx: &egui::Context) {
        let shared = Arc::new(Shared::default());
        let (send, receive) = channel();
        let (request, s, wake) = (self.request.clone(), shared.clone(), ctx.clone());
        let started = crate::worker::spawn("seek preview", move || {
            let fail = send.clone();
            let result =
                crate::worker::guarded(|| run(&request, &s, &send, &|| wake.request_repaint()))
                    .unwrap_or_else(|| Err("the preview failed".into()));
            if let Err(e) = result {
                let _ = fail.send(Message::Failed(e));
                wake.request_repaint();
            }
        });
        match started {
            Ok(_) => self.worker = Some((shared, receive)),
            Err(e) => {
                log::warn!("seek preview: {e}");
                self.failed = true;
            }
        }
    }

    /// The frames come in as textures; the oldest shown are let go.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some((_, receive)) = &self.worker else {
            return;
        };
        let messages: Vec<Message> = receive.try_iter().collect();
        for message in messages {
            match message {
                Message::Frame(step, image) => {
                    let size = [image.width() as usize, image.height() as usize];
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                    let texture = ctx.load_texture(
                        format!("seek_preview_{step}"),
                        pixels,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.insert(step, texture);
                    self.used.retain(|s| *s != step);
                    self.used.push_back(step);
                    while self.used.len() > KEPT {
                        if let Some(old) = self.used.pop_front() {
                            self.textures.remove(&old);
                        }
                    }
                    if self.asked == Some(step) {
                        self.asked = None;
                    }
                }
                Message::Missed(step) => {
                    if self.asked == Some(step) {
                        self.asked = None;
                    }
                }
                Message::Failed(e) => {
                    log::info!("seek preview of {}: {e}", self.request.input.display());
                    self.failed = true;
                    self.textures.clear();
                    self.used.clear();
                }
            }
        }
    }
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        if let Some((shared, _)) = &self.worker {
            shared.stop.store(true, Ordering::Relaxed);
            shared.asked.notify_one();
        }
    }
}

const LOAD: u64 = 1;
const PARAMS: u64 = 2;
const SEEK: u64 = 3;
const SHOT: u64 = 4;

/// The core's thread: the file opened, then a frame for each step asked
/// until the bar's [`Thumbs`] is dropped.
fn run(
    request: &Request,
    shared: &Shared,
    send: &Sender<Message>,
    wake: &dyn Fn(),
) -> Result<(), String> {
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
        ("sub-auto", "no"),
        ("pause", "yes"),
        ("keep-open", "always"),
        ("idle", "yes"),
        ("hr-seek", "no"),
        ("hwdec", "no"),
        // Faster, a little softer: a small picture.
        ("vd-lavc-skiploopfilter", "all"),
        ("cache", "no"),
    ];
    let picture = request.picture.iter().map(|(k, v)| (*k, v.as_str()));
    let options: Vec<(&str, &str)> = options.into_iter().chain(picture).collect();
    let mut mpv = Mpv::new(api, &options)?;
    mpv.request_log_messages("error");
    let input = request.input.to_string_lossy();
    let started = Instant::now();
    mpv.command(LOAD, &["loadfile", &input, "replace"]);

    let mut params: Option<Params> = None;
    let mut shown_first = false;
    // The step being taken, whether its seek is done, since when.
    let mut taking: Option<(u32, bool, Instant)> = None;
    let mut last_event = Instant::now();
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        if params.is_none() && last_event.elapsed() > STALL {
            return Err("mpv went silent opening the file".into());
        }
        if let Some((_, _, since)) = taking
            && since.elapsed() > STALL
        {
            return Err("mpv went silent taking a frame".into());
        }
        let Some(event) = mpv.next_event() else {
            // Nothing from mpv: the latest step asked, else a short wait.
            if taking.is_none()
                && params.is_some()
                && let Ok(mut wanted) = shared.wanted.lock()
            {
                match wanted.take() {
                    Some((step, time)) => {
                        mpv.command(SEEK, &["seek", &format!("{time:.3}"), "absolute+keyframes"]);
                        taking = Some((step, false, Instant::now()));
                    }
                    None => {
                        let _ = shared.asked.wait_timeout(wanted, Duration::from_millis(50));
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
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
            Event::PlaybackRestart if !shown_first => {
                shown_first = true;
                mpv.get(PARAMS, "video-params", sys::MPV_FORMAT_NODE);
            }
            Event::Property { id: PARAMS, value } => {
                params =
                    Some(Params::of(&value).ok_or_else(|| "the file has no picture".to_owned())?);
                log::debug!(
                    "seek preview ready in {:.0} ms",
                    started.elapsed().as_secs_f64() * 1e3
                );
            }
            Event::CommandReply {
                id: SEEK, error, ..
            } => {
                if let Some((step, _, _)) = taking
                    && let Some(e) = error
                {
                    log::debug!("seek preview: seek refused: {e}");
                    let _ = send.send(Message::Missed(step));
                    taking = None;
                    wake();
                }
            }
            Event::Seek => {
                if let Some((_, done, _)) = &mut taking {
                    *done = true;
                }
            }
            Event::PlaybackRestart => {
                if let Some((_, true, _)) = taking {
                    mpv.command(SHOT, &["screenshot-raw", "video", "rgba"]);
                }
            }
            Event::CommandReply {
                id: SHOT,
                error,
                result,
            } => {
                let Some((step, _, since)) = taking.take() else {
                    continue;
                };
                let p = params.expect("params before the frames");
                let frame = match error {
                    Some(e) => {
                        log::debug!("seek preview: frame: {e}");
                        None
                    }
                    None => fitted(&result, p, tile(p.shown(), request.size)),
                };
                log::trace!(
                    "seek preview: step {step} in {:.0} ms",
                    since.elapsed().as_secs_f64() * 1e3
                );
                let _ = send.send(match frame {
                    Some(frame) => Message::Frame(step, frame),
                    None => Message::Missed(step),
                });
                wake();
            }
            Event::Log {
                prefix,
                level,
                text,
            } => log::debug!("seek preview mpv {level} [{prefix}] {text}"),
            Event::Shutdown => return Err("the core stopped".into()),
            _ => {}
        }
    }
}

/// The frame's size for a picture of `shown` display size: its longer
/// side `size` pixels.
fn tile(shown: (u32, u32), size: u32) -> (u32, u32) {
    let (w, h) = (f64::from(shown.0.max(1)), f64::from(shown.1.max(1)));
    let size = f64::from(size);
    let (tw, th) = if w >= h {
        (size, size * h / w)
    } else {
        (size * w / h, size)
    };
    ((tw.round() as u32).max(8), (th.round() as u32).max(8))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The duration of `path`, as mpv gives it.
    fn duration_of(path: &str) -> f64 {
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(api, &[("vo", "null"), ("ao", "null"), ("idle", "yes")]).unwrap();
        mpv.command(0, &["loadfile", path]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match mpv.next_event() {
                Some(Event::FileLoaded) => mpv.get(1, "duration", sys::MPV_FORMAT_DOUBLE),
                Some(Event::Property { id: 1, value }) => return value.double().unwrap(),
                Some(_) => {}
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        panic!("no duration");
    }

    /// METALMPV_SAMPLE (a video): the frames at a tenth, the middle, the
    /// next steps (most on the same keyframe: a seek that does not move),
    /// the very end and the start come, and how long each took.
    #[test]
    #[ignore]
    fn frames_of_sample() {
        let sample = std::env::var("METALMPV_SAMPLE").expect("METALMPV_SAMPLE is not set");
        let duration = duration_of(&sample);
        println!("duration {duration:.2} s");
        let ctx = egui::Context::default();
        let mut thumbs = Thumbs::new(
            Request {
                input: PathBuf::from(sample),
                picture: Vec::new(),
                size: 240,
            },
            duration,
        );
        let step = thumbs.step();
        let middle = duration * 0.5;
        for seconds in [
            duration * 0.1,
            middle,
            middle + step,
            middle + 2.0 * step,
            duration,
            0.0,
        ] {
            let step = thumbs.step_of(seconds);
            let started = Instant::now();
            loop {
                let preview = thumbs.at(&ctx, seconds).expect("the core failed");
                if thumbs.textures.contains_key(&step) {
                    let [w, h] = preview.size.expect("a size");
                    assert!(w == 240 || h == 240, "{w}×{h}");
                    println!(
                        "{seconds:.1} s: {w}×{h} in {:.0} ms",
                        started.elapsed().as_secs_f64() * 1e3
                    );
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(20), "no frame");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn tiles_keep_the_shape() {
        assert_eq!(tile((1920, 1080), 240), (240, 135));
        assert_eq!(tile((1080, 1920), 240), (135, 240));
        assert_eq!(tile((100, 1), 240), (240, 8));
    }

    #[test]
    fn steps() {
        let t = Thumbs::new(
            Request {
                input: PathBuf::from("a.mkv"),
                picture: Vec::new(),
                size: 240,
            },
            6000.0,
        );
        assert_eq!(t.step(), 10.0);
        assert_eq!(t.step_of(0.0), 0);
        assert_eq!(t.step_of(14.9), 1);
        assert_eq!(t.step_of(15.1), 2);
        assert_eq!(t.step_of(7000.0), 600);
        let short = Thumbs::new(t.request.clone(), 90.0);
        assert_eq!(short.step(), 1.0);
        assert_eq!(short.step_of(42.4), 42);
    }
}
