//! MetalMPV: a fast video player for Windows on libmpv.

// The release build is a GUI program without a console; the debug build
// keeps it, so `eprintln!`, logs and panics are seen while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// First, so that `tr!` is in scope in the modules below.
#[macro_use]
mod i18n;

mod adjust;
mod app;
mod assoc;
mod audio;
mod chapters;
mod cover;
mod filetypes;
mod fonts;
mod format;
mod frameless;
mod icon;
mod info;
mod input;
mod instance;
mod mpv;
mod player;
mod playlist;
mod resume;
mod settings;
mod sizing;
mod taskbar;
mod tracks;
mod type_icon;
mod ui;
mod update;
mod video;
mod win;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

/// The display name, as people read it: the window title, About, the
/// panic message, the User-Agent of the update check. The identifier
/// (`metalmpv`) is the Cargo package, the exe and the repository.
pub const APP_NAME: &str = "MetalMPV";
/// eframe app id; also names the settings folder in `%APPDATA%` (unless the
/// copy is portable, see `settings::location`).
pub const APP_ID: &str = "MetalMPV";
/// Version from Cargo.toml, shown in About; a development version
/// ("0.1.0-dev") carries the commit it was built from (`build.rs`).
pub const VERSION: &str = env!("METALMPV_VERSION");
/// The window's size on the first start, in points.
pub const DEFAULT_SIZE: [f32; 2] = [960.0, 600.0];
/// The smallest the window may be made, in points.
pub const MIN_SIZE: [f32; 2] = [560.0, 340.0];

/// The window was saved maximized: `window_builder` creates it restored and
/// `App::new` maximizes it while cloaked (winit would show a window created
/// maximized before its first frame is painted).
pub static MAXIMIZE_WHEN_SHOWN: AtomicBool = AtomicBool::new(false);

/// When the process started, for the timings of `METALMPV_TRACE=1`.
pub fn started() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Milliseconds since the process started.
pub fn since_start() -> f64 {
    started().elapsed().as_secs_f64() * 1e3
}

fn main() -> eframe::Result {
    started();
    init_logging();
    install_panic_hook();

    // The command line modes and the files to play. `args_os`: `args`
    // panics on an argument that is not valid Unicode.
    let mut files = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--export-icon") => {
                // For tools/make_release.py: the installer's icon, then exit.
                win::attach_parent_console();
                let out = args.next().unwrap_or_else(|| "app.ico".into());
                if let Err(e) = std::fs::write(&out, icon::ico(&icon::ICO_SIZES)) {
                    let _ = writeln!(
                        std::io::stderr(),
                        "cannot write {}: {e}",
                        out.to_string_lossy()
                    );
                    std::process::exit(1);
                }
                return Ok(());
            }
            Some(flag @ ("--register" | "--unregister")) => {
                // For the installer: register or unregister the file types,
                // then exit; an error is shown (the installer runs it hidden).
                // The types are named in the language of Windows.
                i18n::set_lang(i18n::system_lang());
                let result = if flag == "--register" {
                    std::env::current_exe()
                        .map_err(|e| e.to_string())
                        .and_then(|exe| assoc::register(&exe))
                } else {
                    assoc::unregister()
                };
                if let Err(e) = &result {
                    log::error!("{flag}: {e}");
                    win::error_box(APP_NAME, e);
                    std::process::exit(1);
                }
                return Ok(());
            }
            Some("-V" | "--version") => {
                win::attach_parent_console();
                let _ = writeln!(std::io::stdout(), "{APP_NAME} {VERSION}");
                return Ok(());
            }
            Some("-h" | "--help") => {
                win::attach_parent_console();
                let _ = writeln!(
                    std::io::stdout(),
                    "{APP_NAME} {VERSION}\n\nmetalmpv [FILE...]\nmetalmpv --version | --help | --export-icon FILE.ico\nmetalmpv --register | --unregister   (the file types, for the installer)"
                );
                return Ok(());
            }
            _ => files.push(normalize(&arg.to_string_lossy())),
        }
    }

    // A copy already running from this exe takes the files and comes
    // forward; this one ends.
    if let Some((first, rest)) = files.split_first() {
        if instance::forward(Some(first)) {
            for file in rest {
                instance::forward(Some(file));
            }
            return Ok(());
        }
    } else if instance::forward(None) {
        return Ok(());
    }

    // The core starts (the DLL is loaded, mpv's threads and the audio
    // output come up) while the window is being made. Hardware decoding is
    // on until the settings say otherwise (`App::new`).
    let boot = std::thread::Builder::new()
        .name("mpv start".into())
        .spawn(|| {
            let started = Instant::now();
            let mpv = player::boot(true);
            log::debug!(
                "mpv started in {:.0} ms (at {:.0} ms)",
                started.elapsed().as_secs_f64() * 1e3,
                since_start()
            );
            mpv
        })
        .expect("spawn the mpv thread");

    let stored = settings::location();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size(DEFAULT_SIZE)
            .with_min_inner_size(MIN_SIZE)
            // The title row is the app's own (frameless.rs).
            .with_decorations(false)
            .with_resizable(true)
            .with_drag_and_drop(true)
            .with_icon(window_icon()),
        renderer: eframe::Renderer::Glow,
        // Centre only on the first run; later runs restore the saved window.
        centered: !stored.has_saved(),
        // A portable copy keeps its settings next to the exe; otherwise
        // eframe's own place in %APPDATA%. A file path, despite eframe's
        // doc calling it a folder.
        persistence_path: if stored.portable {
            stored.file.clone()
        } else {
            None
        },
        window_builder: Some(Box::new(|mut builder| {
            // A window closed in fullscreen comes back maximized. A restored
            // window always has `Some(false)` here, so `is_some` would not do.
            if builder.fullscreen == Some(true) {
                builder.fullscreen = Some(false);
                builder.maximized = Some(true);
            }
            if builder.maximized == Some(true) {
                builder.maximized = Some(false);
                MAXIMIZE_WHEN_SHOWN.store(true, Ordering::Relaxed);
            }
            builder
        })),
        ..Default::default()
    };
    eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, boot, files)))),
    )
}

/// Warnings and errors to stderr (seen in a debug build or with a console);
/// `METALMPV_TRACE=1` adds the app's debug lines (timings, mpv's info
/// messages), `RUST_LOG` works as usual.
fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"));
    if std::env::var_os("METALMPV_TRACE").is_some_and(|v| v != "0") {
        builder
            .filter_module("metalmpv", log::LevelFilter::Debug)
            .format_timestamp_millis();
    }
    builder.init();
}

/// Explorer passes a drive root as `"C:\"`, which Windows argument parsing
/// turns into `C:"`; the trailing quote is turned back into a backslash. A
/// bare `C:` means the root, not the current directory of C. Relative paths
/// become absolute (the running copy may have another current directory).
fn normalize(arg: &str) -> PathBuf {
    // An address is passed to mpv as it is.
    if playlist::is_url(Path::new(arg)) {
        return PathBuf::from(arg);
    }
    let arg = match arg.strip_suffix('"') {
        Some(stripped) => format!("{}\\", stripped.trim_end_matches('\\')),
        None => arg.to_string(),
    };
    let b = arg.as_bytes();
    let path = if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        PathBuf::from(format!("{arg}\\"))
    } else {
        PathBuf::from(arg)
    };
    std::path::absolute(&path).unwrap_or(path)
}

/// The icon of the window, from the embedded 64-pixel rendering.
pub fn window_icon() -> Arc<egui::IconData> {
    Arc::new(egui::IconData {
        rgba: embedded_icon(64).expect("embedded").to_vec(),
        width: 64,
        height: 64,
    })
}

/// The app icon rasterised at build time (`build.rs`) as straight RGBA,
/// `size` x `size` pixels, for the window icon and the start screen's at
/// 100–200 % (drawing 224 px takes ~30 ms, while the title row's 16 to
/// 32 px take under a millisecond); `None` for other sizes.
pub fn embedded_icon(size: u32) -> Option<&'static [u8]> {
    macro_rules! sizes {
        ($($n:literal),*) => {
            match size {
                $($n => Some(include_bytes!(concat!(env!("OUT_DIR"), "/app_icon_", $n, ".rgba")).as_slice()),)*
                _ => None,
            }
        };
    }
    sizes!(64, 80, 96, 112, 128, 140, 168, 196, 224)
}

/// The release build aborts on a panic and has no console, so the window
/// would vanish without a word: say what happened in a message box. The
/// default hook still prints it (seen in a debug build). A stack overflow
/// is not a panic and still ends the process silently.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let text = tr!(
            format!("{APP_NAME} stopped because of an internal error.\n\n{info}"),
            format!("{APP_NAME} остановлен из-за внутренней ошибки.\n\n{info}")
        );
        win::error_box(APP_NAME, &text);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_drive_argument() {
        assert_eq!(normalize("C:\""), PathBuf::from("C:\\"));
        assert_eq!(normalize("C:"), PathBuf::from("C:\\"));
        assert_eq!(
            normalize("D:\\Films\\a.mkv"),
            PathBuf::from("D:\\Films\\a.mkv")
        );
        assert!(normalize("a.mkv").is_absolute());
    }
}
