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
