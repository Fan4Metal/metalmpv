//! The sound's settings and what mpv makes of them: the output device
//! (`audio-device`, from `audio-device-list`), the channels
//! (`audio-channels`), the volume boost (`volume-max`), the normalization
//! (an `af` filter) and the delay against the picture (`audio-delay`).

use crate::mpv::Value;

/// The step of the audio delay keys, in seconds.
pub const DELAY_STEP: f64 = 0.1;
/// The volume's ceilings to choose from, in percent: 100 is no boost.
pub const BOOSTS: [f64; 4] = [100.0, 130.0, 150.0, 200.0];
/// The highest volume there may be, in percent.
pub const MAX_VOLUME: f64 = 200.0;
/// The normalization: FFmpeg's dynamic normalizer, which raises quiet
/// passages and lowers loud ones as the film plays.
pub const NORMALIZE_FILTER: &str = "lavfi=[dynaudnorm=f=250:g=31:p=0.5:m=5:r=0.9:b=1]";

/// The equalizer's bands, their centres in Hz.
pub const EQ_BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
/// The most a band is raised or lowered, in dB.
pub const EQ_RANGE: i64 = 12;

/// The ten-band equalizer: a peaking filter an octave wide at each band
/// with a gain (FFmpeg's `equalizer`); kept in the settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Equalizer {
    pub on: bool,
    /// In dB, by [`EQ_BANDS`].
    pub gains: [i64; 10],
}

/// A preset of the equalizer: its gains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Flat,
    Rock,
    Pop,
    Jazz,
    Classical,
    Bass,
    Voice,
    Treble,
}

impl Preset {
    pub const ALL: [Preset; 8] = [
        Preset::Flat,
        Preset::Rock,
        Preset::Pop,
        Preset::Jazz,
        Preset::Classical,
        Preset::Bass,
        Preset::Voice,
        Preset::Treble,
    ];

    pub fn gains(self) -> [i64; 10] {
        match self {
            Preset::Flat => [0; 10],
            Preset::Rock => [5, 4, 3, 1, -1, -1, 1, 3, 4, 5],
            Preset::Pop => [-1, 1, 3, 4, 3, 0, -1, -1, 0, 1],
            Preset::Jazz => [3, 2, 1, 2, -1, -1, 0, 1, 2, 3],
            Preset::Classical => [4, 3, 2, 1, -1, -1, 0, 2, 3, 4],
            Preset::Bass => [7, 6, 5, 3, 1, 0, 0, 0, 0, 0],
            Preset::Voice => [-3, -2, -1, 1, 4, 4, 3, 1, 0, -2],
            Preset::Treble => [0, 0, 0, 0, 0, 1, 2, 4, 5, 6],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Flat => tr!("Flat", "Ровно"),
            Preset::Rock => tr!("Rock", "Рок"),
            Preset::Pop => tr!("Pop", "Поп"),
            Preset::Jazz => tr!("Jazz", "Джаз"),
            Preset::Classical => tr!("Classical", "Классика"),
            Preset::Bass => tr!("Bass", "Басы"),
            Preset::Voice => tr!("Voice", "Голос"),
            Preset::Treble => tr!("Treble", "Высокие"),
        }
    }

    /// The preset `gains` are, if any.
    pub fn of(gains: &[i64; 10]) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.gains() == *gains)
    }
}

/// Gains the user saved under a name.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnEq {
    pub name: String,
    pub gains: [i64; 10],
}

/// The most own gains kept; saving one more drops the oldest.
pub const MAX_OWN_EQ: usize = 20;

impl OwnEq {
    /// Keep `gains` as `name` in `list`: in place of one of that name,
    /// else at the end.
    pub fn save(list: &mut Vec<OwnEq>, name: &str, gains: [i64; 10]) {
        let name = crate::sub_style::clean(name);
        if name.is_empty() {
            return;
        }
        match list.iter_mut().find(|o| o.name == name) {
            Some(own) => own.gains = gains,
            None => {
                if list.len() >= MAX_OWN_EQ {
                    list.remove(0);
                }
                list.push(OwnEq { name, gains });
            }
        }
    }

    /// The setting: a line each, `name<TAB>g,g,…`.
    pub fn to_text(list: &[OwnEq]) -> String {
        list.iter()
            .map(|o| {
                let gains: Vec<String> = o.gains.iter().map(i64::to_string).collect();
                format!("{}\t{}", o.name, gains.join(","))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The gains of the setting; lines not understood are left out.
    pub fn from_text(text: &str) -> Vec<OwnEq> {
        text.lines()
            .filter_map(|line| {
                let (name, gains) = line.split_once('\t')?;
                let name = crate::sub_style::clean(name);
                let gains: Vec<i64> = gains
                    .split(',')
                    .map(|g| {
                        g.trim()
                            .parse::<i64>()
                            .ok()
                            .map(|g| g.clamp(-EQ_RANGE, EQ_RANGE))
                    })
                    .collect::<Option<_>>()?;
                let gains = <[i64; 10]>::try_from(gains).ok()?;
                (!name.is_empty()).then_some(OwnEq { name, gains })
            })
            .take(MAX_OWN_EQ)
            .collect()
    }
}

/// The band's label: `31`, `1k`, `16k`.
pub fn band_label(hz: u32) -> String {
    if hz >= 1000 {
        format!("{}k", hz / 1000)
    } else {
        hz.to_string()
    }
}

/// How much the centre channel is raised to make speech clearer (+4 dB).
const CENTRE_GAIN: f64 = 1.6;

/// The filter that makes speech clearer in a track of `channels`: stereo
/// through FFmpeg's `dialoguenhance` (it finds the voice common to both
/// sides and puts it in a centre, which the device's mix brings back),
/// five channels and more by raising the centre (third in FFmpeg's order:
/// FL, FR, FC, …). `None` for mono, where the voice is all there is, and
/// for layouts whose third channel is not the centre (2.1).
pub fn voice_filter(channels: i64) -> Option<String> {
    match channels {
        2 => Some("lavfi=[dialoguenhance]".to_owned()),
        n if n >= 5 => {
            let each: Vec<String> = (0..n)
                .map(|i| {
                    if i == 2 {
                        format!("val(2)*{CENTRE_GAIN}")
                    } else {
                        format!("val({i})")
                    }
                })
                .collect();
            Some(format!("lavfi=[aeval={}:c=same]", each.join("|")))
        }
        _ => None,
    }
}

/// mpv's `af`: the voice's filter when `voice` gives the channels of the
/// track playing ([`voice_filter`]), the equalizer's bands that are not
/// flat, then the normalization (which evens out what the others raised).
pub fn filters(normalize: bool, eq: &Equalizer, voice: Option<i64>) -> String {
    let mut af: Vec<String> = Vec::new();
    if let Some(filter) = voice.and_then(voice_filter) {
        af.push(filter);
    }
    if eq.on {
        for (hz, gain) in EQ_BANDS.iter().zip(eq.gains) {
            if gain != 0 {
                let gain = gain.clamp(-EQ_RANGE, EQ_RANGE);
                af.push(format!("lavfi=[equalizer=f={hz}:t=o:w=1:g={gain}]"));
            }
        }
    }
    if normalize {
        af.push(NORMALIZE_FILTER.to_owned());
    }
    af.join(",")
}

/// The channels sent to the device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channels {
    /// As the device is set up in Windows; more channels are mixed down.
    #[default]
    Auto,
    Mono,
    Stereo,
    Surround51,
    Surround71,
}

impl Channels {
    pub const ALL: [Channels; 5] = [
        Channels::Auto,
        Channels::Mono,
        Channels::Stereo,
        Channels::Surround51,
        Channels::Surround71,
    ];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Channels::Auto => "auto",
            Channels::Mono => "mono",
            Channels::Stereo => "stereo",
            Channels::Surround51 => "5.1",
            Channels::Surround71 => "7.1",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }

    /// mpv's `audio-channels`.
    pub fn option(self) -> &'static str {
        match self {
            Channels::Auto => "auto-safe",
            other => other.name(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channels::Auto => tr!("Auto", "Авто"),
            Channels::Mono => tr!("Mono", "Моно"),
            Channels::Stereo => tr!("Stereo", "Стерео"),
            Channels::Surround51 => "5.1",
            Channels::Surround71 => "7.1",
        }
    }
}

/// Compressed sound sent as it is to a receiver, which decodes it
/// (`audio-spdif`). WASAPI takes it in exclusive mode only, which mpv
/// asks for itself; a format the device refuses is decoded as usual.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Passthrough {
    #[default]
    Off,
    /// What an optical or coaxial cable carries.
    Spdif,
    /// HDMI carries the lossless formats too.
    Hdmi,
}

impl Passthrough {
    pub const ALL: [Passthrough; 3] = [Passthrough::Off, Passthrough::Spdif, Passthrough::Hdmi];

    /// The stored name.
    pub fn name(self) -> &'static str {
        match self {
            Passthrough::Off => "off",
            Passthrough::Spdif => "spdif",
            Passthrough::Hdmi => "hdmi",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// mpv's `audio-spdif`: the codecs passed through.
    pub fn option(self) -> &'static str {
        match self {
            Passthrough::Off => "",
            Passthrough::Spdif => "ac3,dts",
            Passthrough::Hdmi => "ac3,eac3,dts,dts-hd,truehd",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Passthrough::Off => tr!("Off", "Выкл."),
            Passthrough::Spdif => tr!("S/PDIF: AC3, DTS", "S/PDIF: AC3, DTS"),
            Passthrough::Hdmi => tr!(
                "HDMI: AC3, E-AC3, DTS, DTS-HD, TrueHD",
                "HDMI: AC3, E-AC3, DTS, DTS-HD, TrueHD"
            ),
        }
    }
}

/// The label of a volume ceiling.
pub fn boost_label(max: f64) -> String {
    if max <= 100.0 {
        tr!("None (up to 100%)", "Нет (до 100%)").into()
    } else {
        tr!(format!("Up to {max:.0}%"), format!("До {max:.0}%"))
    }
}

/// An output device as mpv names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// mpv's `audio-device` value ("wasapi/{…}").
    pub name: String,
    /// What Windows calls it ("Speakers (Realtek(R) Audio)").
    pub description: String,
}

/// The WASAPI devices of mpv's `audio-device-list`: Windows' own. The
/// list also has `auto` and other drivers' (`openal`), which choosing
/// would switch the driver.
pub fn devices_from_value(list: &Value) -> Vec<Device> {
    list.list()
        .iter()
        .filter_map(|d| {
            let name = d.get("name")?.string()?;
            if !name.starts_with("wasapi/") {
                return None;
            }
            let description = d
                .get("description")
                .and_then(Value::string)
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            Some(Device {
                name: name.to_owned(),
                description: description.to_owned(),
            })
        })
        .collect()
}

/// The device to give mpv: the chosen one while Windows has it, else
/// Windows' default (empty). mpv does not fall back from a device that is
/// gone (ao.c's `forced_dev`): the sound would be lost. `None` while the
/// devices are not known yet.
pub fn device_to_use<'a>(chosen: &'a str, devices: &[Device]) -> Option<&'a str> {
    if devices.is_empty() {
        return None;
    }
    Some(if devices.iter().any(|d| d.name == chosen) {
        chosen
    } else {
        ""
    })
}

/// What the player is started with (the device follows once mpv lists
/// the devices, [`device_to_use`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub volume: f64,
    pub mute: bool,
    /// The volume's ceiling, 100 to [`MAX_VOLUME`].
    pub volume_max: f64,
    pub channels: Channels,
    pub normalize: bool,
    pub equalizer: Equalizer,
    pub passthrough: Passthrough,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_names_round_trip() {
        for c in Channels::ALL {
            assert_eq!(Channels::from_name(c.name()), Some(c));
        }
        assert_eq!(Channels::Auto.option(), "auto-safe");
        assert_eq!(Channels::Surround51.option(), "5.1");
        assert_eq!(Channels::from_name("9.1"), None);
        for p in Passthrough::ALL {
            assert_eq!(Passthrough::from_name(p.name()), Some(p));
        }
        assert_eq!(Passthrough::Off.option(), "");
        assert_eq!(Passthrough::Spdif.option(), "ac3,dts");
    }

    #[test]
    fn equalizer_filters() {
        let mut eq = Equalizer::default();
        assert_eq!(filters(false, &eq, None), "");
        eq.gains = Preset::Bass.gains();
        assert_eq!(filters(false, &eq, None), "", "off: no bands");
        eq.on = true;
        eq.gains = [0, 0, 0, 0, 0, 0, 0, 0, 3, -20];
        assert_eq!(
            filters(true, &eq, None),
            format!(
                "lavfi=[equalizer=f=8000:t=o:w=1:g=3],lavfi=[equalizer=f=16000:t=o:w=1:g=-12],{NORMALIZE_FILTER}"
            )
        );
        assert_eq!(Preset::of(&Preset::Rock.gains()), Some(Preset::Rock));
        assert_eq!(band_label(16000), "16k");
        assert_eq!(band_label(62), "62");
    }

    #[test]
    fn own_gains() {
        let mut list = Vec::new();
        OwnEq::save(&mut list, " Night ", [1, 2, 3, 4, 5, 6, 7, 8, 9, -12]);
        OwnEq::save(&mut list, "Night", [0; 10]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].gains, [0; 10]);
        OwnEq::save(&mut list, "Loud", [12; 10]);
        let text = OwnEq::to_text(&list);
        assert_eq!(
            text,
            "Night\t0,0,0,0,0,0,0,0,0,0\nLoud\t12,12,12,12,12,12,12,12,12,12"
        );
        assert_eq!(OwnEq::from_text(&text), list);
        assert_eq!(
            OwnEq::from_text("Odd\t1,2,3\nBig\t99,0,0,0,0,0,0,0,0,0")[0].gains[0],
            12
        );
    }

    #[test]
    fn voice_filters() {
        assert_eq!(voice_filter(1), None);
        assert_eq!(voice_filter(3), None);
        assert_eq!(voice_filter(2).as_deref(), Some("lavfi=[dialoguenhance]"));
        assert_eq!(
            voice_filter(6).as_deref(),
            Some("lavfi=[aeval=val(0)|val(1)|val(2)*1.6|val(3)|val(4)|val(5):c=same]")
        );
        let eq = Equalizer::default();
        assert_eq!(
            filters(true, &eq, Some(2)),
            format!("lavfi=[dialoguenhance],{NORMALIZE_FILTER}")
        );
    }

    #[test]
    fn a_missing_device_falls_back() {
        let devices = [Device {
            name: "wasapi/{1}".into(),
            description: "Speakers".into(),
        }];
        assert_eq!(device_to_use("wasapi/{1}", &devices), Some("wasapi/{1}"));
        assert_eq!(device_to_use("wasapi/{9}", &devices), Some(""));
        assert_eq!(device_to_use("", &devices), Some(""));
        assert_eq!(device_to_use("wasapi/{1}", &[]), None);
    }

    #[test]
    fn only_wasapi_devices() {
        let device = |name: &str, description: &str| {
            Value::Map(vec![
                ("name".into(), Value::Str(name.into())),
                ("description".into(), Value::Str(description.into())),
            ])
        };
        let list = Value::List(vec![
            device("auto", "Autoselect device"),
            device("openal", "Default (openal)"),
            device("wasapi/{1}", "Speakers (Realtek(R) Audio)"),
            device("wasapi/{2}", ""),
        ]);
        assert_eq!(
            devices_from_value(&list),
            vec![
                Device {
                    name: "wasapi/{1}".into(),
                    description: "Speakers (Realtek(R) Audio)".into()
                },
                Device {
                    name: "wasapi/{2}".into(),
                    description: "wasapi/{2}".into()
                },
            ]
        );
    }
}
