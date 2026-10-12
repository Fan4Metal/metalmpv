<p align="center">
  <img src="images/icon.png" width="128" height="128" alt="Metal Player icon">
</p>

<h1 align="center">Metal Player</h1>

<p align="center">A fast video player for Windows built on libmpv</p>

<p align="center">
  <a href="https://github.com/Fan4Metal/metal_player/releases/latest"><img src="https://img.shields.io/github/v/release/Fan4Metal/metal_player?label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-10%2B-0078D6" alt="Windows 10 or later">
  <img src="https://img.shields.io/badge/Rust-B7410E?logo=rust&logoColor=white" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center"><b>English</b> | <a href="README.ru.md">Русский</a></p>

Metal Player is a video player for Windows with a classic layout: the picture fills the window, the seek bar and the buttons sit under it with Play in the middle, and the playlist opens on the right. Playback is done by [libmpv](https://mpv.io), the library of the mpv player, so practically every video and audio format plays, with hardware decoding where the graphics card supports it. The program is built for opening files quickly and for instant seeking on keyframes.

![Metal Player playing a video, with the control bar under the picture](images/screenshot.png)

*Screenshot of version 0.1.0.*

## Features

- **Fast start**: the playback engine starts while the window is being created; on a warm start the first frame of a 1080p file appears about 0.4 s after the program is launched.
- **Instant seeking on keyframes** with the arrow keys, exact seeking and frame stepping on other keys (see [Keyboard shortcuts](#keyboard-shortcuts)); every seek is reported on the screen.
- **Every common format**: MP4, MKV, AVI, MOV, WebM, TS, WMV and more, audio files included; subtitles next to the video are loaded automatically.
- **Hardware decoding** (NVDEC, D3D11VA and others, chosen by libmpv), which can be turned off in the settings.
- **Playlist**: files opened together, dropped onto the window or added with the **+** button of the playlist; optionally the other media files of the folder of an opened file.
- **One window**: a file opened from Explorer while the player runs is played in the window already open.
- **English or Russian interface**: it follows the language of Windows and can be changed in the About window (`F1`).
- **Portable**: the [portable archive](#installation) keeps its settings next to the program.
- **Privacy**: no network access beyond the optional update check (see [Update check and privacy](#update-check-and-privacy)).

## Installation

The installer and the portable archive are on the [Releases](https://github.com/Fan4Metal/metal_player/releases) page.

The installer, `Metal_Player_<version>_Setup.exe`, needs no administrator rights: the program is placed in `%LOCALAPPDATA%\Programs\Metal Player`. The installed program keeps its settings in `%APPDATA%\Metal Player`.

The portable archive, `Metal_Player_<version>_portable.zip`, holds the program in a `Metal Player` folder and runs without installation. It also holds an empty `metal_player.ron`. While that file is next to `metal_player.exe` and the folder is writable, the settings are kept in it, so the program can be carried on a removable drive. Without the file, the program keeps them in `%APPDATA%\Metal Player`, as the installed program does. The About window shows where the settings are kept.

Both contain `libmpv-2.dll` (about 115 MB unpacked), the playback engine, which must stay next to `metal_player.exe`.

The executables are not code-signed, so Windows SmartScreen may warn about an unknown publisher on first launch.

## Usage

A file is opened with the **Open** button (the eject symbol, `Ctrl+O`), by dropping it onto the window, from Explorer (**Open with**), or from the command line: `metal_player.exe file.mkv [more files]`. Several files make a playlist; at the end of a file the next one starts.

The window has its own title row: it is moved by dragging the title, the picture or the free space of the control bar, resized by its edges, and maximised by a double click on the title. A double click on the picture or `Enter` switches to fullscreen, where only the picture is left; the control bar appears over it while the mouse moves, and the pointer hides after half a second of rest.

The control bar holds the seek bar (a click or a drag moves there; the time under the pointer is shown), the position and the duration, the buttons **Open**, **Stop**, **Previous file**, **Play/Pause**, **Next file**, **Settings** and **Playlist**, the volume and the fullscreen button. A right click on the picture opens a menu with the same commands.

The playlist (`L`) lists the files with the current one highlighted; a double click plays an entry. The icon buttons above the list add files (**+**), remove the selected entry (**−**, `Delete`), sort the list by name, numbers by value (the bars with an arrow), and clear it (the bin); their names are shown in the tooltips.

### Settings

The settings window (`F5` or the gear button) lists the categories on the left and shows the settings of the chosen one on the right in framed groups. **Reset** under the list puts every setting back to its default.

| Category | Setting | Default | Meaning |
|---|---|---|---|
| General | Interface language | as in Windows | Also in the About window |
| Updates | Check for updates at start-up | off | See [Update check and privacy](#update-check-and-privacy); **Check now** asks at once |
| General | Opening a file adds the other media files of its folder | off | One opened file brings the media files of its folder into the playlist |
| Playback | Left / Right | 10 s | The step of `←` and `→`, on keyframes |
| Playback | Ctrl+Left / Right | 60 s | The step of `Ctrl+←` and `Ctrl+→`, on keyframes |
| Video | Hardware decoding (GPU) | on | Decoding on the graphics card where possible; the decoder in use is shown below |

The **Keys** category lists the keyboard shortcuts for reference. The volume, the window and the playlist's visibility and width are remembered between runs.

## Keyboard shortcuts

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `←` / `→` | Back / forward by the short step (10 s), on keyframes |
| `Ctrl+←` / `Ctrl+→` | Back / forward by the long step (60 s), on keyframes |
| `Shift+←` / `Shift+→` | Back / forward by 1 s to the exact frame |
| `,` / `.` (or `D` / `F`) | Previous / next frame |
| `Home` / `End` | Start / end of the file |
| `↑` / `↓`, mouse wheel | Volume up / down |
| `M` | Mute |
| `S` | Stop |
| `Page Up` / `Page Down` | Previous / next file of the playlist |
| `Enter`, `F11`, double click | Fullscreen; `Esc` leaves it |
| `Ctrl+O` | Open files |
| `L` | Show or hide the playlist |
| `Delete` | Remove the selected playlist entry |
| `F5` | Settings |
| `F1` | About |

A seek on keyframes is instant, since nothing has to be decoded before the frame shown; it lands on the keyframe nearest to the target, and the on-screen line reports where. An exact seek decodes from the previous keyframe up to the target frame.

## Update check and privacy

The update check is optional and off by default. With **Check for updates at start-up (once a day)** enabled in the About window, the program asks the GitHub Releases API (`api.github.com`) for the latest release at most once a day; **Check now** in the same window asks at once. The request carries nothing but the program version in its `User-Agent` header, and nothing is downloaded or installed: when a newer release exists, a button with its version appears in the title row and opens the release page in the browser. Otherwise the program does not access the network; libmpv's network features (streaming, `youtube-dl`) are not used.

## Building

Requirements: Windows 10 or later, a stable Rust toolchain (MSVC target, 2024 edition), the Visual Studio 2022 Build Tools with the C++ workload and Python 3. For `build.py`, [uv](https://docs.astral.sh/uv/).

```
uv run build.py          # target\release\metal_player.exe with libmpv-2.dll next to it
cargo test
```

`build.py` closes a `metal_player.exe` running from the project's `target` folder, because it would lock the file, fetches libmpv when it is missing (`tools/get_libmpv.py`) and runs `cargo build --release`. `tools/get_libmpv.py` downloads a pinned build of libmpv from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake) and the standalone `7zr.exe` of 7-Zip to unpack it, checks both against their SHA-256 and puts them into `vendor` (not in the repository); `build.rs` copies the DLL next to the executable. The executable links the C runtime statically (`.cargo/config.toml`) and loads libmpv at run time, so it needs nothing beyond Windows and `libmpv-2.dll`.

<details>
<summary>Setting up the environment and the source layout</summary>

The tools can be installed with winget:

```
winget install Rustlang.Rustup Microsoft.VisualStudio.2022.BuildTools Python.Python.3.12 astral-sh.uv JRSoftware.InnoSetup
```

The C++ workload of the Build Tools ("Desktop development with C++") is selected in the Visual Studio Installer.

| Path | Contents |
|---|---|
| `src/main.rs` | Command line, start of the engine, window options |
| `src/app.rs` | The window: layout, commands, events |
| `src/mpv/` | libmpv's C API and its safe wrapper |
| `src/player.rs`, `src/video.rs` | Playback state and commands; rendering into the window |
| `src/playlist.rs`, `src/input.rs`, `src/format.rs` | Playlist, keys, texts of times and of the on-screen line |
| `src/ui/` | Title row, control bar, playlist, on-screen line, dialogs |
| `tools/` | libmpv download, release script, installer script, icon export |

</details>

## Installer and release

The release script builds the executable, the installer and the portable archive. It needs [Inno Setup 6](https://jrsoftware.org/isinfo.php).

```
python tools/make_release.py              # libmpv, tests, release build, installer, archive
python tools/make_release.py --no-tests   # the same without cargo test
python tools/make_release.py --install    # then a silent installation over the installed copy
```

The script writes the files to `dist` and takes the version from `Cargo.toml`. The installer script is `tools/setup.iss`; `metal_player.exe --export-icon <file.ico>` writes the installer icon.

Releases on GitHub are built by the **Release** workflow (`.github/workflows/release.yml`). After the version in `Cargo.toml` is updated and committed, pushing a matching tag publishes a release with the installer and the portable archive:

```
git tag v0.1.0
git push origin v0.1.0
```

The workflow stops if the tag does not match the version in `Cargo.toml`. Running the workflow manually (Actions → Release → Run workflow) builds the files and attaches them to the run, without making a release.

## Built with

- [mpv / libmpv](https://mpv.io): decoding, playback and rendering of every file;
- [mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake): the Windows build of libmpv that is shipped;
- [egui / eframe](https://github.com/emilk/egui): the user interface and the window;
- [7-Zip](https://www.7-zip.org): unpacking libmpv during the build;
- [Inno Setup](https://jrsoftware.org/isinfo.php): the installer.

## License

MIT, see [LICENSE](LICENSE). The included `libmpv-2.dll` is licensed under the GNU GPL version 3, so the program as distributed with it is subject to the terms of that license; the sources and details are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
