# CLAUDE.md

Guidance for Claude Code in this repository. The details are in `docs/dev/`; read the part that concerns the task before starting it, and keep it current when the code changes.

## Project

MetalMPV is a Windows-only video player written in Rust with egui/eframe 0.36 (glow); playback is libmpv's (`libmpv-2.dll`, loaded at run time). The layout is a classic player's: a frameless window with its own title row, the picture, a control bar under it, an optional playlist on the right, a yellow OSD line in the top left corner after seeks. Priorities: opening a file fast, instant keyframe seeking, responsiveness, few settings. `README.md` / `README.ru.md` are the user-facing docs and must be kept in sync (both languages, cross-linked, formal tone, no imperatives).

The name is written `MetalMPV` wherever people read it (UI, window title, READMEs, installer, `%APPDATA%\MetalMPV`, `%LOCALAPPDATA%\Programs\MetalMPV`, `MetalMPV_<ver>_Setup.exe`) and `metalmpv` wherever it is an identifier (Cargo package, `metalmpv.exe`, the GitHub repo `Fan4Metal/metalmpv`, the single-instance names).

## Where the details are

- `docs/dev/architecture.md`: every module, what it does and why, with measurements, and the ignored tests that play real files. Read a module's entry before changing it; update the entry with the change.
- `docs/dev/settings.md`: every persisted key; a new setting is added there.
- `docs/dev/testing.md`: all commands and `METALMPV_*` switches, trying a change, GUI checks.
- `docs/dev/libmpv.md`: the pinned DLL, its mirror, updating it, the GPL obligations.
- `docs/dev/conventions.md`: the conventions below in full, versions and releases, the release profile and panics, the toolchain.

Map: `main.rs` (arguments, start), `app.rs` + `app/` (the eframe `App`), `mpv/` (the C API, the safe layer), `player.rs`, `video.rs` (rendering), `playlist.rs`, `input.rs` + `keymap.rs`, `settings.rs`, `win.rs`, `ui/`, the second cores (`compare.rs`, `clip.rs`, `video_map.rs`, `thumbs.rs`).

## Commands

```
uv run build.py                  # closes a metalmpv.exe running from target\, fetches libmpv if missing, cargo build --release
cargo test                       # unit tests (no DLL needed)
cargo clippy --all-targets       # must stay warning-free
cargo fmt                        # default rustfmt (width 100)
python tools/make_release.py --no-tests --install   # release build, installer, installs over the installed copy
python tools/notices.py          # after the dependencies change (a test fails until then)
METALMPV_TRACE=1                 # debug log with timings and mpv's messages
```

Toolchain: stable MSVC Rust plus VS 2022 Build Tools. In the Bash tool `cargo` is not on PATH: prefix `export PATH="$HOME/.cargo/bin:$PATH"`. Write files with the Write/Edit tools, not heredocs (Rust's apostrophes break the Bash tool, backslashes get eaten).

Trying a change: `cargo test` and `cargo clippy --all-targets`, then `uv run build.py` and run `target\release\metalmpv.exe <file>`; then `make_release.py --no-tests --install`. Details: `testing.md`.

Releases: a pushed tag `vX.Y.Z` publishes a public release: only when the user asks. Between releases the version is the next one with `-dev` (details in `conventions.md`).

## Conventions

- Every user-visible string goes through `tr!("English", "Русский")`; logs, CLI output and technical errors (mpv's) stay English.
- render.h's threading: the render API is used on the UI thread, so the UI thread never calls anything that waits for the core; all mpv calls are asynchronous (the synchronous ones are not wrapped), and nothing calls mpv inside `ctx.input/memory/data` closures (egui's lock).
- Seek commands always name their mode (`relative+keyframes`, `absolute+exact`…).
- Code that reads other people's files or the network runs on `worker::spawn` / under `worker::guarded`: a panic there is only logged.
- The default fonts lack the media symbols: icons are drawn in `ui/icons.rs`.
- Commits end with a trailer naming the Claude model, e.g. `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (overrides the global rule for this repo).
- GUI checks: the OpenGL picture cannot be captured with `PrintWindow`, so the user checks it; warn before launching a window (it takes the focus); close test windows with `taskkill /PID`, without `/F`.
