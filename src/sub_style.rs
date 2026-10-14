//! How subtitles look (the panel's Subtitles tab and the settings): the
//! font family, the size and boldness, the colour of the letters and of
//! their outline, the outline's thickness, the shadow, a box behind the
//! text in place of the outline, and the height on the picture. mpv's
//! `sub-font`, `sub-scale`, `sub-bold`, `sub-color`, `sub-outline-color`,
//! `sub-outline-size`, `sub-shadow-offset`, `sub-border-style` and
//! `sub-pos`. The size and the height apply to all subtitles; the rest
//! only to text ones (SRT and the like): ASS subtitles keep their own
//! styles. Kept between runs. [`Preset`]s set the look at once; the
//! user's own looks ([`Own`]) are kept by name, the whole style.

use std::ops::RangeInclusive;

/// The size, in percent of mpv's.
pub const SCALE_RANGE: RangeInclusive<f64> = 50.0..=300.0;
/// The outline's thickness, in mpv's units (scaled pixels at 720 lines).
pub const OUTLINE_RANGE: RangeInclusive<f64> = 0.0..=6.0;
/// The height: 100 at the bottom (mpv's `sub-pos`), less higher up.
pub const POS_RANGE: RangeInclusive<f64> = 50.0..=100.0;
/// The shadow's offset, in the outline's units.
pub const SHADOW_RANGE: RangeInclusive<f64> = 0.0..=4.0;
/// The opacity of the box behind the text, of 255.
const BOX_ALPHA: u8 = 0xc0;

#[derive(Clone, Debug, PartialEq)]
pub struct SubStyle {
    /// The font family, empty for mpv's (`sans-serif`).
    pub font: String,
    pub scale: f64,
    pub color: [u8; 3],
    pub outline_color: [u8; 3],
    /// The outline's thickness; with `boxed`, the box's margin round the
    /// text.
    pub outline: f64,
    pub pos: f64,
    pub bold: bool,
    pub shadow: f64,
    /// A box of the outline's colour, a little see-through, behind each
    /// line in place of the outline.
    pub boxed: bool,
}

impl Default for SubStyle {
    /// mpv's own.
    fn default() -> Self {
        Self {
            font: String::new(),
            scale: 100.0,
            color: [255, 255, 255],
            outline_color: [0, 0, 0],
            outline: 1.65,
            pos: 100.0,
            bold: false,
            shadow: 0.0,
            boxed: false,
        }
    }
}

/// A ready look: the colours, the outline, the shadow, boldness and the
/// box; the font, the size and the height stay as they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Standard,
    Thin,
    Bold,
    Soft,
    ShadowOnly,
    Cream,
    Mint,
    Boxed,
    LightBox,
}

impl Preset {
    pub const ALL: [Preset; 9] = [
        Preset::Standard,
        Preset::Thin,
        Preset::Bold,
        Preset::Soft,
        Preset::ShadowOnly,
        Preset::Cream,
        Preset::Mint,
        Preset::Boxed,
        Preset::LightBox,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Standard => tr!("Standard", "Обычные"),
            Preset::Thin => tr!("Thin outline", "Тонкая обводка"),
            Preset::Bold => tr!("Bold, thick outline", "Жирные, толстая обводка"),
            Preset::Soft => tr!("Soft, with a shadow", "Мягкие, с тенью"),
            Preset::ShadowOnly => tr!("Shadow only, no outline", "Только тень, без обводки"),
            Preset::Cream => tr!("Cream", "Кремовые"),
            Preset::Mint => tr!("Pale mint", "Бледно-мятные"),
            Preset::Boxed => tr!("On a dark box", "На тёмной подложке"),
            Preset::LightBox => tr!("Dark on a light box", "Тёмные на светлой подложке"),
        }
    }

    /// The look: letters, outline (or box) colour, thickness, shadow,
    /// bold, box.
    fn look(self) -> ([u8; 3], [u8; 3], f64, f64, bool, bool) {
        match self {
            Preset::Standard => ([255, 255, 255], [0, 0, 0], 1.65, 0.0, false, false),
            Preset::Thin => ([255, 255, 255], [0, 0, 0], 0.8, 0.0, false, false),
            Preset::Bold => ([255, 255, 255], [0, 0, 0], 3.0, 0.0, true, false),
            Preset::Soft => ([230, 230, 230], [26, 26, 26], 1.0, 1.5, false, false),
            Preset::ShadowOnly => ([255, 255, 255], [0, 0, 0], 0.0, 2.0, false, false),
            Preset::Cream => ([255, 243, 196], [32, 28, 20], 1.5, 1.0, false, false),
            Preset::Mint => ([214, 245, 228], [12, 32, 24], 1.5, 0.5, false, false),
            Preset::Boxed => ([255, 255, 255], [0, 0, 0], 2.5, 0.0, false, true),
            Preset::LightBox => ([20, 20, 20], [235, 235, 235], 2.5, 0.0, false, true),
        }
    }

    pub fn apply(self, style: &mut SubStyle) {
        let (color, outline_color, outline, shadow, bold, boxed) = self.look();
        style.color = color;
        style.outline_color = outline_color;
        style.outline = outline;
        style.shadow = shadow;
        style.bold = bold;
        style.boxed = boxed;
    }

    /// The preset whose look `style` has, if any.
    pub fn of(style: &SubStyle) -> Option<Preset> {
        Self::ALL.into_iter().find(|p| {
            let mut probe = style.clone();
            p.apply(&mut probe);
            probe == *style
        })
    }
}

/// A look the user saved under a name: the whole style, the font, the
/// size and the height too.
#[derive(Clone, Debug, PartialEq)]
pub struct Own {
    pub name: String,
    pub style: SubStyle,
}

/// The most own looks kept; saving one more drops the oldest.
pub const MAX_OWN: usize = 20;

impl Own {
    /// Keep `style` as `name` in `list`: in place of one of that name, else
    /// at the end.
    pub fn save(list: &mut Vec<Own>, name: &str, style: &SubStyle) {
        let name = clean(name);
        if name.is_empty() {
            return;
        }
        match list.iter_mut().find(|o| o.name == name) {
            Some(own) => own.style = style.clone(),
            None => {
                if list.len() >= MAX_OWN {
                    list.remove(0);
                }
                list.push(Own {
                    name,
                    style: style.clone(),
                });
            }
        }
    }

    /// The setting: a line each, the name and the style's fields after
    /// tabs.
    pub fn to_text(list: &[Own]) -> String {
        list.iter()
            .map(|o| {
                let s = &o.style;
                format!(
                    "{}\t{}\t{:.0}\t{}\t{}\t{:.2}\t{:.0}\t{}\t{:.2}\t{}",
                    o.name,
                    clean(&s.font),
                    s.scale,
                    hex(s.color),
                    hex(s.outline_color),
                    s.outline,
                    s.pos,
                    s.bold,
                    s.shadow,
                    s.boxed
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The looks of the setting; lines that are not understood are left
    /// out, numbers kept within their ranges.
    pub fn from_text(text: &str) -> Vec<Own> {
        let within = |v: &str, range: RangeInclusive<f64>| {
            v.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v.clamp(*range.start(), *range.end()))
        };
        text.lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                let [
                    name,
                    font,
                    scale,
                    color,
                    outline_color,
                    outline,
                    pos,
                    bold,
                    shadow,
                    boxed,
                ] = f.as_slice()
                else {
                    return None;
                };
                let name = clean(name);
                (!name.is_empty()).then_some(())?;
                Some(Own {
                    name,
                    style: SubStyle {
                        font: (*font).to_owned(),
                        scale: within(scale, SCALE_RANGE)?,
                        color: parse_hex(color)?,
                        outline_color: parse_hex(outline_color)?,
                        outline: within(outline, OUTLINE_RANGE)?,
                        pos: within(pos, POS_RANGE)?,
                        bold: bold.parse().ok()?,
                        shadow: within(shadow, SHADOW_RANGE)?,
                        boxed: boxed.parse().ok()?,
                    },
                })
            })
            .take(MAX_OWN)
            .collect()
    }
}

/// A name for a line of the setting: no tabs or line breaks, trimmed.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ").trim().to_owned()
}

impl SubStyle {
    /// mpv's properties and their values.
    pub fn options(&self) -> [(&'static str, String); 9] {
        let font = if self.font.is_empty() {
            "sans-serif"
        } else {
            &self.font
        };
        // The box is the outline's colour with an alpha, `#AARRGGBB`.
        let (outline, border) = if self.boxed {
            let [r, g, b] = self.outline_color;
            (
                format!("#{BOX_ALPHA:02X}{r:02X}{g:02X}{b:02X}"),
                "opaque-box",
            )
        } else {
            (hex(self.outline_color), "outline-and-shadow")
        };
        [
            ("sub-font", font.to_owned()),
            ("sub-scale", format!("{:.2}", self.scale / 100.0)),
            ("sub-color", hex(self.color)),
            ("sub-outline-color", outline),
            ("sub-outline-size", format!("{:.2}", self.outline)),
            ("sub-pos", format!("{:.0}", self.pos)),
            ("sub-bold", if self.bold { "yes" } else { "no" }.to_owned()),
            ("sub-shadow-offset", format!("{:.2}", self.shadow)),
            ("sub-border-style", border.to_owned()),
        ]
    }
}

/// `#RRGGBB`.
pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// A colour written by [`hex`].
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_round_trip() {
        assert_eq!(hex([255, 200, 0]), "#FFC800");
        assert_eq!(parse_hex("#FFC800"), Some([255, 200, 0]));
        assert_eq!(parse_hex("#ffc800"), Some([255, 200, 0]));
        assert_eq!(parse_hex("FFC800"), None);
        assert_eq!(parse_hex("#FFC8"), None);
        assert_eq!(parse_hex("#ЖЖЖ"), None);
    }

    #[test]
    fn options_for_mpv() {
        let style = SubStyle {
            font: "Arial".into(),
            scale: 150.0,
            color: [255, 255, 0],
            outline: 3.0,
            pos: 90.0,
            ..SubStyle::default()
        };
        assert_eq!(
            style.options(),
            [
                ("sub-font", "Arial".to_owned()),
                ("sub-scale", "1.50".to_owned()),
                ("sub-color", "#FFFF00".to_owned()),
                ("sub-outline-color", "#000000".to_owned()),
                ("sub-outline-size", "3.00".to_owned()),
                ("sub-pos", "90".to_owned()),
                ("sub-bold", "no".to_owned()),
                ("sub-shadow-offset", "0.00".to_owned()),
                ("sub-border-style", "outline-and-shadow".to_owned()),
            ]
        );
        assert_eq!(SubStyle::default().options()[0].1, "sans-serif");
        let boxed = SubStyle {
            boxed: true,
            ..SubStyle::default()
        };
        let options = boxed.options();
        assert_eq!(options[3].1, "#C0000000");
        assert_eq!(options[8].1, "opaque-box");
    }

    #[test]
    fn presets() {
        assert_eq!(Preset::of(&SubStyle::default()), Some(Preset::Standard));
        let mut style = SubStyle {
            font: "Arial".into(),
            scale: 140.0,
            ..SubStyle::default()
        };
        for p in Preset::ALL {
            p.apply(&mut style);
            assert_eq!(Preset::of(&style), Some(p));
            // The font and the size stay.
            assert_eq!(style.font, "Arial");
            assert_eq!(style.scale, 140.0);
        }
        style.color = [1, 2, 3];
        assert_eq!(Preset::of(&style), None);
    }

    #[test]
    fn own_looks() {
        let mut list = Vec::new();
        let style = SubStyle {
            font: "Segoe UI".into(),
            scale: 120.0,
            color: [250, 240, 200],
            bold: true,
            boxed: true,
            ..SubStyle::default()
        };
        Own::save(&mut list, "  Evening\tfilm ", &style);
        Own::save(&mut list, "Plain", &SubStyle::default());
        assert_eq!(list[0].name, "Evening film");
        // The same name: replaced in place.
        let bigger = SubStyle {
            scale: 150.0,
            ..style.clone()
        };
        Own::save(&mut list, "Evening film", &bigger);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].style.scale, 150.0);
        Own::save(&mut list, "   ", &style);
        assert_eq!(list.len(), 2);

        let text = Own::to_text(&list);
        assert_eq!(Own::from_text(&text), list);
        let broken =
            format!("{text}\nonly a name\nX\tF\t100\t#GGGGGG\t#000000\t1\t100\tfalse\t0\tfalse");
        assert_eq!(Own::from_text(&broken), list);

        for i in 0..MAX_OWN {
            Own::save(&mut list, &format!("Look {i}"), &style);
        }
        assert_eq!(list.len(), MAX_OWN);
        assert_eq!(list[0].name, "Look 0", "the oldest went");
    }
}
