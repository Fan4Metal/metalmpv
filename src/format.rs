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
            Osd::Text(t) => t.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
