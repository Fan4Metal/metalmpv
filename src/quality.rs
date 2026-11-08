//! How the picture is drawn: the scaling (`settings.scaling`), the user's
//! own shaders (`settings.shaders`) and the smooth motion
//! (`settings.smooth_motion`). `App::follow_quality` gives mpv a
//! [`Quality`] whenever it differs from the one given last.

use std::path::{Path, PathBuf};

/// The scalers: mpv's defaults, cheaper ones for a weak graphics card,
/// or the costly polar ones (as mpv's `high-quality` profile, without its
/// debanding, which the panel's Video tab sets).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scaling {
    Fast,
    #[default]
    Standard,
    High,
}

impl Scaling {
    pub const ALL: [Scaling; 3] = [Scaling::Fast, Scaling::Standard, Scaling::High];

    pub fn name(self) -> &'static str {
        match self {
            Scaling::Fast => "fast",
            Scaling::Standard => "standard",
            Scaling::High => "high",
        }
    }

    pub fn from_name(name: &str) -> Option<Scaling> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Scaling::Fast => tr!("Fast", "Быстрое"),
            Scaling::Standard => tr!("Standard", "Стандартное"),
            Scaling::High => tr!("High quality", "Высокое качество"),
        }
    }

    /// mpv's options of the scalers (the standard ones are mpv's defaults,
    /// `defaults` checks them against the DLL).
    pub fn options(self) -> [(&'static str, &'static str); 6] {
        let (scale, cscale, dscale, better) = match self {
            Scaling::Fast => ("bilinear", "bilinear", "bilinear", "no"),
            Scaling::Standard => ("lanczos", "", "hermite", "yes"),
            Scaling::High => ("ewa_lanczossharp", "ewa_lanczossharp", "mitchell", "yes"),
        };
        [
            ("scale", scale),
            ("cscale", cscale),
            ("dscale", dscale),
            ("correct-downscaling", better),
            ("linear-downscaling", better),
            ("sigmoid-upscaling", better),
        ]
    }
}

/// The extensions of shader files (mpv's user shaders: `.glsl`, `.hook`).
pub const SHADER_EXTENSIONS: [&str; 2] = ["glsl", "hook"];

/// mpv's `glsl-shaders`: the files in order, joined by `;` (a path list on
/// Windows). A path with `;` would be cut in two: its short name instead,
/// as `clip.rs` does with subtitle files.
pub fn shaders_value(files: &[PathBuf]) -> String {
    files
        .iter()
        .map(|f| {
            let text = f.to_string_lossy();
            if text.contains(';') {
                crate::win::short_path(f)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| text.into_owned())
            } else {
                text.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The shader files of `paths` (others are left out), each once.
pub fn shader_files(paths: Vec<PathBuf>, have: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for p in paths {
        if is_shader(&p) && !have.contains(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn is_shader(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SHADER_EXTENSIONS.iter().any(|s| e.eq_ignore_ascii_case(s)))
}

/// What mpv is given.
#[derive(Clone, Debug, PartialEq)]
pub struct Quality {
    pub scaling: Scaling,
    /// The shaders in use (none while they are turned off).
    pub shaders: Vec<PathBuf>,
    /// Smooth motion, with the screen's refresh rate when it is known:
    /// mpv draws its frames to the screen's beat, resampling the sound a
    /// little (`video-sync=display-resample`), and blends frames between
    /// them (`interpolation`). The render API tells mpv nothing of the
    /// screen, so the rate is given (`display-fps-override`); without it
    /// mpv keeps to the sound.
    pub smooth: bool,
    pub refresh: Option<f64>,
}

impl Quality {
    /// mpv's options for this quality.
    pub fn options(&self) -> Vec<(&'static str, String)> {
        let mut options: Vec<(&'static str, String)> = self
            .scaling
            .options()
            .into_iter()
            .map(|(k, v)| (k, v.to_owned()))
            .collect();
        options.push(("glsl-shaders", shaders_value(&self.shaders)));
        let smooth = self.smooth && self.refresh.is_some();
        options.push((
            "video-sync",
            if smooth { "display-resample" } else { "audio" }.to_owned(),
        ));
        options.push((
            "interpolation",
            if smooth { "yes" } else { "no" }.to_owned(),
        ));
        options.push((
            "display-fps-override",
            format!("{:.6}", self.refresh.filter(|_| self.smooth).unwrap_or(0.0)),
        ));
        options
    }

    /// The options that differ from `before`'s (all of them without one).
    pub fn changes(&self, before: Option<&Quality>) -> Vec<(&'static str, String)> {
        let old = before.map(Quality::options).unwrap_or_default();
        self.options()
            .into_iter()
            .filter(|o| !old.contains(o))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpv::{Event, Mpv, Value, sys};

    #[test]
    fn names() {
        for s in Scaling::ALL {
            assert_eq!(Scaling::from_name(s.name()), Some(s));
        }
        assert_eq!(Scaling::from_name("best"), None);
    }

    #[test]
    fn shader_paths() {
        let files = vec![
            PathBuf::from(r"C:\Shaders\FSR.glsl"),
            PathBuf::from(r"D:\a b\x.hook"),
        ];
        assert_eq!(shaders_value(&files), r"C:\Shaders\FSR.glsl;D:\a b\x.hook");
        assert_eq!(shaders_value(&[]), "");
        let have = vec![PathBuf::from(r"C:\Shaders\FSR.glsl")];
        let picked = shader_files(
            vec![
                PathBuf::from(r"C:\Shaders\FSR.glsl"),
                PathBuf::from(r"C:\Shaders\Anime4K.GLSL"),
                PathBuf::from(r"C:\Shaders\readme.txt"),
                PathBuf::from(r"C:\Shaders\Anime4K.GLSL"),
            ],
            &have,
        );
        assert_eq!(picked, vec![PathBuf::from(r"C:\Shaders\Anime4K.GLSL")]);
    }

    #[test]
    fn smooth_needs_the_rate() {
        let mut q = Quality {
            scaling: Scaling::Standard,
            shaders: Vec::new(),
            smooth: true,
            refresh: None,
        };
        let get = |q: &Quality, k: &str| {
            q.options()
                .into_iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v)
                .unwrap()
        };
        assert_eq!(get(&q, "video-sync"), "audio");
        assert_eq!(get(&q, "interpolation"), "no");
        q.refresh = Some(60000.0 / 1001.0);
        assert_eq!(get(&q, "video-sync"), "display-resample");
        assert_eq!(get(&q, "interpolation"), "yes");
        assert_eq!(get(&q, "display-fps-override"), "59.940060");
        let before = q.clone();
        q.scaling = Scaling::High;
        let changes = q.changes(Some(&before));
        assert_eq!(changes.len(), 3, "{changes:?}");
        assert!(
            changes
                .iter()
                .all(|(k, _)| *k == "scale" || *k == "cscale" || *k == "dscale")
        );
        assert_eq!(q.changes(Some(&q)), Vec::new());
    }

    /// The standard scaling is mpv's own, and every level's options and the
    /// smooth motion's are taken.
    #[test]
    #[ignore = "needs vendor/libmpv"]
    fn options_are_mpvs() {
        let api = sys::api().unwrap();
        let mut mpv = Mpv::new(api, &[("vo", "null"), ("ao", "null"), ("idle", "yes")]).unwrap();
        let read = |mpv: &mut Mpv, name: &str| -> Value {
            mpv.get(1, name, sys::MPV_FORMAT_STRING);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                match mpv.next_event() {
                    Some(Event::Property { id: 1, value }) => return value,
                    Some(_) => {}
                    None => std::thread::sleep(std::time::Duration::from_millis(2)),
                }
            }
            panic!("no answer for {name}");
        };
        for (name, value) in Scaling::Standard.options() {
            let default = read(&mut mpv, &format!("option-info/{name}/default-value"));
            assert_eq!(default, Value::Str(value.to_owned()), "{name}");
        }
        for scaling in Scaling::ALL {
            let q = Quality {
                scaling,
                shaders: Vec::new(),
                smooth: true,
                refresh: Some(144.0),
            };
            for (name, value) in q.options() {
                mpv.set(name, &Value::Str(value.clone()));
                let now = read(&mut mpv, name);
                assert_eq!(now, Value::Str(value.clone()), "{name}");
            }
        }
    }

    /// The refresh rate of the primary screen.
    #[test]
    #[ignore = "reads the screen's settings"]
    fn refresh_rate() {
        let desktop = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        let rate = crate::win::refresh_rate(desktop as isize);
        println!("refresh rate: {rate:?}");
        assert!(rate.is_some_and(|r| r > 20.0));
    }
}
