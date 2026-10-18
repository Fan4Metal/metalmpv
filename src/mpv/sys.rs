//! The part of the libmpv C API the player uses (client.h 2.5, render.h,
//! render_gl.h), declared by hand. The DLL is loaded at run time from the
//! exe's folder with `LoadLibraryExW` and its functions are looked up one by
//! one, so the exe has no import of it: without `libmpv-2.dll` the program
//! still starts and says what is missing, and no import library is needed
//! to build it.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The DLL's file name, next to the exe (`build.rs` copies it there).
pub const DLL_NAME: &str = "libmpv-2.dll";
/// The major version of the client API this file is written for.
pub const API_MAJOR: c_ulong = 2;
/// The least minor version: 2.3 is mpv 0.38, whose `loadfile` takes an
/// index before the options (a resumed file failed on an older DLL).
pub const API_MINOR: c_ulong = 3;

pub enum mpv_handle {}
pub enum mpv_render_context {}

pub type mpv_format = c_int;
pub const MPV_FORMAT_STRING: mpv_format = 1;
pub const MPV_FORMAT_FLAG: mpv_format = 3;
pub const MPV_FORMAT_INT64: mpv_format = 4;
pub const MPV_FORMAT_DOUBLE: mpv_format = 5;
pub const MPV_FORMAT_NODE: mpv_format = 6;
pub const MPV_FORMAT_NODE_ARRAY: mpv_format = 7;
pub const MPV_FORMAT_NODE_MAP: mpv_format = 8;
pub const MPV_FORMAT_BYTE_ARRAY: mpv_format = 9;

pub type mpv_event_id = c_int;
pub const MPV_EVENT_NONE: mpv_event_id = 0;
pub const MPV_EVENT_SHUTDOWN: mpv_event_id = 1;
pub const MPV_EVENT_LOG_MESSAGE: mpv_event_id = 2;
pub const MPV_EVENT_GET_PROPERTY_REPLY: mpv_event_id = 3;
pub const MPV_EVENT_SET_PROPERTY_REPLY: mpv_event_id = 4;
pub const MPV_EVENT_COMMAND_REPLY: mpv_event_id = 5;
pub const MPV_EVENT_START_FILE: mpv_event_id = 6;
pub const MPV_EVENT_END_FILE: mpv_event_id = 7;
pub const MPV_EVENT_FILE_LOADED: mpv_event_id = 8;
pub const MPV_EVENT_VIDEO_RECONFIG: mpv_event_id = 17;
pub const MPV_EVENT_SEEK: mpv_event_id = 20;
pub const MPV_EVENT_PLAYBACK_RESTART: mpv_event_id = 21;
pub const MPV_EVENT_PROPERTY_CHANGE: mpv_event_id = 22;
pub const MPV_EVENT_QUEUE_OVERFLOW: mpv_event_id = 24;

pub const MPV_END_FILE_REASON_EOF: c_int = 0;
pub const MPV_END_FILE_REASON_STOP: c_int = 2;
pub const MPV_END_FILE_REASON_QUIT: c_int = 3;
pub const MPV_END_FILE_REASON_ERROR: c_int = 4;
pub const MPV_END_FILE_REASON_REDIRECT: c_int = 5;

/// `mpv_error`: an option the build does not know.
pub const MPV_ERROR_OPTION_NOT_FOUND: c_int = -5;

pub type mpv_render_param_type = c_int;
pub const MPV_RENDER_PARAM_INVALID: mpv_render_param_type = 0;
pub const MPV_RENDER_PARAM_API_TYPE: mpv_render_param_type = 1;
pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: mpv_render_param_type = 2;
pub const MPV_RENDER_PARAM_OPENGL_FBO: mpv_render_param_type = 3;
pub const MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME: mpv_render_param_type = 12;
pub const MPV_RENDER_PARAM_SKIP_RENDERING: mpv_render_param_type = 13;
pub const MPV_RENDER_API_TYPE_OPENGL: &std::ffi::CStr = c"opengl";
pub const MPV_RENDER_UPDATE_FRAME: u64 = 1;

#[repr(C)]
pub struct mpv_event {
    pub event_id: mpv_event_id,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

/// A value of any format; maps and arrays hold more of them.
#[repr(C)]
pub struct mpv_node {
    pub u: mpv_node_u,
    pub format: mpv_format,
}

#[repr(C)]
pub union mpv_node_u {
    pub string: *mut c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double_: f64,
    pub list: *mut mpv_node_list,
    pub ba: *mut mpv_byte_array,
}

/// Raw bytes (`screenshot-raw`'s picture).
#[repr(C)]
pub struct mpv_byte_array {
    pub data: *mut c_void,
    pub size: usize,
}

/// The entries of an array or a map (`keys` is null for an array).
#[repr(C)]
pub struct mpv_node_list {
    pub num: c_int,
    pub values: *mut mpv_node,
    pub keys: *mut *mut c_char,
}

#[repr(C)]
pub struct mpv_event_property {
    pub name: *const c_char,
    pub format: mpv_format,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct mpv_event_log_message {
    pub prefix: *const c_char,
    pub level: *const c_char,
    pub text: *const c_char,
    pub log_level: c_int,
}

#[repr(C)]
pub struct mpv_event_start_file {
    pub playlist_entry_id: i64,
}

/// The result of a command (`MPV_EVENT_COMMAND_REPLY`), on success.
#[repr(C)]
pub struct mpv_event_command {
    pub result: mpv_node,
}

#[repr(C)]
pub struct mpv_event_end_file {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: c_int,
}

#[repr(C)]
pub struct mpv_render_param {
    pub type_: mpv_render_param_type,
    pub data: *mut c_void,
}

pub type GetProcAddress =
    unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void;

#[repr(C)]
pub struct mpv_opengl_init_params {
    pub get_proc_address: Option<GetProcAddress>,
    pub get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
pub struct mpv_opengl_fbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

pub type Callback = unsafe extern "C" fn(ctx: *mut c_void);

/// Declares the functions as fields of [`Api`] and the lookup of each.
macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The functions of a loaded libmpv.
        pub struct Api {
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Api {
            /// Look every function up in `module`; the first one missing
            /// is named in the error.
            unsafe fn resolve(module: *mut c_void) -> Result<Self, String> {
                use windows_sys::Win32::System::LibraryLoader::GetProcAddress;
                Ok(Self {
                    $($name: {
                        let symbol = unsafe { GetProcAddress(module, concat!(stringify!($name), "\0").as_ptr()) };
                        match symbol {
                            // SAFETY: the declaration above follows the C
                            // header; both are plain function pointers.
                            Some(f) => unsafe {
                                std::mem::transmute::<unsafe extern "system" fn() -> isize, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(f)
                            },
                            None => return Err(format!("{} has no function {}", DLL_NAME, stringify!($name))),
                        }
                    },)*
                })
            }
        }
    };
}

api! {
    mpv_client_api_version: fn() -> c_ulong;
    mpv_error_string: fn(c_int) -> *const c_char;
    mpv_create: fn() -> *mut mpv_handle;
    mpv_initialize: fn(*mut mpv_handle) -> c_int;
    mpv_terminate_destroy: fn(*mut mpv_handle);
    mpv_set_option_string: fn(*mut mpv_handle, *const c_char, *const c_char) -> c_int;
    mpv_command_async: fn(*mut mpv_handle, u64, *mut *const c_char) -> c_int;
    mpv_set_property_async: fn(*mut mpv_handle, u64, *const c_char, mpv_format, *mut c_void) -> c_int;
    mpv_get_property_async: fn(*mut mpv_handle, u64, *const c_char, mpv_format) -> c_int;
    mpv_observe_property: fn(*mut mpv_handle, u64, *const c_char, mpv_format) -> c_int;
    mpv_request_log_messages: fn(*mut mpv_handle, *const c_char) -> c_int;
    mpv_wait_event: fn(*mut mpv_handle, f64) -> *mut mpv_event;
    mpv_set_wakeup_callback: fn(*mut mpv_handle, Option<Callback>, *mut c_void);
    mpv_render_context_create: fn(*mut *mut mpv_render_context, *mut mpv_handle, *mut mpv_render_param) -> c_int;
    mpv_render_context_set_update_callback: fn(*mut mpv_render_context, Option<Callback>, *mut c_void);
    mpv_render_context_update: fn(*mut mpv_render_context) -> u64;
    mpv_render_context_render: fn(*mut mpv_render_context, *mut mpv_render_param) -> c_int;
    mpv_render_context_report_swap: fn(*mut mpv_render_context);
    mpv_render_context_free: fn(*mut mpv_render_context);
}

/// Why libmpv could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// No DLL in the places it is looked for.
    NotFound { looked_in: Vec<PathBuf> },
    /// A DLL is there but cannot be used (not a 64-bit DLL, a function or
    /// the client API version missing); the reason, technical.
    Unusable(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NotFound { looked_in } => {
                let places: Vec<String> =
                    looked_in.iter().map(|p| p.display().to_string()).collect();
                write!(f, "{DLL_NAME} not found (looked in {})", places.join(", "))
            }
            LoadError::Unusable(reason) => f.write_str(reason),
        }
    }
}

/// libmpv, loaded on first use; the error says why it could not be.
pub fn api() -> Result<&'static Api, &'static LoadError> {
    static API: OnceLock<Result<Api, LoadError>> = OnceLock::new();
    API.get_or_init(|| {
        let candidates = dll_candidates();
        match candidates.iter().find(|p| p.is_file()) {
            Some(path) => load(path).map_err(LoadError::Unusable),
            None => Err(LoadError::NotFound {
                looked_in: candidates,
            }),
        }
    })
    .as_ref()
}

/// Next to the exe; a debug build also takes the one `tools/get_libmpv.py`
/// fetched into the project (tests run from `target\debug\deps`).
fn dll_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        out.push(dir.join(DLL_NAME));
    }
    if cfg!(debug_assertions) {
        out.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("vendor")
                .join("libmpv")
                .join(DLL_NAME),
        );
    }
    out
}

/// Load the DLL at `path` and look its functions up.
pub fn load(path: &Path) -> Result<Api, String> {
    use windows_sys::Win32::System::LibraryLoader::{
        LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
    };
    let wide = crate::win::wide(path);
    // The DLL's own dependencies are looked for in its folder first.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        return Err(format!(
            "cannot load {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    // The module stays loaded for the life of the process.
    let api = unsafe { Api::resolve(module)? };
    let version = unsafe { (api.mpv_client_api_version)() };
    if version >> 16 != API_MAJOR || version & 0xffff < API_MINOR {
        return Err(format!(
            "{} has client API {}.{}, {}.{} or a later {}.x (mpv 0.38 or later) is needed",
            path.display(),
            version >> 16,
            version & 0xffff,
            API_MAJOR,
            API_MINOR,
            API_MAJOR
        ));
    }
    Ok(api)
}

/// The client API version of the loaded DLL as "2.5".
pub fn api_version(api: &Api) -> String {
    let v = unsafe { (api.mpv_client_api_version)() };
    format!("{}.{}", v >> 16, v & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layouts of the C structs on x64 (checked against the header).
    #[test]
    fn struct_sizes() {
        assert_eq!(size_of::<mpv_event>(), 24);
        assert_eq!(size_of::<mpv_event_property>(), 24);
        assert_eq!(size_of::<mpv_event_log_message>(), 32);
        assert_eq!(size_of::<mpv_event_end_file>(), 32);
        assert_eq!(size_of::<mpv_event_start_file>(), 8);
        assert_eq!(size_of::<mpv_event_command>(), 16);
        assert_eq!(size_of::<mpv_render_param>(), 16);
        assert_eq!(size_of::<mpv_opengl_init_params>(), 16);
        assert_eq!(size_of::<mpv_opengl_fbo>(), 16);
        assert_eq!(size_of::<mpv_node>(), 16);
        assert_eq!(size_of::<mpv_node_list>(), 24);
        assert_eq!(size_of::<mpv_byte_array>(), 16);
    }

    #[test]
    fn missing_dll_is_an_error() {
        let path = std::env::temp_dir()
            .join("no_such_dir_metalmpv")
            .join(DLL_NAME);
        let err = load(&path).err().expect("must fail");
        assert!(err.contains("no_such_dir_metalmpv"), "{err}");
    }

    /// Loads the fetched DLL for real: `cargo test sys -- --ignored`.
    #[test]
    #[ignore = "needs vendor/libmpv"]
    fn loads_the_dll() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/libmpv")
            .join(DLL_NAME);
        let api = load(&path).unwrap();
        assert!(api_version(&api).starts_with("2."));
    }
}
