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

/// The work area (left, top, right, bottom in pixels: the screen without
/// the taskbar) of the monitor the window is mostly on.
pub fn work_area(hwnd: isize) -> Option<[i32; 4]> {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    let monitor = unsafe { MonitorFromWindow(hwnd as _, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let r = info.rcWork;
    Some([r.left, r.top, r.right, r.bottom])
}

/// Restore a maximized window; the window's rectangle and the size of its
/// client area, in pixels.
pub fn restored_rects(hwnd: isize) -> Option<([i32; 4], [i32; 2])> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, IsZoomed, SW_RESTORE, ShowWindow,
    };
    if unsafe { IsZoomed(hwnd as _) } != 0 {
        unsafe { ShowWindow(hwnd as _, SW_RESTORE) };
    }
    let mut window = RECT::default();
    let mut client = RECT::default();
    if unsafe { GetWindowRect(hwnd as _, &mut window) } == 0
        || unsafe { GetClientRect(hwnd as _, &mut client) } == 0
    {
        return None;
    }
    Some((
        [window.left, window.top, window.right, window.bottom],
        [client.right - client.left, client.bottom - client.top],
    ))
}

/// Move and size the window to `rect` (pixels).
pub fn set_rect(hwnd: isize, rect: [i32; 4]) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };
    unsafe {
        SetWindowPos(
            hwnd as _,
            std::ptr::null_mut(),
            rect[0],
            rect[1],
            rect[2] - rect[0],
            rect[3] - rect[1],
            SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        )
    };
}

/// Whether a window being moved is drawn to the screen's centre lines
/// ([`set_snap`]).
static SNAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// How near the screen's centre a moving window's centre is drawn to it,
/// in pixels at 96 dpi: the window holds on the line while the pointer
/// goes twice as far. 16 felt sticky.
const SNAP_REACH: i32 = 8;

/// What the window's messages asked for, for the app to take each frame
/// ([`take_hooked`]).
static HOOKED: std::sync::Mutex<Vec<Hooked>> = std::sync::Mutex::new(Vec::new());

/// Wakes the app when something is hooked ([`hook_window`]).
static WAKE: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

/// A key of a keyboard, a headset or a remote that egui does not see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

/// What the window's messages asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hooked {
    Media(MediaKey),
    /// The taskbar button exists (`TaskbarButtonCreated`): the thumbnail
    /// buttons can be added (`taskbar.rs`).
    TaskbarButton,
    /// A thumbnail button was clicked (its id).
    Thumb(u16),
}

/// `TaskbarButtonCreated`, registered by [`hook_window`].
static TASKBAR_CREATED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Subclass the window once: media keys (`WM_KEYDOWN` with a
/// `VK_MEDIA_*` key, which egui drops, and `WM_APPCOMMAND`, which headsets
/// and remotes send) are queued for [`take_hooked`] and `wake` is called;
/// while [`set_snap`] is on, a window being moved is drawn to the screen's
/// centre lines (`WM_MOVING`).
pub fn hook_window(hwnd: isize, wake: impl Fn() + Send + Sync + 'static) {
    use std::sync::Once;
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    static SUBCLASSED: Once = Once::new();
    SUBCLASSED.call_once(|| {
        use windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;
        let name = wide("TaskbarButtonCreated");
        let message = unsafe { RegisterWindowMessageW(name.as_ptr()) };
        TASKBAR_CREATED.store(message, std::sync::atomic::Ordering::Relaxed);
        let _ = WAKE.set(Box::new(wake));
        unsafe { SetWindowSubclass(hwnd as _, Some(window_proc), 1, 0) };
    });
}

/// While the user moves the window, its centre is drawn to the centre of
/// the monitor's work area along each axis it comes within [`SNAP_REACH`]
/// of (`sizing::snapped_to_centre`), as GOM Player does.
pub fn set_snap(on: bool) {
    SNAP.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// What the window's messages asked for since the last call.
pub fn take_hooked() -> Vec<Hooked> {
    HOOKED
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}

fn hook(what: Hooked) {
    if let Ok(mut q) = HOOKED.lock() {
        q.push(what);
    }
    if let Some(wake) = WAKE.get() {
        wake();
    }
}

/// The media key of a virtual key code.
fn media_vk(vk: u32) -> Option<MediaKey> {
    match vk {
        0xB0 => Some(MediaKey::Next),      // VK_MEDIA_NEXT_TRACK
        0xB1 => Some(MediaKey::Previous),  // VK_MEDIA_PREV_TRACK
        0xB2 => Some(MediaKey::Stop),      // VK_MEDIA_STOP
        0xB3 => Some(MediaKey::PlayPause), // VK_MEDIA_PLAY_PAUSE
        _ => None,
    }
}

/// The media key of a `WM_APPCOMMAND` command.
fn media_command(command: u32) -> Option<MediaKey> {
    match command {
        11 => Some(MediaKey::Next),      // APPCOMMAND_MEDIA_NEXTTRACK
        12 => Some(MediaKey::Previous),  // APPCOMMAND_MEDIA_PREVIOUSTRACK
        13 => Some(MediaKey::Stop),      // APPCOMMAND_MEDIA_STOP
        14 => Some(MediaKey::PlayPause), // APPCOMMAND_MEDIA_PLAY_PAUSE
        46 => Some(MediaKey::Play),      // APPCOMMAND_MEDIA_PLAY
        47 => Some(MediaKey::Pause),     // APPCOMMAND_MEDIA_PAUSE
        _ => None,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::UI::Shell::DefSubclassProc;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        WM_APPCOMMAND, WM_COMMAND, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_KEYDOWN, WM_MOVING,
    };
    // THBN_CLICKED in WM_COMMAND's high word.
    const THBN_CLICKED: usize = 0x1800;
    if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) {
        hook(Hooked::TaskbarButton);
    }
    match msg {
        WM_COMMAND if (wparam >> 16) & 0xFFFF == THBN_CLICKED => {
            hook(Hooked::Thumb((wparam & 0xFFFF) as u16));
            return 0;
        }
        WM_ENTERSIZEMOVE => unsafe { move_started(hwnd as isize) },
        WM_EXITSIZEMOVE => {
            if let Ok(mut start) = MOVE_START.lock() {
                *start = None;
            }
        }
        WM_MOVING if lparam != 0 && SNAP.load(Ordering::Relaxed) => {
            if unsafe { snap(hwnd as isize, lparam) } {
                return 1;
            }
        }
        WM_KEYDOWN => {
            if let Some(key) = media_vk(wparam as u32) {
                // Held keys repeat; one press is one command.
                if lparam & (1 << 30) == 0 {
                    hook(Hooked::Media(key));
                }
                return 0;
            }
        }
        WM_APPCOMMAND => {
            // GET_APPCOMMAND_LPARAM: the high word without the device bits.
            let command = ((lparam as u32) >> 16) & !0xF000;
            if let Some(key) = media_command(command) {
                hook(Hooked::Media(key));
                return 1;
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Where the window and the pointer were when the move began
/// (`WM_ENTERSIZEMOVE`): the rectangle the pointer alone would give.
static MOVE_START: std::sync::Mutex<Option<([i32; 4], [i32; 2])>> = std::sync::Mutex::new(None);

unsafe fn move_started(hwnd: isize) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
    let mut rect = RECT::default();
    let mut cursor = POINT::default();
    let start = (unsafe { GetWindowRect(hwnd as _, &mut rect) } != 0
        && unsafe { GetCursorPos(&mut cursor) } != 0)
        .then_some((
            [rect.left, rect.top, rect.right, rect.bottom],
            [cursor.x, cursor.y],
        ));
    if let Ok(mut s) = MOVE_START.lock() {
        *s = start;
    }
}

/// `WM_MOVING`: put the moved rectangle at `lparam` on the centre lines of
/// the work area it is within [`SNAP_REACH`] of; whether it could. The
/// rectangle snapped is where the pointer alone puts the window (the start
/// plus the pointer's way): Windows works the next rectangle out from the
/// one given back, so a snapped one would keep the window on the line for
/// as long as each step stays within the reach (slow moves got stuck).
unsafe fn snap(hwnd: isize, lparam: windows_sys::Win32::Foundation::LPARAM) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForWindow(hwnd: isize) -> u32;
    }
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let rect = unsafe { &mut *(lparam as *mut RECT) };
    let start = MOVE_START.lock().ok().and_then(|s| *s);
    let mut cursor = POINT::default();
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if unsafe { GetCursorPos(&mut cursor) } != 0 {
        match start {
            // A maximized window dragged is restored under the pointer:
            // its size changed, so the way is counted from here.
            Some((r, _)) if (r[2] - r[0], r[3] - r[1]) != (w, h) => {
                if let Ok(mut s) = MOVE_START.lock() {
                    *s = Some((
                        [rect.left, rect.top, rect.right, rect.bottom],
                        [cursor.x, cursor.y],
                    ));
                }
            }
            Some((r, c)) => {
                let (dx, dy) = (cursor.x - c[0], cursor.y - c[1]);
                *rect = RECT {
                    left: r[0] + dx,
                    top: r[1] + dy,
                    right: r[0] + dx + w,
                    bottom: r[1] + dy + h,
                };
            }
            None => {}
        }
    }
    let monitor = unsafe { MonitorFromRect(rect, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return false;
    }
    let dpi = match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        dpi => dpi as i32,
    };
    let w = info.rcWork;
    let [left, top, right, bottom] = crate::sizing::snapped_to_centre(
        [rect.left, rect.top, rect.right, rect.bottom],
        [w.left, w.top, w.right, w.bottom],
        SNAP_REACH * dpi / 96,
    );
    *rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    true
}

/// What playback needs Windows to keep on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Awake {
    /// Nothing: Windows may turn the display off and sleep.
    #[default]
    No,
    /// The computer (sound plays).
    System,
    /// The computer and the display (a picture plays).
    Display,
}

/// Keep the computer, and the display, from sleeping while `awake` says
/// so (`SetThreadExecutionState` with `ES_CONTINUOUS`: it lasts until the
/// next call from the same thread). mpv does this only for its own video
/// window, not for a render API client.
pub fn keep_awake(awake: Awake) {
    use windows_sys::Win32::System::Power::{
        ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
    };
    let flags = match awake {
        Awake::No => 0,
        Awake::System => ES_SYSTEM_REQUIRED,
        Awake::Display => ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED,
    };
    unsafe { SetThreadExecutionState(ES_CONTINUOUS | flags) };
}

/// The user's Pictures folder, wherever it was moved (OneDrive).
pub fn pictures_dir() -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Pictures, SHGetKnownFolderPath};
    // ole32 is declared by hand, to spare a `windows-sys` feature.
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(pv: *const core::ffi::c_void);
    }
    let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Pictures, 0, std::ptr::null_mut(), &mut raw) };
    let path = (hr >= 0 && !raw.is_null()).then(|| {
        let len = (0..).take_while(|&i| unsafe { *raw.add(i) } != 0).count();
        let wide = unsafe { std::slice::from_raw_parts(raw, len) };
        std::path::PathBuf::from(std::ffi::OsString::from_wide(wide))
    });
    unsafe { CoTaskMemFree(raw.cast()) };
    path
}

/// The double-click time of Windows in seconds (egui assumes 0.3).
pub fn double_click_time() -> f64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDoubleClickTime() -> u32;
    }
    unsafe { GetDoubleClickTime() as f64 / 1000.0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn media_keys() {
        use super::{MediaKey, media_command, media_vk};
        assert_eq!(media_vk(0xB3), Some(MediaKey::PlayPause));
        assert_eq!(media_vk(0xB0), Some(MediaKey::Next));
        assert_eq!(media_vk(0x41), None);
        assert_eq!(media_command(46), Some(MediaKey::Play));
        assert_eq!(media_command(8), None);
    }

    #[test]
    fn pictures_dir_is_found() {
        let dir = super::pictures_dir().expect("the Pictures folder");
        assert!(dir.is_absolute(), "{}", dir.display());
    }
}
