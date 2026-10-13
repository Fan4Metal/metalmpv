//! Times, percentages and the OSD lines in the interface's language. Each
//! function has an `_in(lang, …)` form, so that tests do not depend on the
//! global language.

use crate::i18n::{Lang, lang};

/// A position or duration as `H:MM:SS`, or `MM:SS` under an hour; `long`
/// forces the hours (to match a duration of an hour or more). Fractions of
/// a second are dropped, as clocks do; a missing time is `--:--`.
pub fn time(seconds: Option<f64>, long: bool) -> String {
    let Some(s) = seconds.filter(|s| s.is_finite()) else {
        return if long {
            "-:--:--".into()
        } else {
            "--:--".into()
        };
    };
    let total = s.max(0.0) as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 || long {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// A time typed in the Go to time window: `1:23:45`, `83:20`, `5000`
/// (seconds), a fraction after `.` or `,` (`1:23,5`), a share of the
/// duration (`50%`), or a move from `now` with `+` or `-` (`+30`,
/// `-1:00`, `+10%`). Within the file: from 0 to `duration` when known.
/// `None` when it is not a time, or names what is not known (a share of
/// an unknown duration, a move without a position).
pub fn parse_time(text: &str, now: Option<f64>, duration: Option<f64>) -> Option<f64> {
    let text: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    let (sign, rest) = match text.chars().next()? {
        '+' => (Some(1.0), &text[1..]),
        '-' => (Some(-1.0), &text[1..]),
        _ => (None, text.as_str()),
    };
    let amount = match rest.strip_suffix('%') {
        Some(share) => {
            let share = number(share)?;
            (share <= 100.0).then_some(())?;
            duration.filter(|d| *d > 0.0)? * share / 100.0
        }
        None => {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() > 3 {
                return None;
            }
            let (last, whole) = parts.split_last()?;
            let mut seconds = 0.0;
            for (i, part) in whole.iter().enumerate() {
                (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())).then_some(())?;
                let value: f64 = part.parse().ok()?;
                // Minutes after hours stay under 60.
                (i == 0 || value < 60.0).then_some(())?;
                seconds = seconds * 60.0 + value;
            }
            let last = number(last)?;
            (whole.is_empty() || last < 60.0).then_some(())?;
            seconds * 60.0 + last
        }
    };
    let target = match sign {
        Some(sign) => now? + sign * amount,
        None => amount,
    };
    let target = target.max(0.0);
    let target = duration
        .filter(|d| *d > 0.0)
        .map_or(target, |d| target.min(d));
    target.is_finite().then_some(target)
}

/// A number of digits with an optional fraction (`12`, `12.5`, `.5`).
fn number(text: &str) -> Option<f64> {
    let digits = text.bytes().filter(u8::is_ascii_digit).count();
    let dots = text.bytes().filter(|b| *b == b'.').count();
    (digits > 0 && dots <= 1 && digits + dots == text.len()).then_some(())?;
    text.parse().ok()
}

/// The position and the duration, both with hours when the file is an hour
/// or longer: `01:23 / 45:00`, `0:01:23 / 1:45:00`.
pub fn position(time_pos: Option<f64>, duration: Option<f64>) -> String {
    let long = duration.is_some_and(|d| d >= 3600.0);
    format!("{} / {}", time(time_pos, long), time(duration, long))
}

/// What the control bar's time shows; a click on it moves to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeMode {
    /// The position and the duration (`position`).
    #[default]
    Both,
    /// The position only.
    Elapsed,
    /// The time left, with a minus.
    Remaining,
}

impl TimeMode {
    /// Both → elapsed → remaining → both.
    pub fn next(self) -> Self {
        match self {
            Self::Both => Self::Elapsed,
            Self::Elapsed => Self::Remaining,
            Self::Remaining => Self::Both,
        }
    }

    /// The name in the settings.
    pub fn name(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Elapsed => "elapsed",
            Self::Remaining => "remaining",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Both, Self::Elapsed, Self::Remaining]
            .into_iter()
            .find(|m| m.name() == name)
    }
}

/// The control bar's time in `mode`: `01:23 / 45:00`, `01:23`, `-43:37`;
/// with hours when the file is an hour or longer.
pub fn time_line(mode: TimeMode, time_pos: Option<f64>, duration: Option<f64>) -> String {
    let long = duration.is_some_and(|d| d >= 3600.0);
    match mode {
        TimeMode::Both => position(time_pos, duration),
        TimeMode::Elapsed => time(time_pos, long),
        TimeMode::Remaining => match (time_pos, duration) {
            (Some(t), Some(d)) if t.is_finite() && d.is_finite() => {
                // Rounded up, so that the two readouts add up to the duration.
                format!("-{}", time(Some((d - t).max(0.0).ceil()), long))
            }
            _ => time(None, long),
        },
    }
}

/// The OSD's clock: always `HH:MM:SS`.
/// `HH:MM:SS.t`, the tenths after the decimal sign of `lang` (the A-B
/// repeat's points, set to a frame).
pub fn clock_tenths_in(lang: Lang, seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    let sign = if lang == Lang::Ru { ',' } else { '.' };
    format!("{}{sign}{}", clock((tenths / 10) as f64), tenths % 10)
}

pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        total / 60 % 60,
        total % 60
    )
}

/// A share (0 to 1) as a percentage with one decimal: `12.5%`, `12,5%`.
pub fn percent_in(lang: Lang, share: f64) -> String {
    let text = format!("{:.1}%", (share * 100.0).clamp(0.0, 100.0));
    match lang {
        Lang::En => text,
        Lang::Ru => text.replace('.', ","),
    }
}

/// The steps of the playback speed (`C`, `X`; `Z` is 1).
pub const SPEEDS: [f64; 11] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0];

/// The speed after `speed` by `steps` steps of [`SPEEDS`] (0: normal);
/// from between two steps, one step is to the next.
pub fn speed_step(speed: f64, steps: i32) -> f64 {
    if steps == 0 {
        return 1.0;
    }
    let found = if steps > 0 {
        SPEEDS.iter().position(|s| *s > speed + 1e-9)
    } else {
        SPEEDS.iter().rposition(|s| *s < speed - 1e-9)
    };
    match found {
        Some(i) => {
            let i = i as i32 + steps.signum() * (steps.abs() - 1);
            SPEEDS[i.clamp(0, SPEEDS.len() as i32 - 1) as usize]
        }
        None if steps > 0 => SPEEDS[SPEEDS.len() - 1],
        None => SPEEDS[0],
    }
}

/// A speed: `1.25×`, `0.5×`; `1,25×`.
pub fn speed_in(lang: Lang, speed: f64) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    let text = match lang {
        Lang::En => text.to_owned(),
        Lang::Ru => text.replace('.', ","),
    };
    format!("{text}×")
}

/// A seek step: `10 s`, `1 min`, `1:30 min`; `10 сек`, `1 мин`.
pub fn step_in(lang: Lang, seconds: f64) -> String {
    let s = seconds.abs();
    let (sec, min) = match lang {
        Lang::En => ("s", "min"),
        Lang::Ru => ("сек", "мин"),
    };
    if s < 60.0 {
        let n = if s.fract() == 0.0 {
            format!("{s:.0}")
        } else {
            format!("{s:.1}")
        };
        let n = if lang == Lang::Ru {
            n.replace('.', ",")
        } else {
            n
        };
        format!("{n} {sec}")
    } else if s % 60.0 == 0.0 {
        format!("{:.0} {min}", s / 60.0)
    } else {
        let total = s as u64;
        format!("{}:{:02} {min}", total / 60, total % 60)
    }
}

/// What the OSD reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Osd {
    /// A seek by `delta` seconds, on a keyframe or exact, to `time` of
    /// `duration`.
    Seek {
        delta: f64,
        exact: bool,
        time: f64,
        duration: Option<f64>,
    },
    /// A move to `time` (the seek bar, Home, End).
    Jump {
        time: f64,
        duration: Option<f64>,
    },
    Frame {
        forward: bool,
    },
    Volume(f64),
    Mute(bool),
    Pause(bool),
    /// The audio track or the subtitles now ("2/3: English (AAC · 2.0)";
    /// `None`: no subtitles).
    Track {
        audio: bool,
        now: Option<String>,
    },
    /// A move to the start of chapter `number` (from 1), named by its title
    /// if it has one.
    Chapter {
        number: usize,
        title: Option<String>,
    },
    /// Where playback is (`O`): `time` of `duration`, and the chapter
    /// playing (its number from 1 and its title) if the file has chapters.
    Position {
        time: f64,
        duration: Option<f64>,
        chapter: Option<(usize, Option<String>)>,
    },
    /// The window sized for the picture at `percent` of the video's size.
    Scale(f64),
    /// The largest window the screen holds, without the bars.
    FitScreen,
    /// The window sized to the picture, without the bars.
    FitWindow,
    /// Fullscreen with the picture filling the screen, cropped, or the
    /// picture whole again.
    Fill(bool),
    /// The picture's shape forced (its label), or the file's (`None`).
    Aspect(Option<String>),
    /// The sound against the picture, in seconds (later when positive).
    AudioDelay(f64),
    /// The subtitles against the picture, in seconds.
    SubDelay(f64),
    /// The playback speed.
    Speed(f64),
    /// When the window stays over the others.
    OnTop(crate::settings::OnTop),
    /// What plays after the end of a file.
    Repeat(crate::playlist::Repeat),
    /// The A-B repeat's points (none: off).
    Loop {
        a: Option<f64>,
        b: Option<f64>,
    },
    /// Free text: a file name, an error.
    Text(String),
}

impl Osd {
    pub fn text(&self) -> String {
        self.text_in(lang())
    }

    pub fn text_in(&self, lang: Lang) -> String {
        let ru = lang == Lang::Ru;
        let at = |time: f64, duration: Option<f64>| match duration.filter(|d| *d > 0.0) {
            Some(d) => format!("{} ({})", clock(time), percent_in(lang, time / d)),
            None => clock(time),
        };
        match self {
            Osd::Seek {
                delta,
                exact,
                time,
                duration,
            } => {
                let way = match (*delta < 0.0, ru) {
                    (true, false) => "Back",
                    (false, false) => "Forward",
                    (true, true) => "Назад",
                    (false, true) => "Вперёд",
                };
                let how = match (exact, ru) {
                    (false, false) => "Keyframe",
                    (true, false) => "Exact",
                    (false, true) => "Ключевой кадр",
                    (true, true) => "Точно",
                };
                format!(
                    "[ {way} {} / {how} / {} ]",
                    step_in(lang, *delta),
                    at(*time, *duration)
                )
            }
            Osd::Jump { time, duration } => format!("[ {} ]", at(*time, *duration)),
            Osd::Frame { forward } => match (forward, ru) {
                (true, false) => "[ Next frame ]".into(),
                (false, false) => "[ Previous frame ]".into(),
                (true, true) => "[ Следующий кадр ]".into(),
                (false, true) => "[ Предыдущий кадр ]".into(),
            },
            Osd::Volume(v) => {
                if ru {
                    format!("[ Громкость {v:.0}% ]")
                } else {
                    format!("[ Volume {v:.0}% ]")
                }
            }
            Osd::Mute(on) => match (on, ru) {
                (true, false) => "[ Mute ]".into(),
                (false, false) => "[ Sound on ]".into(),
                (true, true) => "[ Без звука ]".into(),
                (false, true) => "[ Звук включён ]".into(),
            },
            Osd::Pause(on) => match (on, ru) {
                (true, false) => "[ Pause ]".into(),
                (false, false) => "[ Play ]".into(),
                (true, true) => "[ Пауза ]".into(),
                (false, true) => "[ Воспроизведение ]".into(),
            },
            Osd::Track { audio, now } => match (audio, now, ru) {
                (true, Some(t), false) => format!("[ Audio {t} ]"),
                (true, Some(t), true) => format!("[ Звук {t} ]"),
                (true, None, false) => "[ No audio ]".into(),
                (true, None, true) => "[ Звук выключен ]".into(),
                (false, Some(t), false) => format!("[ Subtitles {t} ]"),
                (false, Some(t), true) => format!("[ Субтитры {t} ]"),
                (false, None, false) => "[ Subtitles off ]".into(),
                (false, None, true) => "[ Субтитры выключены ]".into(),
            },
            Osd::Chapter { number, title } => {
                let chapter = if ru { "Глава" } else { "Chapter" };
                match title {
                    Some(t) => format!("[ {chapter} {number}: {t} ]"),
                    None => format!("[ {chapter} {number} ]"),
                }
            }
            Osd::Position {
                time,
                duration,
                chapter,
            } => {
                let mut text = match duration.filter(|d| *d > 0.0) {
                    Some(d) => format!(
                        "{} / {} ({})",
                        clock(*time),
                        clock(d),
                        percent_in(lang, time / d)
                    ),
                    None => clock(*time),
                };
                if let Some((number, title)) = chapter {
                    let word = if ru { "Глава" } else { "Chapter" };
                    match title {
                        Some(t) => text.push_str(&format!(" · {word} {number}: {t}")),
                        None => text.push_str(&format!(" · {word} {number}")),
                    }
                }
                format!("[ {text} ]")
            }
            Osd::Scale(percent) => {
                if ru {
                    format!("[ Размер {percent:.0}% ]")
                } else {
                    format!("[ Size {percent:.0}% ]")
                }
            }
            Osd::FitScreen => match ru {
                false => "[ Largest window without black bars ]".into(),
                true => "[ Развёрнуто без чёрных полос ]".into(),
            },
            Osd::FitWindow => match ru {
                false => "[ Window fitted to the picture ]".into(),
                true => "[ Окно по размеру картинки ]".into(),
            },
            Osd::Fill(on) => match (on, ru) {
                (true, false) => "[ Fullscreen without black bars ]".into(),
                (false, false) => "[ Whole picture ]".into(),
                (true, true) => "[ Полный экран без чёрных полос ]".into(),
                (false, true) => "[ Картинка целиком ]".into(),
            },
            Osd::Aspect(shape) => match (shape, ru) {
                (Some(s), false) => format!("[ Aspect ratio {s} ]"),
                (Some(s), true) => format!("[ Пропорции {s} ]"),
                (None, false) => "[ Aspect ratio as in the file ]".into(),
                (None, true) => "[ Пропорции как в файле ]".into(),
            },
            Osd::OnTop(on_top) => {
                use crate::settings::OnTop;
                match (on_top, ru) {
                    (OnTop::Never, false) => "[ On top: never ]".into(),
                    (OnTop::Always, false) => "[ On top: always ]".into(),
                    (OnTop::Playing, false) => "[ On top: while playing ]".into(),
                    (OnTop::Never, true) => "[ Поверх окон: никогда ]".into(),
                    (OnTop::Always, true) => "[ Поверх окон: всегда ]".into(),
                    (OnTop::Playing, true) => "[ Поверх окон: во время воспроизведения ]".into(),
                }
            }
            Osd::Repeat(repeat) => {
                use crate::playlist::Repeat;
                match (repeat, ru) {
                    (Repeat::Off, false) => "[ Repeat off ]".into(),
                    (Repeat::One, false) => "[ Repeat the file ]".into(),
                    (Repeat::All, false) => "[ Repeat the list ]".into(),
                    (Repeat::Off, true) => "[ Повтор выключен ]".into(),
                    (Repeat::One, true) => "[ Повтор файла ]".into(),
                    (Repeat::All, true) => "[ Повтор плейлиста ]".into(),
                }
            }
            Osd::Loop { a, b } => {
                let at = |t: f64| clock_tenths_in(lang, t);
                match (a, b, ru) {
                    (Some(a), Some(b), false) => format!("[ A-B repeat {} – {} ]", at(*a), at(*b)),
                    (Some(a), Some(b), true) => format!("[ Повтор A-B {} – {} ]", at(*a), at(*b)),
                    (Some(a), None, false) => format!("[ A-B repeat from {} ]", at(*a)),
                    (Some(a), None, true) => format!("[ Повтор A-B с {} ]", at(*a)),
                    (None, Some(b), false) => format!("[ A-B repeat to {} ]", at(*b)),
                    (None, Some(b), true) => format!("[ Повтор A-B до {} ]", at(*b)),
                    (None, None, false) => "[ A-B repeat off ]".into(),
                    (None, None, true) => "[ Повтор A-B выключен ]".into(),
                }
            }
            Osd::Speed(speed) => {
                let s = speed_in(lang, *speed);
                if ru {
                    format!("[ Скорость {s} ]")
                } else {
                    format!("[ Speed {s} ]")
                }
            }
            Osd::AudioDelay(seconds) | Osd::SubDelay(seconds) => {
                let ms = (seconds * 1000.0).round();
                let ms = if ms == 0.0 {
                    "0".to_owned()
                } else {
                    format!("{ms:+.0}")
                };
                match (matches!(self, Osd::SubDelay(_)), ru) {
                    (false, true) => format!("[ Задержка звука {ms} мс ]"),
                    (false, false) => format!("[ Audio delay {ms} ms ]"),
                    (true, true) => format!("[ Задержка субтитров {ms} мс ]"),
                    (true, false) => format!("[ Subtitle delay {ms} ms ]"),
                }
            }
            Osd::Text(t) => t.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_steps() {
        assert_eq!(speed_step(1.0, 1), 1.25);
        assert_eq!(speed_step(1.0, -1), 0.75);
        assert_eq!(speed_step(4.0, 1), 4.0);
        assert_eq!(speed_step(0.25, -1), 0.25);
        assert_eq!(speed_step(1.1, 1), 1.25);
        assert_eq!(speed_step(1.1, -1), 1.0);
        assert_eq!(speed_step(3.0, 0), 1.0);
    }

    #[test]
    fn times() {
        assert_eq!(time(Some(0.0), false), "00:00");
        assert_eq!(time(Some(59.9), false), "00:59");
        assert_eq!(time(Some(3599.5), false), "59:59");
        assert_eq!(time(Some(3600.0), false), "1:00:00");
        assert_eq!(time(Some(83.0), true), "0:01:23");
        assert_eq!(time(None, false), "--:--");
        assert_eq!(time(Some(f64::NAN), false), "--:--");
        assert_eq!(time(Some(-5.0), false), "00:00");
        assert_eq!(position(Some(83.0), Some(6360.0)), "0:01:23 / 1:46:00");
        assert_eq!(position(Some(83.0), Some(600.0)), "01:23 / 10:00");
        assert_eq!(clock(83.4), "00:01:23");
    }

    #[test]
    fn time_modes() {
        let (t, d) = (Some(83.4), Some(600.0));
        assert_eq!(time_line(TimeMode::Both, t, d), "01:23 / 10:00");
        assert_eq!(time_line(TimeMode::Elapsed, t, d), "01:23");
        assert_eq!(time_line(TimeMode::Remaining, t, d), "-08:37");
        assert_eq!(
            time_line(TimeMode::Remaining, Some(0.0), Some(6360.0)),
            "-1:46:00"
        );
        assert_eq!(
            time_line(TimeMode::Elapsed, Some(83.0), Some(6360.0)),
            "0:01:23"
        );
        assert_eq!(time_line(TimeMode::Remaining, Some(700.0), d), "-00:00");
        assert_eq!(time_line(TimeMode::Remaining, t, None), "--:--");
        let mut m = TimeMode::default();
        for expected in [TimeMode::Elapsed, TimeMode::Remaining, TimeMode::Both] {
            m = m.next();
            assert_eq!(m, expected);
            assert_eq!(TimeMode::from_name(m.name()), Some(m));
        }
        assert_eq!(TimeMode::from_name("total"), None);
    }

    #[test]
    fn percents_and_steps() {
        assert_eq!(percent_in(Lang::En, 0.125), "12.5%");
        assert_eq!(percent_in(Lang::Ru, 0.125), "12,5%");
        assert_eq!(percent_in(Lang::En, 1.5), "100.0%");
        assert_eq!(step_in(Lang::En, -10.0), "10 s");
        assert_eq!(step_in(Lang::Ru, 1.0), "1 сек");
        assert_eq!(step_in(Lang::Ru, 0.5), "0,5 сек");
        assert_eq!(step_in(Lang::En, 60.0), "1 min");
        assert_eq!(step_in(Lang::Ru, 90.0), "1:30 мин");
    }

    #[test]
    fn times_typed() {
        let d = Some(2.0 * 3600.0);
        let now = Some(600.0);
        let at = |text: &str| parse_time(text, now, d);
        assert_eq!(at("1:23:45"), Some(5025.0));
        assert_eq!(at(" 83:20 "), Some(5000.0));
        assert_eq!(at("5000"), Some(5000.0));
        assert_eq!(at("1:23,5"), Some(83.5));
        assert_eq!(at("0:05.25"), Some(5.25));
        assert_eq!(at("50%"), Some(3600.0));
        assert_eq!(at("+30"), Some(630.0));
        assert_eq!(at("-1:00"), Some(540.0));
        assert_eq!(at("+10%"), Some(1320.0));
        // Kept within the file.
        assert_eq!(at("-20:00"), Some(0.0));
        assert_eq!(at("5:00:00"), Some(7200.0));
        // Not times.
        for text in [
            "", "abc", "1:60", "1:2:3:4", "1::2", ":30", "1.5:00", "150%", "1e3", "--5", "1.2.3",
        ] {
            assert_eq!(at(text), None, "{text:?}");
        }
        // What is not known.
        assert_eq!(parse_time("50%", now, None), None);
        assert_eq!(parse_time("+30", None, d), None);
        // A stream without a duration: any time on.
        assert_eq!(parse_time("3:00:00", now, None), Some(10800.0));
    }

    #[test]
    fn osd_lines() {
        let seek = Osd::Seek {
            delta: -1.0,
            exact: false,
            time: 0.0,
            duration: Some(106.0),
        };
        assert_eq!(
            seek.text_in(Lang::Ru),
            "[ Назад 1 сек / Ключевой кадр / 00:00:00 (0,0%) ]"
        );
        assert_eq!(
            seek.text_in(Lang::En),
            "[ Back 1 s / Keyframe / 00:00:00 (0.0%) ]"
        );
        let seek = Osd::Seek {
            delta: 10.0,
            exact: true,
            time: 83.0,
            duration: None,
        };
        assert_eq!(
            seek.text_in(Lang::En),
            "[ Forward 10 s / Exact / 00:01:23 ]"
        );
        assert_eq!(Osd::Volume(55.0).text_in(Lang::Ru), "[ Громкость 55% ]");
        assert_eq!(
            Osd::Frame { forward: false }.text_in(Lang::En),
            "[ Previous frame ]"
        );
        let sub = Osd::Track {
            audio: false,
            now: Some("1/2: Русский (SRT)".into()),
        };
        assert_eq!(sub.text_in(Lang::Ru), "[ Субтитры 1/2: Русский (SRT) ]");
        assert_eq!(
            Osd::Track {
                audio: false,
                now: None
            }
            .text_in(Lang::En),
            "[ Subtitles off ]"
        );
        let chapter = |title: Option<&str>| Osd::Chapter {
            number: 3,
            title: title.map(str::to_owned),
        };
        assert_eq!(
            chapter(Some("The Quickening")).text_in(Lang::Ru),
            "[ Глава 3: The Quickening ]"
        );
        assert_eq!(chapter(None).text_in(Lang::En), "[ Chapter 3 ]");
        let position = |chapter| Osd::Position {
            time: 2530.0,
            duration: Some(6537.0),
            chapter,
        };
        assert_eq!(
            position(None).text_in(Lang::Ru),
            "[ 00:42:10 / 01:48:57 (38,7%) ]"
        );
        assert_eq!(
            position(Some((3, Some("The Quickening".into())))).text_in(Lang::En),
            "[ 00:42:10 / 01:48:57 (38.7%) · Chapter 3: The Quickening ]"
        );
        assert_eq!(Osd::Scale(50.0).text_in(Lang::Ru), "[ Размер 50% ]");
        assert_eq!(Osd::Scale(100.0).text_in(Lang::En), "[ Size 100% ]");
        assert_eq!(
            Osd::Fill(true).text_in(Lang::Ru),
            "[ Полный экран без чёрных полос ]"
        );
        assert_eq!(Osd::Fill(false).text_in(Lang::En), "[ Whole picture ]");
        assert_eq!(Osd::Speed(1.25).text_in(Lang::Ru), "[ Скорость 1,25× ]");
        assert_eq!(Osd::Speed(2.0).text_in(Lang::En), "[ Speed 2× ]");
        let both = Osd::Loop {
            a: Some(83.44),
            b: Some(3725.0),
        };
        assert_eq!(
            both.text_in(Lang::En),
            "[ A-B repeat 00:01:23.4 – 01:02:05.0 ]"
        );
        assert_eq!(
            both.text_in(Lang::Ru),
            "[ Повтор A-B 00:01:23,4 – 01:02:05,0 ]"
        );
        let start = Osd::Loop {
            a: Some(0.96),
            b: None,
        };
        assert_eq!(start.text_in(Lang::En), "[ A-B repeat from 00:00:01.0 ]");
        let off = Osd::Loop { a: None, b: None };
        assert_eq!(off.text_in(Lang::Ru), "[ Повтор A-B выключен ]");
        assert_eq!(speed_in(Lang::En, 0.5), "0.5×");
        assert_eq!(
            Osd::AudioDelay(0.3).text_in(Lang::Ru),
            "[ Задержка звука +300 мс ]"
        );
        assert_eq!(
            Osd::AudioDelay(-0.1).text_in(Lang::En),
            "[ Audio delay -100 ms ]"
        );
        assert_eq!(
            Osd::AudioDelay(-0.0).text_in(Lang::En),
            "[ Audio delay 0 ms ]"
        );
        assert_eq!(
            Osd::SubDelay(-0.2).text_in(Lang::Ru),
            "[ Задержка субтитров -200 мс ]"
        );
    }
}
