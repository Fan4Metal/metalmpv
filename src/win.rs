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
    https_request(host, path, agent, headers, timeout_ms, limit, true)
        .map(|(status, body, _)| (status, body))
}

/// As [`https_get`], but a redirect is not followed: its status comes
/// back with the `Location` header, for the caller to check where it
/// leads (`logos.rs`).
pub fn https_get_once(
    host: &str,
    path: &str,
    agent: &str,
    headers: &str,
    timeout_ms: i32,
    limit: usize,
) -> Result<(u32, Vec<u8>, Option<String>), String> {
    https_request(host, path, agent, headers, timeout_ms, limit, false)
}

fn https_request(
    host: &str,
    path: &str,
    agent: &str,
    headers: &str,
    timeout_ms: i32,
    limit: usize,
    follow: bool,
) -> Result<(u32, Vec<u8>, Option<String>), String> {
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
    // Each step has `timeout_ms`; the whole answer at most twice that, else
    // a server sending a byte now and then held a thread for good.
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(2 * u64::try_from(timeout_ms).unwrap_or(10_000));
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
        if !follow {
            let mut policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
            check(WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                (&raw mut policy).cast(),
                size_of::<u32>() as u32,
            ))?;
        }
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
        let location = (!follow && (300..400).contains(&status))
            .then(|| {
                // Asked for its length first: a CDN's signed address can be
                // longer than any buffer guessed.
                let mut len = 0u32;
                WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_LOCATION,
                    null(),
                    null_mut(),
                    &mut len,
                    null_mut(),
                );
                let mut buf = vec![0u16; len as usize / 2 + 1];
                let mut len = (buf.len() * 2) as u32;
                let ok = WinHttpQueryHeaders(
                    request.0,
                    WINHTTP_QUERY_LOCATION,
                    null(),
                    buf.as_mut_ptr().cast(),
                    &mut len,
                    null_mut(),
                );
                (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize / 2]))
            })
            .flatten();
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
            if std::time::Instant::now() > deadline {
                return Err("the answer took too long".into());
            }
        }
        Ok((status, body, location))
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

/// The window's rectangle on the screen, in pixels.
pub fn window_rect(hwnd: isize) -> Option<[i32; 4]> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    (unsafe { GetWindowRect(hwnd as _, &mut r) } != 0).then_some([r.left, r.top, r.right, r.bottom])
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

/// The way a media key came. One press may come more than one way: with
/// the window in front, Windows sends `WM_APPCOMMAND` and then the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// `WM_KEYDOWN` with a `VK_MEDIA_*` key.
    Key,
    /// `WM_APPCOMMAND`.
    Command,
    /// Windows' media controls (`smtc.rs`).
    Controls,
}

/// What the window's messages asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hooked {
    /// A media key and the way it came.
    Media(MediaKey, Via),
    /// The taskbar button exists (`TaskbarButtonCreated`): the thumbnail
    /// buttons can be added (`taskbar.rs`).
    TaskbarButton,
    /// A thumbnail button was clicked (its id).
    Thumb(u16),
    /// Windows is ending the session (`WM_QUERYENDSESSION`): winit closes
    /// nothing, so the settings are saved now.
    EndSession,
    /// The Restart Manager asks the app to close (`WM_QUERYENDSESSION`
    /// with `ENDSESSION_CLOSEAPP`): an installer is to replace its files.
    Close,
    /// The computer is going to sleep (`WM_POWERBROADCAST` with
    /// `PBT_APMSUSPEND`): saved now too, as it may not wake.
    Suspend,
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
        // A copy running elevated still hears Explorer (UIPI): the taskbar
        // button made and the thumbnail buttons' clicks.
        #[link(name = "user32")]
        unsafe extern "system" {
            fn ChangeWindowMessageFilterEx(
                hwnd: windows_sys::Win32::Foundation::HWND,
                message: u32,
                action: u32,
                filter: *mut core::ffi::c_void,
            ) -> i32;
        }
        const MSGFLT_ALLOW: u32 = 1;
        for message in [
            message,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
        ] {
            unsafe {
                ChangeWindowMessageFilterEx(hwnd as _, message, MSGFLT_ALLOW, std::ptr::null_mut())
            };
        }
        unsafe { SetWindowSubclass(hwnd as _, Some(window_proc), 1, 0) };
    });
}

/// While the user moves the window, its centre is drawn to the centre of
/// the monitor's work area along each axis it comes within [`SNAP_REACH`]
/// of (`sizing::snapped_to_centre`), as GOM Player does.
pub fn set_snap(on: bool) {
    SNAP.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The windows take the media keys ([`set_media_keys`]).
static MEDIA_KEYS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Whether the player's windows take the media keys; off, the keys and
/// `WM_APPCOMMAND` go on to Windows (and through it to other programs).
pub fn set_media_keys(on: bool) {
    MEDIA_KEYS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The window is in fullscreen ([`set_fullscreen`]).
static FULLSCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The window enters or leaves fullscreen. A window maximized before keeps
/// `WS_MAXIMIZE` there, and winit's `WM_NCCALCSIZE` limits a maximized
/// window's client area to the work area: a strip as tall as the taskbar
/// was left black at the bottom (now and then: winit's move to the monitor
/// is asynchronous). Meanwhile the client area is the whole monitor
/// ([`fullscreen_client`]).
pub fn set_fullscreen(on: bool) {
    FULLSCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// `WM_NCCALCSIZE` in fullscreen: the client area is the whole window
/// (no frame), within the monitor it is on (a maximized window's
/// rectangle goes past the monitor's edges by its frame).
unsafe fn fullscreen_client(lparam: windows_sys::Win32::Foundation::LPARAM) {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::NCCALCSIZE_PARAMS;
    let params = unsafe { &mut *(lparam as *mut NCCALCSIZE_PARAMS) };
    let monitor = unsafe { MonitorFromRect(&params.rgrc[0], MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        let (r, m) = (&mut params.rgrc[0], info.rcMonitor);
        r.left = r.left.max(m.left);
        r.top = r.top.max(m.top);
        r.right = r.right.min(m.right).max(r.left);
        r.bottom = r.bottom.min(m.bottom).max(r.top);
    }
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

/// A media key pressed elsewhere (Windows' media controls, `smtc.rs`),
/// queued as one of the window's.
pub fn hook_media(key: MediaKey) {
    hook(Hooked::Media(key, Via::Controls));
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
        ENDSESSION_CLOSEAPP, PBT_APMSUSPEND, WM_COMMAND, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE,
        WM_MOVING, WM_NCCALCSIZE, WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_WINDOWPOSCHANGED,
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
        // Answered as DefWindowProc does (yes); the app saves before the
        // process is ended. The Restart Manager (an installer replacing
        // the exe) asks the app to close and waits for it.
        WM_QUERYENDSESSION if lparam as u32 & ENDSESSION_CLOSEAPP != 0 => hook(Hooked::Close),
        WM_QUERYENDSESSION => hook(Hooked::EndSession),
        WM_POWERBROADCAST if wparam == PBT_APMSUSPEND as usize => hook(Hooked::Suspend),
        // Not winit's: the whole rectangle, maximized or not.
        WM_NCCALCSIZE if wparam != 0 && lparam != 0 && FULLSCREEN.load(Ordering::Relaxed) => {
            unsafe { fullscreen_client(lparam) };
            return 0;
        }
        WM_ENTERSIZEMOVE => unsafe { move_started(hwnd as isize) },
        WM_WINDOWPOSCHANGED => follow_player(hwnd as isize),
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
        _ => {
            if let Some(result) = media_message(hwnd, msg, wparam, lparam) {
                return result;
            }
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// A media key's message (`WM_KEYDOWN` with a `VK_MEDIA_*` key,
/// `WM_APPCOMMAND`), queued for [`take_hooked`]; its result if it was one.
/// While the windows let the keys go ([`set_media_keys`] off), a media
/// key goes to `DefWindowProc`, which makes it the `WM_APPCOMMAND` other
/// players take: winit answers every key itself without it, and the key
/// went nowhere.
fn media_message(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> Option<windows_sys::Win32::Foundation::LRESULT> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, WM_APPCOMMAND, WM_KEYDOWN, WM_KEYUP,
    };
    if !MEDIA_KEYS.load(std::sync::atomic::Ordering::Relaxed) {
        return (matches!(msg, WM_KEYDOWN | WM_KEYUP) && media_vk(wparam as u32).is_some())
            .then(|| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) });
    }
    match msg {
        WM_KEYDOWN => {
            let key = media_vk(wparam as u32)?;
            // Held keys repeat; one press is one command.
            if lparam & (1 << 30) == 0 {
                hook(Hooked::Media(key, Via::Key));
            }
            Some(0)
        }
        WM_APPCOMMAND => {
            // GET_APPCOMMAND_LPARAM: the high word without the device bits.
            let command = ((lparam as u32) >> 16) & !0xF000;
            let key = media_command(command)?;
            hook(Hooked::Media(key, Via::Command));
            Some(1)
        }
        _ => None,
    }
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

/// The window's visible rectangle in pixels: a framed window's
/// `GetWindowRect` takes in its invisible resize borders, DWM's extended
/// frame bounds do not.
pub fn visible_rect(hwnd: isize) -> Option<[i32; 4]> {
    use windows_sys::Win32::Foundation::RECT;
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmGetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *mut core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
    let mut r = RECT::default();
    let got = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut r).cast(),
            size_of::<RECT>() as u32,
        )
    } >= 0;
    if got && r.right > r.left {
        Some([r.left, r.top, r.right, r.bottom])
    } else {
        window_rect(hwnd)
    }
}

/// Whether `hwnd` is still a window.
pub fn is_window(hwnd: isize) -> bool {
    unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd as _) != 0 }
}

/// Whether `hwnd` is a window owned by `owner` (an adopted playlist).
pub fn owned_by(hwnd: isize, owner: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GW_OWNER, GetWindow};
    is_window(hwnd) && unsafe { GetWindow(hwnd as _, GW_OWNER) } as isize == owner
}

/// Whether `rect` (pixels) is at least partly on a monitor.
pub fn on_a_monitor(rect: [i32; 4]) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
    let r = RECT {
        left: rect[0],
        top: rect[1],
        right: rect[2],
        bottom: rect[3],
    };
    !unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONULL) }.is_null()
}

/// Put the window's visible rectangle at `rect` (pixels).
pub fn set_visible_rect(hwnd: isize, rect: [i32; 4]) {
    let (Some(outer), Some(visible)) = (window_rect(hwnd), visible_rect(hwnd)) else {
        return;
    };
    set_rect(
        hwnd,
        [
            rect[0] - (visible[0] - outer[0]),
            rect[1] - (visible[1] - outer[1]),
            rect[2] + (outer[2] - visible[2]),
            rect[3] + (outer[3] - visible[3]),
        ],
    );
}

/// The visible top-level windows of this thread, for the window egui makes
/// for a child viewport: eframe gives no handle for it (metalmd's way).
pub fn thread_windows() -> Vec<isize> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumThreadWindows, IsWindowVisible};
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` is the pointer to `out` passed below, and the
        // callback runs only during the call.
        let out = unsafe { &mut *(lparam as *mut Vec<isize>) };
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            out.push(hwnd as isize);
        }
        1
    }
    let mut out: Vec<isize> = Vec::new();
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            (&raw mut out) as LPARAM,
        )
    };
    out
}

/// The playlist's window, while it is open, and the player's edge it is
/// glued to.
struct PlaylistWindow {
    hwnd: isize,
    glue: Option<crate::dock::Glue>,
}

static PLAYLIST: std::sync::Mutex<Option<PlaylistWindow>> = std::sync::Mutex::new(None);
/// Whether the glued playlist follows the player (not in fullscreen).
static FOLLOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Where the playlist's window and the pointer were when its move began,
/// and the window's invisible borders (left, top, right, bottom).
static PLAYLIST_MOVE: std::sync::Mutex<Option<MoveStart>> = std::sync::Mutex::new(None);

/// The window's rectangle, the pointer, the window's invisible borders.
type MoveStart = ([i32; 4], [i32; 2], [i32; 4]);

/// Make `hwnd`, the playlist's window, the player's (`owner`): above it,
/// minimised with it, without a taskbar button; rounded corners as the
/// player's; drawn to the player's edges while moved and glued to the one
/// it touches.
pub fn adopt_playlist(hwnd: isize, owner: isize) {
    use windows_sys::Win32::UI::Shell::SetWindowSubclass;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWLP_HWNDPARENT, SetWindowLongPtrW};
    unsafe { SetWindowLongPtrW(hwnd as _, GWLP_HWNDPARENT, owner) };
    round_corners(hwnd);
    unsafe { SetWindowSubclass(hwnd as _, Some(playlist_proc), 2, 0) };
    if let Ok(mut p) = PLAYLIST.lock() {
        *p = Some(PlaylistWindow { hwnd, glue: None });
    }
    glue_playlist();
}

/// The playlist's window is gone (or going).
pub fn playlist_closed() {
    if let Ok(mut p) = PLAYLIST.lock() {
        *p = None;
    }
}

/// Glue the playlist to the player's edge it touches now, if any.
pub fn glue_playlist() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GW_OWNER, GetWindow};
    let Ok(mut p) = PLAYLIST.lock() else {
        return;
    };
    let Some(p) = p.as_mut() else {
        return;
    };
    let owner = unsafe { GetWindow(p.hwnd as _, GW_OWNER) } as isize;
    p.glue = match (visible_rect(owner), visible_rect(p.hwnd)) {
        (Some(main), Some(child)) => crate::dock::glue_of(main, child),
        _ => None,
    };
}

/// Whether the glued playlist follows the player; off in fullscreen.
pub fn set_playlist_follow(on: bool) {
    FOLLOW.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The player's window moved or was resized: the glued playlist goes
/// along, unless the player is minimised, maximized or in fullscreen.
fn follow_player(main: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsZoomed};
    if !FOLLOW.load(std::sync::atomic::Ordering::Relaxed)
        || unsafe { IsIconic(main as _) } != 0
        || unsafe { IsZoomed(main as _) } != 0
    {
        return;
    }
    let Some((hwnd, glue)) = PLAYLIST
        .lock()
        .ok()
        .and_then(|p| p.as_ref().and_then(|p| Some((p.hwnd, p.glue?))))
    else {
        return;
    };
    let (Some(main), Some(child)) = (visible_rect(main), visible_rect(hwnd)) else {
        return;
    };
    let size = [child[2] - child[0], child[3] - child[1]];
    let wanted = crate::dock::placed(main, size, glue);
    if wanted != child {
        set_visible_rect(hwnd, wanted);
    }
}

unsafe extern "system" fn playlist_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GW_OWNER, GetCursorPos, GetWindow, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_MOVING,
        WM_NCDESTROY,
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForWindow(hwnd: isize) -> u32;
    }
    let me = hwnd as isize;
    if let Some(result) = media_message(hwnd, msg, wparam, lparam) {
        return result;
    }
    // The window, the pointer and the invisible frame's widths now: where
    // a move counts from.
    let move_start = || {
        let mut cursor = POINT::default();
        match (window_rect(me), visible_rect(me)) {
            (Some(outer), Some(visible)) if unsafe { GetCursorPos(&mut cursor) } != 0 => Some((
                outer,
                [cursor.x, cursor.y],
                [
                    visible[0] - outer[0],
                    visible[1] - outer[1],
                    outer[2] - visible[2],
                    outer[3] - visible[3],
                ],
            )),
            _ => None,
        }
    };
    match msg {
        WM_ENTERSIZEMOVE => {
            if let Ok(mut m) = PLAYLIST_MOVE.lock() {
                *m = move_start();
            }
        }
        WM_EXITSIZEMOVE => {
            if let Ok(mut m) = PLAYLIST_MOVE.lock() {
                *m = None;
            }
            glue_playlist();
        }
        // As the player's own magnet: the rectangle the pointer alone
        // gives, else Windows would work the next one out from the one
        // given back and a slow move would stay stuck.
        WM_MOVING if lparam != 0 => {
            let mut start = PLAYLIST_MOVE.lock().ok().and_then(|m| *m);
            // Its size changed on the way (another monitor's scale): the
            // move counts from here, as the player's does.
            let given = unsafe { &*(lparam as *const RECT) };
            let size = (given.right - given.left, given.bottom - given.top);
            if let Some((outer, ..)) = start
                && (outer[2] - outer[0], outer[3] - outer[1]) != size
            {
                start = move_start();
                if let Ok(mut m) = PLAYLIST_MOVE.lock() {
                    *m = start;
                }
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            let owner = unsafe { GetWindow(hwnd, GW_OWNER) } as isize;
            let mut cursor = POINT::default();
            if let (Some((outer, from, edge)), Some(main)) = (start, visible_rect(owner))
                && unsafe { GetCursorPos(&mut cursor) } != 0
            {
                let (dx, dy) = (cursor.x - from[0], cursor.y - from[1]);
                let visible = [
                    outer[0] + dx + edge[0],
                    outer[1] + dy + edge[1],
                    outer[2] + dx - edge[2],
                    outer[3] + dy - edge[3],
                ];
                let dpi = match unsafe { GetDpiForWindow(me) } {
                    0 => 96,
                    dpi => dpi as i32,
                };
                let [l, t, r, b] = crate::dock::snapped(main, visible, SNAP_REACH * dpi / 96);
                let rect = unsafe { &mut *(lparam as *mut RECT) };
                *rect = RECT {
                    left: l - edge[0],
                    top: t - edge[1],
                    right: r + edge[2],
                    bottom: b + edge[3],
                };
                return 1;
            }
        }
        WM_NCDESTROY => {
            if let Ok(mut p) = PLAYLIST.lock()
                && p.as_ref().is_some_and(|p| p.hwnd == me)
            {
                *p = None;
            }
            unsafe { RemoveWindowSubclass(hwnd, Some(playlist_proc), 2) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
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

/// `bytes` in the system's ANSI code page (Windows-1251 on a Russian
/// system): what an old M3U list is written in.
pub fn ansi_text(bytes: &[u8]) -> Option<String> {
    use windows_sys::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
    let len = i32::try_from(bytes.len()).ok()?;
    if len == 0 {
        return Some(String::new());
    }
    let count =
        unsafe { MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0) };
    if count <= 0 {
        return None;
    }
    let mut wide = vec![0u16; count as usize];
    let written =
        unsafe { MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), count) };
    (written > 0).then(|| String::from_utf16_lossy(&wide[..written as usize]))
}

/// `time` in the local time zone, by the zone's rules of that date: the
/// year, month, day, hour, minute and second. `None` before 1970.
pub fn local_time(time: std::time::SystemTime) -> Option<[u16; 6]> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    // kernel32 is declared by hand, to spare a `windows-sys` feature.
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FileTimeToSystemTime(file_time: *const FILETIME, system_time: *mut SYSTEMTIME) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const core::ffi::c_void,
            universal: *const SYSTEMTIME,
            local: *mut SYSTEMTIME,
        ) -> i32;
        fn GetDynamicTimeZoneInformation(zone: *mut DynamicZone) -> u32;
        fn SystemTimeToTzSpecificLocalTimeEx(
            zone: *const DynamicZone,
            universal: *const SYSTEMTIME,
            local: *mut SYSTEMTIME,
        ) -> i32;
    }
    /// `DYNAMIC_TIME_ZONE_INFORMATION`, only passed on: 432 bytes, aligned
    /// as its `LONG`s.
    #[repr(C, align(4))]
    struct DynamicZone([u8; 432]);
    const TIME_ZONE_ID_INVALID: u32 = u32::MAX;
    // FILETIME: 100 ns intervals since 1601.
    let since = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    let ticks = u64::try_from(since.as_nanos() / 100).ok()? + 116_444_736_000_000_000;
    let file_time = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let mut universal: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    // The zone with its rules of each year (Moscow was UTC+4 in 2011-2014):
    // with no zone given, this year's rules were used for every date.
    let mut zone = DynamicZone([0; 432]);
    let ok = unsafe {
        FileTimeToSystemTime(&file_time, &mut universal) != 0
            && if GetDynamicTimeZoneInformation(&mut zone) != TIME_ZONE_ID_INVALID {
                SystemTimeToTzSpecificLocalTimeEx(&zone, &universal, &mut local) != 0
            } else {
                SystemTimeToTzSpecificLocalTime(std::ptr::null(), &universal, &mut local) != 0
            }
    };
    ok.then_some([
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wHour,
        local.wMinute,
        local.wSecond,
    ])
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
    /// A date takes the zone's rules of its year: on a Moscow machine,
    /// 2013 was UTC+4 and 2020 UTC+3 (elsewhere only the call is tried).
    #[test]
    fn local_time_by_the_rules_of_the_year() {
        let at = |secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        // 2020-07-01 and 2013-07-01, 00:00 UTC.
        let (now, then) = (
            super::local_time(at(1_593_561_600)).unwrap(),
            super::local_time(at(1_372_636_800)).unwrap(),
        );
        if now[3] == 3 && then[0] == 2013 {
            assert_eq!(then[3], 4, "{then:?}");
        }
    }

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
