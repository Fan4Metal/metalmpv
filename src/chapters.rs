//! The chapters of the playing file, from mpv's `chapter-list`: the marks
//! on the seek bar, the section under the playlist and the context menu.

use crate::i18n::Lang;
use crate::mpv::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    /// The start in seconds.
    pub time: f64,
    /// The title as in the file, left out when it only numbers the chapter
    /// ("Chapter 01", "Глава 1"): the interface numbers them itself.
    pub title: Option<String>,
}

/// The chapters of a `chapter-list` value, by start; entries without a
/// time are left out.
pub fn from_value(list: &Value) -> Vec<Chapter> {
    let mut chapters: Vec<Chapter> = list
        .list()
        .iter()
        .filter_map(|c| {
            let time = c.get("time")?.double()?;
            let title = c
                .get("title")
                .and_then(Value::string)
                .map(str::trim)
                .filter(|t| !t.is_empty() && !only_numbers(t))
                .map(str::to_owned);
            time.is_finite().then(|| Chapter {
                time: time.max(0.0),
                title,
            })
        })
        .collect();
    chapters.sort_by(|a, b| a.time.total_cmp(&b.time));
    chapters
}

/// Whether a title says nothing but a chapter's number: "Chapter 01",
/// "Глава 3", "Часть 2", "05", "00:12:30.000" (mkvmerge's default from a
/// timestamp).
fn only_numbers(title: &str) -> bool {
    let lower = title.to_lowercase();
    let rest = ["chapter", "глава", "часть", "chap.", "ch."]
        .iter()
        .find_map(|w| lower.strip_prefix(w))
        .unwrap_or(&lower);
    let rest = rest.trim();
    !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.'))
}

/// The chapter playing at `time`: the last one that has started.
pub fn current(chapters: &[Chapter], time: f64) -> Option<usize> {
    // A little slack: an exact seek to a chapter can land a few
    // milliseconds before its start.
    chapters.iter().rposition(|c| c.time <= time + 0.05)
}

impl Chapter {
    /// The chapter's title, or "Chapter N" (`number` from 1) without one.
    pub fn name_in(&self, lang: Lang, number: usize) -> String {
        match &self.title {
            Some(t) => t.clone(),
            None => match lang {
                Lang::En => format!("Chapter {number}"),
                Lang::Ru => format!("Глава {number}"),
            },
        }
    }

    pub fn name(&self, number: usize) -> String {
        self.name_in(crate::i18n::lang(), number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(time: f64, title: &str) -> Value {
        Value::Map(vec![
            ("title".into(), Value::Str(title.into())),
            ("time".into(), Value::Double(time)),
        ])
    }

    #[test]
    fn chapters_of_a_list() {
        let list = Value::List(vec![
            entry(600.0, "Chapter 02"),
            entry(0.0, "Opening"),
            Value::Map(vec![("title".into(), Value::Str("no time".into()))]),
            entry(1200.5, ""),
        ]);
        let c = from_value(&list);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].title.as_deref(), Some("Opening"));
        assert_eq!(c[1].time, 600.0);
        assert_eq!(c[1].title, None);
        assert_eq!(c[1].name_in(Lang::Ru, 2), "Глава 2");
        assert_eq!(c[2].name_in(Lang::En, 3), "Chapter 3");
        assert!(from_value(&Value::None).is_empty());
    }

    #[test]
    fn numbering_titles() {
        for t in [
            "Chapter 01",
            "chapter 1",
            "Глава 3",
            "Часть 2",
            "05",
            "00:12:30.000",
        ] {
            assert!(only_numbers(t), "{t}");
        }
        for t in ["Opening", "Chapter One", "Глава 3. Схватка", "Chapter"] {
            assert!(!only_numbers(t), "{t}");
        }
    }

    #[test]
    fn current_chapter() {
        let c = vec![
            Chapter {
                time: 0.0,
                title: None,
            },
            Chapter {
                time: 100.0,
                title: None,
            },
        ];
        assert_eq!(current(&c, 0.0), Some(0));
        assert_eq!(current(&c, 99.0), Some(0));
        assert_eq!(current(&c, 99.98), Some(1));
        assert_eq!(current(&c, 500.0), Some(1));
        assert_eq!(current(&c[1..], 50.0), None);
    }
}
