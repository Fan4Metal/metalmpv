# Testing

## Commands

```
python tools/get_libmpv.py       # fetches the pinned libmpv-2.dll into vendor\libmpv (build.py and make_release.py do it)
uv run build.py                  # closes a metalmpv.exe running from target\, fetches libmpv if missing, cargo build --release
cargo build --release            # target\release\metalmpv.exe; build.rs copies libmpv-2.dll next to it
cargo test                       # unit tests (no DLL needed)
cargo test sys -- --ignored      # loads vendor\libmpv\libmpv-2.dll for real
cargo clippy --all-targets       # must stay warning-free
cargo fmt                        # default rustfmt (width 100)
python tools/make_release.py     # libmpv, tests, release build, dist\MetalMPV_<ver>_Setup.exe (Inno Setup 6) and _portable.zip
python tools/make_release.py --no-tests --install   # the same, then installs it silently over the installed copy
python tools/icon_png.py         # images\icon.png from the exe's icon
python tools/notices.py          # licenses\THIRD_PARTY_NOTICES.txt after the dependencies change (a test fails until then)
METALMPV_TRACE=1                 # debug log with millisecond timings (start → window → file loaded → first frame), mpv's info messages
METALMPV_TRACE=2                 # also mpv's verbose messages
METALMPV_HWDEC=nvdec             # force mpv's hwdec option (measurements)
METALMPV_MPV="name=value;…"      # extra mpv options (measurements, diagnostics)
METALMPV_RENDERER=gpu-next       # mpv's gpu-next renderer instead of gpu (libmpv of client API 2.7 and later)
```

## Trying a change

Trying a change: `cargo test` and `cargo clippy --all-targets`, then `uv run build.py` and run `target\release\metalmpv.exe <file>`; when the change concerns the installed copy (installer, portable mode, the DLL) or the user runs the program all day, `python tools/make_release.py --no-tests --install`. A test run with logs: start the exe with `METALMPV_TRACE=1` and stdout/stderr redirected to a file (the release exe has no console, but inherited handles work), close it with `taskkill /PID` (no `/F`).

## GUI checks

- GUI checks: the OpenGL client area cannot be captured with `PrintWindow` on an NVIDIA machine, so the user checks the picture; logs (`METALMPV_TRACE`) show what mpv does. A launched window takes the focus: warn the user before launching test windows and before any simulated input, and close test windows with `taskkill /PID` (WM_CLOSE, which saves the settings).
