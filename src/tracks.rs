//! The audio and subtitle tracks of the playing file, from mpv's
//! `track-list`, with their names as the interface shows them and the
//! order the keys cycle through them in.

use crate::i18n::Lang;
use crate::mpv::Value;

/// Extensions of subtitle files: the file dialog's filter, and dropped
/// files that go with the playing video instead of replacing it.
pub const SUBTITLE_EXTENSIONS: &[&str] = &[
    "srt", "ass", "ssa", "sub", "idx", "vtt", "sup", "smi", "txt",
];

/// Whether `path` is a subtitle file by its extension (`.txt` aside: too
/// many text files are not subtitles).
pub fn is_subtitles(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        !e.eq_ignore_ascii_case("txt")
            && SUBTITLE_EXTENSIONS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(e))
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Kind {
    #[default]
    Video,
    Audio,
    Sub,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// mpv's id (`aid`/`sid`), counted per kind from 1.
    pub id: i64,
    pub kind: Kind,
    pub title: Option<String>,
    /// ISO 639 code as in the file (`rus`, `en`).
    pub lang: Option<String>,
    /// FFmpeg's codec name (`ac3`, `subrip`).
    pub codec: Option<String>,
    pub channels: Option<i64>,
    /// The channel layout (`5.1(side)`).
    pub layout: Option<String>,
    /// FFmpeg's long codec name ("H.265 / HEVC (High Efficiency Video Coding)").
    pub codec_desc: Option<String>,
    /// The codec's profile ("High", "Main 10", "LC", "DTS-HD MA").
    pub profile: Option<String>,
    /// A picture's size in pixels as the file states it.
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// The pixels' aspect ratio, 1 for square pixels.
    pub par: Option<f64>,
    /// Degrees clockwise the picture is turned for display.
    pub rotation: Option<i64>,
    /// Dolby Vision's profile and level.
    pub dolby_vision: Option<(i64, i64)>,
    pub fps: Option<f64>,
    /// Audio samples per second.
    pub samplerate: Option<i64>,
    /// Bits per second as the file states it.
    pub bitrate: Option<i64>,
    /// A cover picture, not a video.
    pub albumart: bool,
    /// Loaded from a file of its own (subtitles next to the video).
    pub external: bool,
    pub default: bool,
    pub forced: bool,
    pub commentary: bool,
    pub original: bool,
    pub hearing_impaired: bool,
    pub visual_impaired: bool,
    pub selected: bool,
    /// The track's own tags, as the file names them; the language and
    /// FFmpeg's default handler names left out.
    pub tags: Vec<(String, String)>,
}

/// The tracks of a `track-list` value; entries that are not understood
/// are left out.
pub fn from_value(list: &Value) -> Vec<Track> {
    list.list()
        .iter()
        .filter_map(|t| {
            let kind = match t.get("type")?.string()? {
                "video" => Kind::Video,
                "audio" => Kind::Audio,
                "sub" => Kind::Sub,
                _ => return None,
            };
            let text = |key| {
                t.get(key)
                    .and_then(Value::string)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            };
            let flag = |key| t.get(key).is_some_and(Value::flag);
            Some(Track {
                id: t.get("id")?.int()?,
                kind,
                title: text("title"),
                lang: text("lang"),
                codec: text("codec"),
                channels: t.get("demux-channel-count").and_then(Value::int),
                layout: text("demux-channels"),
                codec_desc: text("codec-desc"),
                profile: text("codec-profile"),
                width: t.get("demux-w").and_then(Value::int),
                height: t.get("demux-h").and_then(Value::int),
                par: t
                    .get("demux-par")
                    .and_then(Value::double)
                    .filter(|p| *p > 0.0),
                rotation: t
                    .get("demux-rotation")
                    .and_then(Value::int)
                    .filter(|r| *r != 0),
                dolby_vision: t.get("dolby-vision-profile").and_then(Value::int).map(|p| {
                    (
                        p,
                        t.get("dolby-vision-level")
                            .and_then(Value::int)
                            .unwrap_or(0),
                    )
                }),
                fps: t.get("demux-fps").and_then(Value::double),
                samplerate: t.get("demux-samplerate").and_then(Value::int),
                bitrate: t.get("demux-bitrate").and_then(Value::int),
                albumart: flag("albumart"),
                external: flag("external"),
                default: flag("default"),
                forced: flag("forced"),
                commentary: flag("commentary"),
                original: flag("original"),
                hearing_impaired: flag("hearing-impaired"),
                visual_impaired: flag("visual-impaired"),
                selected: flag("selected"),
                tags: t.get("metadata").map(track_tags).unwrap_or_default(),
            })
        })
        .collect()
}

/// A track's `metadata` map without what says nothing new: the language
/// (the track has it), FFmpeg's default handler names ("SoundHandler") and
/// empty values.
fn track_tags(metadata: &Value) -> Vec<(String, String)> {
    let Value::Map(entries) = metadata else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.string()?.trim().to_owned())))
        .filter(|(k, v)| {
            !v.is_empty()
                && !k.eq_ignore_ascii_case("language")
                && !(k.eq_ignore_ascii_case("handler_name") && v.ends_with("Handler"))
        })
        .collect()
}

/// What a track chosen by hand is like: its language and title, to choose
/// the most alike in a file opened for the first time (the same voice-over
/// in the next episode of a series; `prefer_last_tracks`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Likeness {
    /// The language as English names it, in lower case ("russian").
    pub lang: Option<String>,
    /// The title, in lower case.
    pub title: Option<String>,
}

impl Likeness {
    pub fn of(track: &Track) -> Self {
        Self {
            lang: track
                .lang
                .as_deref()
                .map(|code| language_in(Lang::En, code).to_lowercase()),
            title: track
                .title
                .as_deref()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty()),
        }
    }

    /// How alike `track` is: the title counts more than the language.
    fn score(&self, track: &Track) -> u8 {
        let other = Self::of(track);
        let same = |a: &Option<String>, b: &Option<String>| a.is_some() && a == b;
        2 * u8::from(same(&self.title, &other.title)) + u8::from(same(&self.lang, &other.lang))
    }

    /// `language<TAB>title`, empty for what is not known.
    pub fn to_text(&self) -> String {
        format!(
            "{}\t{}",
            self.lang.as_deref().unwrap_or_default(),
            self.title.as_deref().unwrap_or_default()
        )
    }

    pub fn parse(text: &str) -> Option<Self> {
        let (lang, title) = text.split_once('\t')?;
        let known = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        let like = Self {
            lang: known(lang),
            title: known(title),
        };
        (like != Self::default()).then_some(like)
    }
}

/// The track of `kind` most like `like`, the first of equals; `None` when
/// none shares its title or language.
pub fn most_like(tracks: &[Track], kind: Kind, like: &Likeness) -> Option<i64> {
    let mut best: Option<(u8, i64)> = None;
    for t in of_kind(tracks, kind) {
        let score = like.score(t);
        if score > 0 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, t.id));
        }
    }
    best.map(|(_, id)| id)
}

/// The tracks of `kind`, in the file's order.
pub fn of_kind(tracks: &[Track], kind: Kind) -> Vec<&Track> {
    tracks.iter().filter(|t| t.kind == kind).collect()
}

/// The selected track of `kind`.
pub fn selected(tracks: &[Track], kind: Kind) -> Option<&Track> {
    tracks.iter().find(|t| t.kind == kind && t.selected)
}

/// The track after the selected one of `kind`, round: audio goes from the
/// last back to the first; subtitles pass through "off" (`None`) after the
/// last. `None` overall when there is nothing to switch to.
pub fn next(tracks: &[Track], kind: Kind) -> Option<Option<i64>> {
    let ids: Vec<i64> = of_kind(tracks, kind).iter().map(|t| t.id).collect();
    if ids.is_empty() {
        return None;
    }
    let current = selected(tracks, kind).map(|t| t.id);
    let position = current.and_then(|c| ids.iter().position(|id| *id == c));
    match (kind, position) {
        (Kind::Sub, None) => Some(Some(ids[0])),
        (Kind::Sub, Some(i)) if i + 1 == ids.len() => Some(None),
        (_, Some(i)) => Some(Some(ids[(i + 1) % ids.len()])),
        (_, None) => Some(Some(ids[0])),
    }
    .filter(|next| ids.len() > 1 || kind == Kind::Sub || *next != current)
}

impl Track {
    /// The track's number among those of its kind, from 1.
    pub fn number(&self, tracks: &[Track]) -> usize {
        of_kind(tracks, self.kind)
            .iter()
            .position(|t| t.id == self.id)
            .map_or(0, |i| i + 1)
    }

    /// What a track is chosen by: the language, then the title after a
    /// dash ("Русский — Superbit"); the title alone when it names the
    /// language itself or there is none; "Track N" when neither is known.
    pub fn name_in(&self, lang: Lang, number: usize) -> String {
        let language = self.lang.as_deref().map(|c| language_in(lang, c));
        match (language, &self.title) {
            (Some(l), Some(t)) if !self.title_names_language() => format!("{l} — {t}"),
            (_, Some(t)) => t.clone(),
            (Some(l), None) => l,
            (None, None) => match lang {
                Lang::En => format!("Track {number}"),
                Lang::Ru => format!("Дорожка {number}"),
            },
        }
    }

    /// Whether the title already says the language ("Russian Dub",
    /// "Русский", "rus").
    fn title_names_language(&self) -> bool {
        let (Some(title), Some(code)) = (&self.title, &self.lang) else {
            return false;
        };
        let title = words(title);
        let names = [
            language_in(Lang::En, code),
            language_in(Lang::Ru, code),
            code.clone(),
        ];
        // Whole words: "en" is in "Commentary", "no" in "Mono".
        names.iter().any(|n| {
            let name = words(n);
            !name.is_empty() && title.windows(name.len()).any(|w| w == name.as_slice())
        })
    }

    /// The format, the channels and the marks (external, forced, default),
    /// each shown as a badge under the name.
    pub fn badges_in(&self, lang: Lang) -> Vec<String> {
        let ru = lang == Lang::Ru;
        let mut badges = Vec::new();
        if let Some(codec) = &self.codec {
            badges.push(codec_name(codec));
        }
        if let Some(n) = self.channels {
            badges.push(channels_in(lang, n));
        }
        for (on, en, ru_text) in [
            (self.external, "external", "внешние"),
            (self.forced, "forced", "принудительные"),
            (self.default, "default", "по умолчанию"),
        ] {
            if on {
                badges.push(if ru { ru_text } else { en }.into());
            }
        }
        badges
    }

    /// The badges in one line: "AC3 · 5.1 · default".
    pub fn details_in(&self, lang: Lang) -> String {
        self.badges_in(lang).join(" · ")
    }

    /// "2/3: English (AAC · 2.0)", for the OSD.
    pub fn summary_in(&self, lang: Lang, tracks: &[Track]) -> String {
        let n = self.number(tracks);
        let total = of_kind(tracks, self.kind).len();
        let details = self.details_in(lang);
        let name = self.name_in(lang, n);
        if details.is_empty() {
            format!("{n}/{total}: {name}")
        } else {
            format!("{n}/{total}: {name} ({details})")
        }
    }
}

/// The codec as people know the format.
pub fn codec_name(codec: &str) -> String {
    match codec {
        "subrip" | "srt" => "SRT".into(),
        "ass" => "ASS".into(),
        "ssa" => "SSA".into(),
        "hdmv_pgs_subtitle" => "PGS".into(),
        "dvd_subtitle" => "VobSub".into(),
        "dvb_subtitle" => "DVB".into(),
        "webvtt" => "WebVTT".into(),
        "mov_text" => "TX3G".into(),
        "eac3" => "E-AC3".into(),
        "truehd" => "TrueHD".into(),
        "vorbis" => "Vorbis".into(),
        "opus" => "Opus".into(),
        "h264" => "H.264".into(),
        "mpeg2video" => "MPEG-2".into(),
        "mpeg4" => "MPEG-4".into(),
        "mjpeg" => "JPEG".into(),
        c if c.starts_with("pcm_") => "PCM".into(),
        c => c.to_uppercase(),
    }
}

pub fn channels_in(lang: Lang, n: i64) -> String {
    match (n, lang) {
        (1, Lang::En) => "mono".into(),
        (1, Lang::Ru) => "моно".into(),
        (2, _) => "2.0".into(),
        (3, _) => "2.1".into(),
        (6, _) => "5.1".into(),
        (8, _) => "7.1".into(),
        (n, Lang::En) => format!("{n} ch"),
        (n, Lang::Ru) => format!("{n} кан."),
    }
}

/// Languages by their ISO 639-1 and 639-2 codes, in English and Russian.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["ru", "rus"], "Russian", "Русский"),
    (&["en", "eng"], "English", "Английский"),
    (&["uk", "ukr"], "Ukrainian", "Украинский"),
    (&["be", "bel"], "Belarusian", "Белорусский"),
    (&["kk", "kaz"], "Kazakh", "Казахский"),
    (&["de", "ger", "deu"], "German", "Немецкий"),
    (&["fr", "fre", "fra"], "French", "Французский"),
    (&["es", "spa"], "Spanish", "Испанский"),
    (&["it", "ita"], "Italian", "Итальянский"),
    (&["pt", "por"], "Portuguese", "Португальский"),
    (&["pl", "pol"], "Polish", "Польский"),
    (&["cs", "cze", "ces"], "Czech", "Чешский"),
    (&["nl", "dut", "nld"], "Dutch", "Нидерландский"),
    (&["sv", "swe"], "Swedish", "Шведский"),
    (&["fi", "fin"], "Finnish", "Финский"),
    (&["no", "nor", "nob"], "Norwegian", "Норвежский"),
    (&["da", "dan"], "Danish", "Датский"),
    (&["hu", "hun"], "Hungarian", "Венгерский"),
    (&["ro", "rum", "ron"], "Romanian", "Румынский"),
    (&["bg", "bul"], "Bulgarian", "Болгарский"),
    (&["el", "gre", "ell"], "Greek", "Греческий"),
    (&["tr", "tur"], "Turkish", "Турецкий"),
    (&["ar", "ara"], "Arabic", "Арабский"),
    (&["he", "heb"], "Hebrew", "Иврит"),
    (&["hi", "hin"], "Hindi", "Хинди"),
    (&["ja", "jpn"], "Japanese", "Японский"),
    (&["zh", "chi", "zho"], "Chinese", "Китайский"),
    (&["ko", "kor"], "Korean", "Корейский"),
];

/// The language of `code` in the interface's language; an unknown code
/// as it is, in capitals.
pub fn language_in(lang: Lang, code: &str) -> String {
    let lower = code.to_ascii_lowercase();
    let base = lower.split(['-', '_']).next().unwrap_or_default();
    match LANGUAGES.iter().find(|(codes, _, _)| codes.contains(&base)) {
        Some((_, en, ru)) => match lang {
            Lang::En => (*en).into(),
            Lang::Ru => (*ru).into(),
        },
        None => code.to_uppercase(),
    }
}

/// The words of `text` in lower case.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, kind: &str, extra: Vec<(&str, Value)>) -> Value {
        let mut entries = vec![
            ("id".to_owned(), Value::Int(id)),
            ("type".to_owned(), Value::Str(kind.into())),
        ];
        entries.extend(extra.into_iter().map(|(k, v)| (k.to_owned(), v)));
        Value::Map(entries)
    }

    fn sample() -> Vec<Track> {
        from_value(&Value::List(vec![
            track(
                1,
                "video",
                vec![
                    ("codec", Value::Str("hevc".into())),
                    ("selected", Value::Flag(true)),
                ],
            ),
            track(
                1,
                "audio",
                vec![
                    ("lang", Value::Str("rus".into())),
                    ("codec", Value::Str("ac3".into())),
                    ("demux-channel-count", Value::Int(6)),
                    ("default", Value::Flag(true)),
                    ("selected", Value::Flag(true)),
                ],
            ),
            track(
                2,
                "audio",
                vec![
                    ("lang", Value::Str("eng".into())),
                    ("title", Value::Str("Commentary".into())),
                    ("codec", Value::Str("aac".into())),
                    ("demux-channel-count", Value::Int(2)),
                ],
            ),
            track(
                1,
                "sub",
                vec![
                    ("lang", Value::Str("ru".into())),
                    ("codec", Value::Str("subrip".into())),
                    ("external", Value::Flag(true)),
                ],
            ),
            track(
                2,
                "sub",
                vec![("codec", Value::Str("hdmv_pgs_subtitle".into()))],
            ),
            Value::Str("not a track".into()),
        ]))
    }

    #[test]
    fn reads_the_track_list() {
        let t = sample();
        assert_eq!(t.len(), 5);
        assert_eq!(of_kind(&t, Kind::Audio).len(), 2);
        assert_eq!(selected(&t, Kind::Audio).map(|t| t.id), Some(1));
        assert_eq!(selected(&t, Kind::Sub), None);
        assert_eq!(t[1].channels, Some(6));
        assert!(t[3].external);
    }

    #[test]
    fn names_and_details() {
        let mut t = sample();
        assert_eq!(t[1].name_in(Lang::Ru, 1), "Русский");
        assert_eq!(t[1].details_in(Lang::Ru), "AC3 · 5.1 · по умолчанию");
        assert_eq!(t[2].name_in(Lang::En, 2), "English — Commentary");
        assert_eq!(t[2].name_in(Lang::Ru, 2), "Английский — Commentary");
        assert_eq!(t[2].details_in(Lang::En), "AAC · 2.0");
        assert_eq!(t[3].details_in(Lang::En), "SRT · external");
        assert_eq!(t[4].name_in(Lang::Ru, 2), "Дорожка 2");
        assert_eq!(t[4].details_in(Lang::Ru), "PGS");
        assert_eq!(
            t[2].summary_in(Lang::En, &t),
            "2/2: English — Commentary (AAC · 2.0)"
        );
        // A title that says the language is the name alone.
        t[2].title = Some("English (original)".into());
        assert_eq!(t[2].name_in(Lang::Ru, 2), "English (original)");
        t[2].title = Some("Русский дубляж".into());
        t[2].lang = Some("rus".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "Русский дубляж");
        // The code is a word of its own, not a part of one.
        t[2].title = Some("Commentary".into());
        t[2].lang = Some("en".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "English — Commentary");
        t[2].title = Some("Mono".into());
        t[2].lang = Some("no".into());
        assert!(t[2].name_in(Lang::En, 2).ends_with("— Mono"));
        t[2].title = Some("rus, 2.0".into());
        t[2].lang = Some("rus".into());
        assert_eq!(t[2].name_in(Lang::En, 2), "rus, 2.0");
        assert_eq!(language_in(Lang::En, "pt-BR"), "Portuguese");
        assert_eq!(language_in(Lang::Ru, "xyz"), "XYZ");
    }

    #[test]
    fn the_most_alike_track() {
        let episode = |tracks: &[(&str, Option<&str>)]| -> Vec<Track> {
            tracks
                .iter()
                .enumerate()
                .map(|(i, (lang, title))| Track {
                    id: i as i64 + 1,
                    kind: Kind::Audio,
                    lang: Some((*lang).into()),
                    title: title.map(str::to_owned),
                    ..Track::default()
                })
                .collect()
        };
        let first = episode(&[
            ("eng", None),
            ("rus", Some("Дубляж")),
            ("rus", Some("Многоголосый")),
        ]);
        let like = Likeness::of(&first[2]);
        assert_eq!(Likeness::parse(&like.to_text()), Some(like.clone()));
        // The same voice-over in another order, its title in another case.
        let next = episode(&[
            ("rus", Some("Дубляж")),
            ("eng", None),
            ("ru", Some("МНОГОГОЛОСЫЙ")),
        ]);
        assert_eq!(most_like(&next, Kind::Audio, &like), Some(3));
        // No such title: the first of the same language.
        let other = episode(&[("eng", None), ("rus", Some("Дубляж"))]);
        assert_eq!(most_like(&other, Kind::Audio, &like), Some(2));
        // Nothing alike: mpv's choice stays.
        let none = episode(&[("eng", None), ("jpn", None)]);
        assert_eq!(most_like(&none, Kind::Audio, &like), None);
        assert_eq!(Likeness::parse("\t"), None);
    }

    #[test]
    fn cycling() {
        let mut t = sample();
        assert_eq!(next(&t, Kind::Audio), Some(Some(2)));
        assert_eq!(next(&t, Kind::Sub), Some(Some(1)));
        t[4].selected = true;
        assert_eq!(next(&t, Kind::Sub), Some(None));
        t[4].selected = false;
        t[3].selected = true;
        assert_eq!(next(&t, Kind::Sub), Some(Some(2)));
        // One audio track: nothing to switch to.
        let one: Vec<Track> = t
            .iter()
            .filter(|x| !(x.kind == Kind::Audio && x.id == 2))
            .cloned()
            .collect();
        assert_eq!(next(&one, Kind::Audio), None);
        assert_eq!(next(&[], Kind::Sub), None);
    }
}
