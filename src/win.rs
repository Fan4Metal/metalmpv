//! Win32 helpers shared by the apps of this family: a message box that
//! needs no window, the parent console for the command line modes, the
//! shell's "open", and an HTTPS GET through WinHTTP for the update check.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

/// A NUL-terminated UTF-16 string for the Win32 `W` functions.
pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Show `text` in a system message box with an error icon, waiting until
/// it is closed. It needs no window of the app, so it works from any thread
/// and while the app is failing (the panic hook). user32 is declared by
/// hand, to spare a `windows-sys` feature.
pub fn error_box(title: &str, text: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_ICONERROR: u32 = 0x10;
    const MB_SETFOREGROUND: u32 = 0x10000;
    let (text, title) = (wide(text), wide(title));
    unsafe {
        MessageBoxW(
            0,
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

/// The release build is a GUI-subsystem program with no console of its own,
/// so text printed by the command line modes (`--version`, `--help`,
/// `--bench`) would be lost when started from cmd or PowerShell. Attach to
/// the parent's console in that case; output already redirected (a pipe or
/// a file) is left untouched. A GUI program started by Python's
/// `subprocess` gets no handles either, hence the check.
pub fn attach_parent_console() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    if std::io::stdout().as_raw_handle().is_null() {
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Open `target` (a file, a folder or a URI such as `https://…` or
/// `ms-settings:…`) as Explorer would. eframe opens links only with its
/// `links` feature (the webbrowser crate), which is off, so About and the
/// update button go through this.
pub fn shell_open(target: impl AsRef<OsStr>) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = wide("open");
    let file = wide(target);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success.
    result as isize > 32
}

/// Open the folder of `file` in Explorer with the file selected.
pub fn show_in_folder(file: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    // Explorer parses its own command line: the path in quotes after the
    // comma, not escaped the way Rust would quote an argument.
    let result = std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", file.display()))
        .spawn();
    if let Err(e) = result {
        log::warn!("cannot start Explorer: {e}");
    }
}

/// HTTPS GET of `path` on `host` through WinHTTP, which brings the system's
/// TLS and proxy settings (no HTTP crate); `headers` are extra request
/// lines joined with `\r\n`. Returns the status code and the body, refused
/// past `limit` bytes. Blocks for up to `timeout_ms` per step, so call it
/// off the UI thread.
pub fn https_get(
    host: &str,
    path: &str,
    agent: &str,
    headers: &str,
    timeout_ms: i32,
    limit: usize,
) -> Result<(u32, Vec<u8>), String> {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Networking::WinHttp::*;

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }
    fn handle(h: *mut c_void) -> Result<Handle, String> {
        if h.is_null() {
            Err(winhttp_error())
        } else {
            Ok(Handle(h))
        }
    }
    fn check(ok: windows_sys::core::BOOL) -> Result<(), String> {
        if ok == 0 {
            Err(winhttp_error())
        } else {
            Ok(())
        }
    }

    let (agent, host, path, headers) = (wide(agent), wide(host), wide(path), wide(headers));
    let verb = wide("GET");
    let t = timeout_ms;
    // Declared in this order, the handles close request first.
    unsafe {
        let session = handle(WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        check(WinHttpSetTimeouts(session.0, t, t, t, t))?;
        let connection = handle(WinHttpConnect(
            session.0,
            host.as_ptr(),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ))?;
        let request = handle(WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        check(WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            (headers.len() - 1) as u32,
            null(),
            0,
            0,
            0,
        ))?;
        check(WinHttpReceiveResponse(request.0, null_mut()))?;
        let mut status = 0u32;
        let mut len = size_of::<u32>() as u32;
        check(WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&raw mut status).cast(),
            &mut len,
            null_mut(),
        ))?;
        let mut body = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            check(WinHttpReadData(
                request.0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut read,
            ))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > limit {
                return Err(format!("the answer is larger than {limit} bytes"));
            }
        }
        Ok((status, body))
    }
}

/// The last WinHTTP error in words; the system's message table lacks them.
fn winhttp_error() -> String {
    let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    match code {
        12002 => "the server did not answer in time".into(),
        12007 => "the server name could not be resolved".into(),
        12029 | 12030 => "the connection to the server failed".into(),
        12175 => "the secure connection failed".into(),
        _ => format!("WinHTTP error {code}"),
    }
}

/// The window's handle, if it is a Win32 window. `eframe::CreationContext`
/// and `eframe::Frame` both implement `HasWindowHandle`.
pub fn hwnd(window: &impl raw_window_handle::HasWindowHandle) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
        _ => None,
    }
}

/// Set a DWM window attribute of 32-bit `value`; whether it was taken.
fn dwm_set(hwnd: isize, attribute: u32, value: i32) -> bool {
    // dwmapi is declared by hand, to spare another `windows-sys` feature.
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *const core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    unsafe { DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast(), 4) >= 0 }
}

/// Round the corners of the frameless window as Windows 11 does its own
/// (`DWMWA_WINDOW_CORNER_PREFERENCE` = `DWMWCP_ROUND`); Windows 10 has no
/// such attribute and keeps them square. Also the dark frame colours for
/// the window's border on Windows 11.
pub fn round_corners(hwnd: isize) {
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_ROUND: i32 = 2;
    dwm_set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    dwm_set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
}

/// Hide the window from the screen while it keeps working (`DWMWA_CLOAK`),
/// or show it again.
pub fn cloak(hwnd: isize, on: bool) {
    const DWMWA_CLOAK: u32 = 13;
    dwm_set(hwnd, DWMWA_CLOAK, i32::from(on));
}

/// The window's restored rectangle in pixels (left, top, right, bottom),
/// also while it is maximized.
pub fn normal_rect(hwnd: isize) -> Option<[i32; 4]> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowPlacement, WINDOWPLACEMENT};
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd as _, &mut placement) } == 0 {
        return None;
    }
    let r = placement.rcNormalPosition;
    (r.right > r.left && r.bottom > r.top).then_some([r.left, r.top, r.right, r.bottom])
}

/// Maximize the window in one step, its restored rectangle being `normal`
/// (or `default` points centred where it is). winit shows a window created
/// maximized before it is painted (a white flash), and maximizing after
/// eframe shows it would take two steps; the caller cloaks the window
/// until its first maximized frame (qview's way).
pub fn show_maximized(hwnd: isize, normal: Option<[i32; 4]>, default: [f32; 2]) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, SW_SHOWMAXIMIZED, SetWindowPlacement, WINDOWPLACEMENT,
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForWindow(hwnd: isize) -> u32;
    }
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = size_of::<WINDOWPLACEMENT>() as u32;
    unsafe { GetWindowPlacement(hwnd as _, &mut placement) };
    let [left, top, right, bottom] = normal.unwrap_or_else(|| {
        let r = placement.rcNormalPosition;
        let scale = match unsafe { GetDpiForWindow(hwnd) } {
            0 => 1.0,
            dpi => dpi as f32 / 96.0,
        };
        let (w, h) = ((default[0] * scale) as i32, (default[1] * scale) as i32);
        let (x, y) = ((r.left + r.right - w) / 2, (r.top + r.bottom - h) / 2);
        [x, y, x + w, y + h]
    });
    placement.rcNormalPosition = RECT {
        left,
        top,
        right,
        bottom,
    };
    placement.showCmd = SW_SHOWMAXIMIZED as u32;
    placement.flags = 0;
    unsafe { SetWindowPlacement(hwnd as _, &placement) };
}

/// The double-click time of Windows in seconds (egui assumes 0.3).
pub fn double_click_time() -> f64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDoubleClickTime() -> u32;
    }
    unsafe { GetDoubleClickTime() as f64 / 1000.0 }
}
