//! Two videos compared on the same frame, as pixop's video-compare does:
//! the file playing (A) and another (B) on a second mpv core with a render
//! context of its own in the same OpenGL context. Both pictures are
//! rendered at the size of the video area (or of its half, side by side)
//! with the same zoom, pan, turn and shape (B's `video-aspect-override` is
//! A's), so mpv puts them in the same rectangle and egui shows them in
//! parts: A left of the split line, B right of it.
//!
//! B follows A ([`Sync`]): paused, B is put on A's frame exactly (an exact
//! seek to A's position plus the offset whenever that changes: a step, a
//! seek); playing, B plays without sound and its speed is nudged by the
//! drift (a seek when it is far off). B's own frames are B's: with another
//! frame rate the frame shown is the one mpv's exact seek lands on.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How the two pictures are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// One picture, A left of a line that follows the pointer, B right.
    #[default]
    Split,
    /// Each whole, side by side or one over the other, whichever shows
    /// them larger.
    Side,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Split, Mode::Side];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Split => "split",
            Mode::Side => "side",
        }
    }

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Split => tr!("Split line", "Шторка"),
            Mode::Side => tr!("Side by side", "Рядом"),
        }
    }

    pub fn next(self) -> Mode {
        match self {
            Mode::Split => Mode::Side,
            Mode::Side => Mode::Split,
        }
    }
}

/// What starts a comparison: A (played if it is not), B and B's offset.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub a: Option<PathBuf>,
    pub b: PathBuf,
    pub offset: f64,
}

/// The first of the paths that ask for a comparison (`metalmpv --compare
/// A B`, also as handed over to a running copy): `?` is in no Windows
/// path. Then the offset, A and B.
const MARK: &str = "?compare";

/// The paths of `--compare a b --offset s` for the window: what
/// [`requested`] takes back.
pub fn command_paths(a: &Path, b: &Path, offset: f64) -> Vec<PathBuf> {
    vec![
        PathBuf::from(MARK),
        PathBuf::from(format!("{offset}")),
        a.to_path_buf(),
        b.to_path_buf(),
    ]
}

/// The files to open and the comparison asked for among `paths` (the last
/// one, if several came).
pub fn requested(paths: Vec<PathBuf>) -> (Vec<PathBuf>, Option<Request>) {
    let mut files = Vec::new();
    let mut request = None;
    let mut rest = paths.into_iter();
    while let Some(path) = rest.next() {
        if path.as_os_str() != MARK {
            files.push(path);
            continue;
        }
        let (Some(offset), Some(a), Some(b)) = (rest.next(), rest.next(), rest.next()) else {
            break;
        };
        let offset = offset.to_string_lossy().parse::<f64>().unwrap_or(0.0);
        request = Some(Request {
            a: Some(a),
            b,
            offset: if offset.is_finite() { offset } else { 0.0 },
        });
    }
    (files, request)
}

/// What [`Sync`] needs to know of a player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Side {
    pub time: Option<f64>,
    pub duration: Option<f64>,
    pub paused: bool,
    pub eof: bool,
    pub seeking: bool,
    pub buffering: bool,
    /// Nothing loaded, or a file still loading.
    pub idle: bool,
    pub speed: f64,
}

impl Side {
    pub fn of(state: &crate::player::State, loading: bool) -> Self {
        Side {
            time: state.time,
            duration: state.duration,
            paused: state.paused,
            eof: state.eof,
            seeking: state.seeking,
            buffering: state.buffering,
            idle: state.idle || loading,
            speed: state.speed,
        }
    }
}

/// What B is told.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// An exact seek to this time of B's.
    Seek(f64),
    Pause(bool),
    Speed(f64),
}

/// Playing, a drift beyond this is put right by a seek.
const RESEEK: f64 = 0.5;
/// Playing, a drift under this is left alone (a quarter of a frame at
/// 24 fps): the positions are those of the frames shown, read at the
/// interface's frames.
const DEADBAND: f64 = 0.010;
/// The share of the speed a second of drift changes, and the most it does.
const GAIN: f64 = 1.0;
const MAX_NUDGE: f64 = 0.10;
/// A seek of B's not answered this long is given up.
const SEEK_WAIT: Duration = Duration::from_secs(2);
/// The weight of a new drift reading.
const SMOOTHING: f64 = 0.15;

/// B's speed for a drift (B ahead positive) at A's speed `base`.
pub fn speed_for(drift: f64, base: f64) -> f64 {
    if drift.abs() < DEADBAND {
        base
    } else {
        base * (1.0 - (drift * GAIN).clamp(-MAX_NUDGE, MAX_NUDGE))
    }
}

/// How B is kept on A.
#[derive(Clone, Debug)]
pub struct Sync {
    /// Paused: the target B was last sent to.
    sent: Option<f64>,
    /// Playing: B is to be sought to A (A jumped, B loaded, the offset
    /// changed).
    resync: bool,
    /// Playing: B's seek given then, not answered yet.
    awaiting: Option<Instant>,
    /// How long B's seeks took: a seek while playing aims that much ahead.
    latency: f64,
    /// The drift, smoothed; `None` until read anew.
    pub drift: Option<f64>,
    /// The speed B was given.
    speed: f64,
}

impl Default for Sync {
    fn default() -> Self {
        Sync {
            sent: None,
            resync: true,
            awaiting: None,
            latency: 0.1,
            drift: None,
            speed: 1.0,
        }
    }
}

impl Sync {
    /// A jumped while playing (a seek, a loop) or the offset changed: B
    /// follows. Paused, B follows A's position anyway.
    pub fn jumped(&mut self) {
        self.resync = true;
    }

    /// B loaded its file (at a position of its own): put on A's anew.
    pub fn reset(&mut self) {
        self.resync = true;
        self.sent = None;
        self.awaiting = None;
        self.drift = None;
    }

    /// B finished a seek.
    pub fn restarted(&mut self, now: Instant) {
        if let Some(at) = self.awaiting.take() {
            let took = now.saturating_duration_since(at).as_secs_f64().min(1.0);
            self.latency = self.latency * 0.5 + took * 0.5;
        }
        self.drift = None;
    }

    /// What B is to do now, `offset` seconds after A (B's time is A's plus
    /// the offset).
    pub fn follow(&mut self, a: &Side, b: &Side, offset: f64, now: Instant) -> Vec<Step> {
        let mut steps = Vec::new();
        if a.idle || b.idle {
            return steps;
        }
        let Some(ta) = a.time else {
            return steps;
        };
        let target = ta + offset;
        let playing = !a.paused && !a.eof && !a.buffering && target >= 0.0;
        if !playing {
            // On A's frame exactly, whenever A's moves.
            self.resync = false;
            self.awaiting = None;
            self.drift = None;
            if !b.paused {
                steps.push(Step::Pause(true));
            }
            if self.speed != a.speed {
                self.speed = a.speed;
                steps.push(Step::Speed(a.speed));
            }
            let target = target.max(0.0);
            if self.sent.is_none_or(|s| (s - target).abs() > 1e-4) {
                self.sent = Some(target);
                steps.push(Step::Seek(target));
            }
            return steps;
        }
        self.sent = None;
        if a.seeking {
            return steps;
        }
        // B ended before A: its last frame stays.
        if b.eof && !self.resync && b.duration.is_some_and(|d| target >= d - 0.05) {
            return steps;
        }
        if self.resync {
            self.resync = false;
            self.awaiting = Some(now);
            self.drift = None;
            steps.push(Step::Seek(target + self.latency * a.speed));
            if b.paused {
                steps.push(Step::Pause(false));
            }
            return steps;
        }
        if let Some(at) = self.awaiting {
            if now.saturating_duration_since(at) < SEEK_WAIT {
                return steps;
            }
            self.awaiting = None;
        }
        if b.paused {
            steps.push(Step::Pause(false));
        }
        let Some(tb) = b.time.filter(|_| !b.seeking) else {
            return steps;
        };
        let raw = (tb - offset) - ta;
        if raw.abs() > RESEEK {
            self.resync = true;
            return steps;
        }
        let drift = match self.drift {
            Some(d) => d + SMOOTHING * (raw - d),
            None => raw,
        };
        self.drift = Some(drift);
        let speed = speed_for(drift, a.speed);
        if (speed - self.speed).abs() > 0.0005 {
            self.speed = speed;
            steps.push(Step::Speed(speed));
        }
        steps
    }
}

/// The frame rate the offset is counted in: B's video track's, else 25.
pub fn frame_rate(tracks: &[crate::tracks::Track]) -> f64 {
    crate::tracks::selected(tracks, crate::tracks::Kind::Video)
        .and_then(|t| t.fps)
        .filter(|f| f.is_finite() && *f > 1.0)
        .unwrap_or(25.0)
}

/// B's offset for the screen: in B's frames at `fps`, and in seconds;
/// `None` while there is none.
pub fn shift_in(lang: crate::i18n::Lang, offset: f64, fps: f64) -> Option<String> {
    let frames = (offset * fps).round() as i64;
    if offset.abs() < 1e-6 {
        return None;
    }
    let sign = if offset < 0.0 { "−" } else { "+" };
    let n = frames.unsigned_abs();
    let seconds = format!("{:.3}", offset.abs());
    Some(match lang {
        crate::i18n::Lang::En => {
            let unit = if n == 1 { "frame" } else { "frames" };
            format!("B {sign}{n} {unit} ({sign}{seconds} s)")
        }
        crate::i18n::Lang::Ru => {
            let unit = crate::i18n::ru_plural(n, "кадр", "кадра", "кадров");
            let seconds = seconds.replace('.', ",");
            format!("B {sign}{n} {unit} ({sign}{seconds} с)")
        }
    })
}

/// The two places, side by side or one over the other, in which a picture
/// of `aspect` (width over height) is shown larger within `area`.
pub fn halves(area: egui::Rect, aspect: f32) -> [egui::Rect; 2] {
    let aspect = if aspect > 0.0 { aspect } else { 16.0 / 9.0 };
    let fitted = |size: egui::Vec2| (size.x / aspect).min(size.y);
    let across = egui::vec2(area.width() / 2.0, area.height());
    let down = egui::vec2(area.width(), area.height() / 2.0);
    if fitted(across) >= fitted(down) {
        let mid = area.center().x;
        [area.with_max_x(mid), area.with_min_x(mid)]
    } else {
        let mid = area.center().y;
        [area.with_max_y(mid), area.with_min_y(mid)]
    }
}

/// The pan (in percent, as `Adjust`'s) that keeps the point of the picture
/// `from_centre` points from the area's centre where it is as the zoom goes
/// from `zoom` to `new_zoom` (percent): `geometry` gives the picture's
/// fitted size. mpv moves the picture by the pan in its zoomed size.
pub fn pan_for_zoom(
    geometry: crate::ui::navigator::Geometry,
    pan: [f64; 2],
    zoom: i64,
    new_zoom: i64,
    from_centre: egui::Vec2,
) -> [f64; 2] {
    let aspect = f64::from(geometry.aspect);
    let area = [f64::from(geometry.area.x), f64::from(geometry.area.y)];
    if aspect <= 0.0 || area[0] <= 0.0 || area[1] <= 0.0 {
        return pan;
    }
    let height = (area[0] / aspect).min(area[1]);
    let fitted = [height * aspect, height];
    let d = [f64::from(from_centre.x), f64::from(from_centre.y)];
    let max = crate::adjust::PAN_MAX;
    std::array::from_fn(|i| {
        let before = fitted[i] * zoom as f64 / 100.0;
        let after = fitted[i] * new_zoom as f64 / 100.0;
        // The picture's point under the pointer, from its left (top), in
        // fractions; the same after.
        let point = 0.5 - pan[i] / 100.0 + d[i] / before;
        ((0.5 - point + d[i] / after) * 100.0).clamp(-max, max)
    })
}

/// The pan (in percent, as `Adjust`'s) after the picture is dragged `by`
/// points from where `pan` had it at `zoom` (percent): the picture follows
/// the pointer. `geometry` gives its fitted size.
pub fn pan_by(
    geometry: crate::ui::navigator::Geometry,
    pan: [f64; 2],
    zoom: i64,
    by: egui::Vec2,
) -> [f64; 2] {
    let aspect = f64::from(geometry.aspect);
    let area = [f64::from(geometry.area.x), f64::from(geometry.area.y)];
    if aspect <= 0.0 || area[0] <= 0.0 || area[1] <= 0.0 || zoom <= 0 {
        return pan;
    }
    let height = (area[0] / aspect).min(area[1]);
    let zoomed = [
        height * aspect * zoom as f64 / 100.0,
        height * zoom as f64 / 100.0,
    ];
    let by = [f64::from(by.x), f64::from(by.y)];
    let max = crate::adjust::PAN_MAX;
    std::array::from_fn(|i| (pan[i] + by[i] / zoomed[i] * 100.0).clamp(-max, max))
}

/// The zoom after `notches` of the wheel: a quarter more each, within the
/// panel's range.
pub fn wheel_zoom(zoom: i64, notches: i32) -> i64 {
    let range = crate::adjust::ZOOM_RANGE;
    let zoomed = (zoom as f64 * 1.25f64.powi(notches)).round() as i64;
    // At least a step: a small zoom rounded back to itself.
    let zoomed = match notches.signum() {
        1 => zoomed.max(zoom + 1),
        -1 => zoomed.min(zoom - 1),
        _ => zoom,
    };
    zoomed.clamp(*range.start(), *range.end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(time: f64, paused: bool) -> Side {
        Side {
            time: Some(time),
            duration: Some(600.0),
            paused,
            speed: 1.0,
            ..Side::default()
        }
    }

    #[test]
    fn modes_by_name() {
        for m in Mode::ALL {
            assert_eq!(Mode::from_name(m.name()), Some(m));
        }
        assert_eq!(Mode::from_name("diff"), None);
        assert_eq!(Mode::Split.next().next(), Mode::Split);
    }

    #[test]
    fn the_command_line_asks_for_a_comparison() {
        let a = PathBuf::from(r"C:\a.mkv");
        let b = PathBuf::from(r"D:\b.mkv");
        let (files, request) = requested(command_paths(&a, &b, 0.08));
        assert!(files.is_empty());
        assert_eq!(
            request,
            Some(Request {
                a: Some(a.clone()),
                b: b.clone(),
                offset: 0.08
            })
        );
        // Among other starts' files, and cut short.
        let mut paths = vec![PathBuf::from(r"C:\x.mp4")];
        paths.extend(command_paths(&a, &b, -1.5));
        let (files, request) = requested(paths);
        assert_eq!(files, vec![PathBuf::from(r"C:\x.mp4")]);
        assert_eq!(request.map(|r| r.offset), Some(-1.5));
        let (files, request) = requested(vec![PathBuf::from(MARK), PathBuf::from("0")]);
        assert!(files.is_empty() && request.is_none());
    }

    #[test]
    fn paused_b_goes_to_a_s_frame_once() {
        let now = Instant::now();
        let mut sync = Sync::default();
        let a = side(10.0, true);
        let b = side(3.0, false);
        let steps = sync.follow(&a, &b, 0.5, now);
        assert_eq!(steps, vec![Step::Pause(true), Step::Seek(10.5)]);
        // Sent: not again while A stays.
        let b = side(10.5, true);
        assert!(sync.follow(&a, &b, 0.5, now).is_empty());
        // A stepped a frame.
        let a = side(10.04, true);
        assert_eq!(sync.follow(&a, &b, 0.5, now), vec![Step::Seek(10.54)]);
        // Before B's start: its first frame.
        let a = side(0.2, true);
        assert_eq!(sync.follow(&a, &b, -1.0, now), vec![Step::Seek(0.0)]);
    }

    #[test]
    fn playing_b_is_sought_then_nudged() {
        let now = Instant::now();
        let mut sync = Sync::default();
        let a = side(20.0, false);
        let b = side(5.0, true);
        // Loaded: sought ahead by the seek's time, and played.
        let steps = sync.follow(&a, &b, 0.0, now);
        assert_eq!(steps, vec![Step::Seek(20.1), Step::Pause(false)]);
        // Nothing while the seek is under way.
        let b = side(20.1, false);
        assert!(sync.follow(&a, &b, 0.0, now).is_empty());
        sync.restarted(now + Duration::from_millis(100));
        // 50 ms ahead: slowed by 5 %.
        let a = side(30.0, false);
        let b = side(30.05, false);
        let steps = sync.follow(&a, &b, 0.0, now);
        assert_eq!(steps.len(), 1);
        let Step::Speed(s) = steps[0] else {
            panic!("{steps:?}")
        };
        assert!((s - 0.95).abs() < 1e-9);
        // Far off: sought again.
        let b = side(31.0, false);
        assert!(sync.follow(&a, &b, 0.0, now).is_empty());
        assert!(matches!(sync.follow(&a, &b, 0.0, now)[0], Step::Seek(_)));
    }

    #[test]
    fn a_b_that_ended_stays_at_its_end() {
        let now = Instant::now();
        let mut sync = Sync {
            resync: false,
            ..Sync::default()
        };
        let a = side(700.0, false);
        let b = Side {
            eof: true,
            ..side(600.0, true)
        };
        assert!(sync.follow(&a, &b, 0.0, now).is_empty());
    }

    #[test]
    fn the_nudge_has_a_dead_band_and_a_limit() {
        assert_eq!(speed_for(0.005, 1.0), 1.0);
        assert!((speed_for(-0.05, 2.0) - 2.1).abs() < 1e-9);
        assert!((speed_for(3.0, 1.0) - 0.9).abs() < 1e-9);
    }

    #[test]
    fn halves_follow_the_shape() {
        let wide = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 500.0));
        let [a, b] = halves(wide, 16.0 / 9.0);
        assert_eq!(a.width(), 800.0);
        assert_eq!(b.min.x, 800.0);
        let tall = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 900.0));
        let [a, b] = halves(tall, 16.0 / 9.0);
        assert_eq!(a.height(), 450.0);
        assert_eq!(b.min.y, 450.0);
    }

    #[test]
    fn the_wheel_zooms_about_the_pointer() {
        use crate::ui::navigator::Geometry;
        let geometry = Geometry {
            aspect: 2.0,
            area: egui::vec2(800.0, 400.0),
        };
        // At the centre: no pan.
        let pan = pan_for_zoom(geometry, [0.0, 0.0], 100, 200, egui::Vec2::ZERO);
        assert!(pan[0].abs() < 1e-9 && pan[1].abs() < 1e-9);
        // 200 points right of the centre (a quarter of the picture): the
        // picture moves left by a quarter of its doubled width, 12.5 %.
        let pan = pan_for_zoom(geometry, [0.0, 0.0], 100, 200, egui::vec2(200.0, 0.0));
        assert!((pan[0] + 12.5).abs() < 1e-9, "{pan:?}");
        // And back.
        let back = pan_for_zoom(geometry, pan, 200, 100, egui::vec2(200.0, 0.0));
        assert!(back[0].abs() < 1e-9, "{back:?}");
        assert_eq!(wheel_zoom(100, 1), 125);
        assert_eq!(wheel_zoom(100, -1), 80);
        assert_eq!(wheel_zoom(390, 2), 400);
        assert_eq!(wheel_zoom(50, -1), 50);
    }

    #[test]
    fn a_dragged_picture_follows_the_pointer() {
        use crate::ui::navigator::Geometry;
        let geometry = Geometry {
            aspect: 2.0,
            area: egui::vec2(800.0, 400.0),
        };
        // At 200 % the picture is 1600 × 800: 160 points right is 10 %,
        // 40 up is -5 %.
        let pan = pan_by(geometry, [1.0, 0.0], 200, egui::vec2(160.0, -40.0));
        assert!(
            (pan[0] - 11.0).abs() < 1e-9 && (pan[1] + 5.0).abs() < 1e-9,
            "{pan:?}"
        );
        // No further than the panel goes.
        let pan = pan_by(geometry, [0.0, 0.0], 100, egui::vec2(-2000.0, 0.0));
        assert_eq!(pan[0], -crate::adjust::PAN_MAX);
    }

    /// Two cores kept on the same frame without a window, as the app keeps
    /// them: paused, after exact seeks and frame steps, B's position is
    /// A's; playing, the drift is printed. `cargo test compare_sync --
    /// --ignored --nocapture` with METALMPV_SAMPLE (A) and
    /// METALMPV_SAMPLE_B (B; A again without it), METALMPV_COMPARE_OFFSET.
    #[test]
    #[ignore = "needs vendor/libmpv and a sample video"]
    fn compare_sync_of_sample() {
        use crate::player::{Happened, Player, Seek};
        let a_path = std::env::var("METALMPV_SAMPLE")
            .unwrap_or_else(|_| "D:/Projects/video_converter/samples/test.mp4".into());
        let b_path = std::env::var("METALMPV_SAMPLE_B").unwrap_or_else(|_| a_path.clone());
        let offset: f64 = std::env::var("METALMPV_COMPARE_OFFSET")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let mut a = Player::headless();
        let mut b = Player::headless();
        a.load(Path::new(&a_path), None, None);
        let silent = crate::resume::Tracks {
            audio: None,
            sub: None,
            sub_file: None,
        };
        b.load(Path::new(&b_path), None, Some(silent));
        let mut sync = Sync::default();
        let mut loaded = false;
        // Runs the app's loop for `seconds`, or until `until` holds.
        let mut run = |a: &mut Player,
                       b: &mut Player,
                       seconds: f64,
                       until: &mut dyn FnMut(&Player, &Player, &Sync) -> bool|
         -> bool {
            let deadline = Instant::now() + Duration::from_secs_f64(seconds);
            while Instant::now() < deadline {
                let now = Instant::now();
                for event in a.poll() {
                    if event == Happened::Restarted {
                        sync.jumped();
                    }
                }
                for event in b.poll() {
                    match event {
                        Happened::FileLoaded => {
                            loaded = true;
                            sync.reset();
                        }
                        Happened::Restarted => sync.restarted(now),
                        _ => {}
                    }
                }
                let a_side = Side::of(&a.state, a.loading());
                let b_side = Side::of(&b.state, b.loading() || !loaded);
                for step in sync.follow(&a_side, &b_side, offset, now) {
                    match step {
                        Step::Seek(t) => b.seek_to(t, Seek::Exact),
                        Step::Pause(p) => b.set_pause(p),
                        Step::Speed(s) => b.set_speed(s),
                    }
                }
                if until(a, b, &sync) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(8));
            }
            false
        };
        // Settled: both still, B where A is plus the offset.
        let mut settled = |a: &Player, b: &Player, _: &Sync| {
            let (sa, sb) = (&a.state, &b.state);
            !sa.seeking
                && !sb.seeking
                && sb.paused
                && sa
                    .time
                    .zip(sb.time)
                    .is_some_and(|(ta, tb)| (tb - (ta + offset).max(0.0)).abs() < 0.002)
        };
        assert!(run(&mut a, &mut b, 10.0, &mut |a, _, _| a
            .state
            .time
            .is_some_and(|t| t > 1.0)));
        a.set_pause(true);
        assert!(run(&mut a, &mut b, 5.0, &mut settled), "paused");
        eprintln!("paused: A {:?}, B {:?}", a.state.time, b.state.time);
        a.seek_to(3.3, Seek::Exact);
        assert!(run(&mut a, &mut b, 5.0, &mut settled), "after a seek");
        eprintln!("seek: A {:?}, B {:?}", a.state.time, b.state.time);
        for i in 0..5 {
            let before = a.state.time;
            a.frame_step(true);
            run(&mut a, &mut b, 2.0, &mut |a, _, _| a.state.time != before);
            assert!(run(&mut a, &mut b, 5.0, &mut settled), "step {i}");
            eprintln!("step {i}: A {:?}, B {:?}", a.state.time, b.state.time);
        }
        a.frame_step(false);
        run(&mut a, &mut b, 0.5, &mut |_, _, _| false);
        assert!(run(&mut a, &mut b, 5.0, &mut settled), "a step back");
        // Playing: the drift as read, after B caught up.
        a.set_pause(false);
        run(&mut a, &mut b, 2.0, &mut |_, _, _| false);
        let mut drifts = Vec::new();
        run(&mut a, &mut b, 10.0, &mut |a, b, _| {
            if let (Some(ta), Some(tb)) = (a.state.time, b.state.time) {
                drifts.push(tb - offset - ta);
            }
            a.state.eof
        });
        let n = drifts.len().max(1) as f64;
        let mean = drifts.iter().sum::<f64>() / n;
        let mean_abs = drifts.iter().map(|d| d.abs()).sum::<f64>() / n;
        let worst = drifts.iter().fold(0.0f64, |m, d| m.max(d.abs()));
        eprintln!(
            "playing: {} readings, mean {:+.1} ms, mean |drift| {:.1} ms, worst {:.1} ms",
            drifts.len(),
            mean * 1e3,
            mean_abs * 1e3,
            worst * 1e3
        );
        assert!(mean_abs < 0.1, "B kept within 100 ms on average");
    }

    #[test]
    fn the_offset_counts_b_s_frames() {
        use crate::i18n::Lang;
        assert_eq!(frame_rate(&[]), 25.0);
        assert_eq!(shift_in(Lang::En, 0.0, 25.0), None);
        assert_eq!(
            shift_in(Lang::En, 0.12, 25.0).as_deref(),
            Some("B +3 frames (+0.120 s)")
        );
        assert_eq!(
            shift_in(Lang::Ru, -0.04, 25.0).as_deref(),
            Some("B −1 кадр (−0,040 с)")
        );
        assert_eq!(
            shift_in(Lang::Ru, 0.2, 25.0).as_deref(),
            Some("B +5 кадров (+0,200 с)")
        );
    }
}
