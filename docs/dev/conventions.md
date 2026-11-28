# Conventions

- Every user-visible string goes through `tr!("English", "Русский")` (`i18n.rs`); the language follows Windows unless chosen in the settings. Log messages, CLI output and technical errors (mpv's) stay English.
- The version comes from `Cargo.toml` (`main::VERSION`); between releases the next version with `-dev`, and `build.rs` appends `git describe --tags --always --match v[0-9]*` (not the libmpv mirror's tag). Release: drop `-dev`, commit `Version X.Y.Z`, tag `vX.Y.Z`, then set the next `-dev` version.
- The release profile has `panic = "unwind"` and no console. A panic on a background thread started by `worker::spawn` (cover, facts, fonts, logos, update check: libraries reading other people's files and pictures from the network) is only logged, and the thread's job is lost (`worker::guarded` around each logo, so the pool goes on; a logo's kept copy is dropped); any other panic shows a message box and aborts (`main::install_panic_hook`), not unwinding through the window's loop and the engine.
- Commits in this repository end with a trailer naming the Claude model that did the work, e.g. `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` (this overrides the global "no Co-Authored-By" rule for this repo only). Commit only when the user asks.

## Releases

Releases: pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`, which runs `make_release.py` and publishes both files; the tag must equal the version in `Cargo.toml`. Creating or pushing a tag publishes a public release, so do it only when the user asks. `tools/setup.iss` installs per user. `.cargo/config.toml` links the CRT statically. These scripts mirror those of disk_flashlight, qview, qrec and metalmd; keep them in step (the libmpv steps are this project's own).

## Toolchain

Toolchain: stable MSVC Rust plus VS 2022 Build Tools. In the Bash tool, `cargo` is not on PATH; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"`. A running `metalmpv.exe` locks the release binary; `build.py` closes it first. Heredocs containing Rust code break the Bash tool (lifetime apostrophes), and backslashes in Python code passed through it get eaten; write files with the Write/Edit tools.
