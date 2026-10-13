<p align="center">
  <img src="images/icon.png" width="128" height="128" alt="MetalMPV icon">
</p>

<h1 align="center">MetalMPV</h1>

<p align="center">A fast video player for Windows built on libmpv</p>

<p align="center">
  <a href="https://github.com/Fan4Metal/metalmpv/releases/latest"><img src="https://img.shields.io/github/v/release/Fan4Metal/metalmpv?label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-10%2B-0078D6" alt="Windows 10 or later">
  <img src="https://img.shields.io/badge/Rust-B7410E?logo=rust&logoColor=white" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center"><b>English</b> | <a href="README.ru.md">Русский</a></p>

MetalMPV is a video player for Windows with a classic layout: the picture fills the window, the seek bar and the buttons sit under it with Play in the middle, and the playlist opens on the right. Playback is done by [libmpv](https://mpv.io), the library of the mpv player, so practically every video and audio format plays, with hardware decoding where the graphics card supports it. The program is built for opening files quickly and for instant seeking on keyframes.

![MetalMPV playing a video, with the control bar under the picture](images/screenshot.png)

*Screenshot of version 0.1.0.*

## Features

- **Fast start**: the playback engine starts while the window is being created; on a warm start the first frame of a 1080p file appears about 0.4 s after the program is launched.
- **Instant seeking on keyframes** with the arrow keys, exact seeking and frame stepping on other keys (see [Keyboard shortcuts](#keyboard-shortcuts)); every seek is reported on the screen.
- **Every common format**: MP4, MKV, AVI, MOV, WebM, TS, WMV and more, audio files included; subtitles next to the video are loaded automatically.
- **Audio tracks and subtitles** in one window by the volume (`T`), both lists side by side; subtitles from a file of their own by **Load from a file…** or by dropping the file onto the playing video (see [Audio and subtitles](#audio-and-subtitles)).
- **File information** (`I`): the cover, the tags, the size, the container and the details of every track in a window that stays over the player while it plays; the cover of an audio file fills the picture (see [File information](#file-information)).
- **Window for the picture**: 50 %, 100 %, 150 % or 200 % of the video's size, the largest window without black bars, the window fitted to the picture, or fullscreen without black bars (`1`–`6`, `` ` ``).
- **Chapters**: marks on the seek bar, which can be turned off, and a list under the playlist to go to a chapter (see [Chapters](#chapters)).
- **Hardware decoding** (NVDEC, D3D11VA and others, chosen by libmpv), which can be turned off in the settings.
- **Sound settings**: the output device, the channels (automatic or a layout chosen by hand), a volume boost up to 200 %, normalization that evens the volume out, and Dolby and DTS sound sent to a receiver undecoded; the sound can be moved against the picture with `Ctrl+=` and `Ctrl+-`.
- **Video panel** (`F7`): brightness, contrast, saturation, hue and gamma, sharpness, noise reduction and debanding, a forced aspect ratio, rotation and flips, and saving the frame (see [Video panel](#video-panel)).
- **Resume**: a film opened again goes on from where it was stopped (see [Settings](#settings)).
- **Playlist**: files opened together, dropped onto the window or added with the **+** button of the playlist; optionally the other media files of the folder of an opened file.
- **One window**: a file opened from Explorer while the player runs is played in the window already open.
- **File types**: the player can be registered with Windows for its video and audio formats, each with an icon of its own, and chosen as the default player (see [File types](#file-types)).
- **English or Russian interface**: it follows the language of Windows and can be changed in the settings (`F5`).
- **Portable**: the [portable archive](#installation) keeps its settings next to the program.
- **Privacy**: no network access beyond the optional update check (see [Update check and privacy](#update-check-and-privacy)).

## Installation

The installer and the portable archive are on the [Releases](https://github.com/Fan4Metal/metalmpv/releases) page.

The installer, `MetalMPV_<version>_Setup.exe`, needs no administrator rights: the program is placed in `%LOCALAPPDATA%\Programs\MetalMPV`. The installed program keeps its settings in `%APPDATA%\MetalMPV`. Its **File types** task, on by default, registers the player for its formats (see [File types](#file-types)); on the last page it offers to open Windows Settings to choose the player as the default. Uninstalling removes the registration.

The portable archive, `MetalMPV_<version>_portable.zip`, holds the program in a `MetalMPV` folder and runs without installation. It also holds an empty `metalmpv.ron`. While that file is next to `metalmpv.exe` and the folder is writable, the settings are kept in it, so the program can be carried on a removable drive. Without the file, the program keeps them in `%APPDATA%\MetalMPV`, as the installed program does. The About window shows where the settings are kept.

Both contain `libmpv-2.dll` (about 115 MB unpacked), the playback engine, which must stay next to `metalmpv.exe`.

The executables are not code-signed, so Windows SmartScreen may warn about an unknown publisher on first launch.

## Usage

A file is opened with the **Open** button (the eject symbol, `Ctrl+O`), by dropping it onto the window, from Explorer (**Open with**), or from the command line: `metalmpv.exe file.mkv [more files]`. Several files make a playlist; at the end of a file the next one starts. An M3U or M3U8 playlist opened, dropped or given on the command line is replaced by its entries (relative paths are from the playlist's folder). `Ctrl+U` or **Open an address…** in the context menu plays a direct link to a video or a stream (http, https, rtsp, rtmp, udp and others; an address on the command line works too); pages of YouTube and other sites are not supported, since the player does not use yt-dlp. While nothing plays, the start screen lists the files played last under the RECENT heading (up to eight, as many as the window's height allows); a click plays one, and **Clear the list** empties it, as does **Forget all** under Resume in the settings. The taskbar button shows the position as its progress (yellow while paused), and its thumbnail has the buttons **Previous file**, **Play**/**Pause** and **Next file**. While a video plays, the display does not turn off and the computer does not sleep; while only sound plays, the computer does not sleep, and when playback pauses or ends, Windows' power settings apply again.

The window has its own title row: it is moved by dragging the title, the picture or the free space of the control bar, resized by its edges, and maximised by a double click on the title. A double click on the picture or `Enter` switches to fullscreen, where only the picture is left; the control bar at the bottom and the file's name at the top appear over it independently, each while the pointer is near its edge, and the pointer itself hides after half a second of rest away from them. The strip with the name can be turned off, and the transparency of both is set in the settings.

`1`, `2`, `3` and `4` size the window so that the picture is shown at 50 %, 100 %, 150 % or 200 % of the video's size (at 100 %, a pixel of the video on a pixel of the screen), `5` makes the largest window the screen holds with the picture's shape, so that there are no black bars, and `` ` `` (`Ё` in the Russian layout) fits the window to the picture shown, removing the black bars. A maximised window is restored first and fullscreen is left; a window that would not fit the screen is made smaller, the picture keeping its shape, and keeps its centre where possible. `6` switches to fullscreen with the picture enlarged to fill the screen, its edges cropped, so that there are no black bars; `6` again shows the picture whole with the black bars, still in fullscreen, and leaving fullscreen does so too. The same commands are in the **Zoom** submenu of the picture's context menu.

The control bar holds the seek bar (a click or a drag moves there; the time under the pointer is shown), the time (a click on it switches between the position with the duration, the position alone and the time remaining; the choice is kept), the buttons **Open**, **Stop**, **Previous file**, **Play/Pause**, **Next file**, **Settings** and **Playlist**, the volume and the fullscreen button. A right click on the picture opens a menu with the same commands.

The playlist (`L`) lists the files with the current one highlighted; a double click plays an entry. The icon buttons above the list add files (**+**), remove the selected entry (**−**, `Delete`), sort the list by name, numbers by value (the bars with an arrow), clear it (the bin), save it as an M3U8 playlist (the arrow into a tray) and shuffle it (crossing arrows; the current file plays on); their names are shown in the tooltips. The loop button on the right, `R` or the **Repeat** submenu of the context menu switches the repeat mode, which is kept: off, the file (played again without a gap) or the list (the first file after the last; **Next** and **Previous** go round).

### Video panel

`F7` or **Video settings…** in the picture's context menu opens a panel over the player; playback and the keys go on while it is open, and `F7` or `Esc` closes it. Its sliders set the brightness, the contrast, the saturation, the hue and the gamma from −100 to 100 (a double click puts one back to 0), and the sharpness (AMD's contrast adaptive sharpening). Noise reduction smooths out grain and compression noise, and gradient smoothing removes the steps in skies and dark scenes. The aspect ratio is the file's or one forced on it: 4:3, 16:9, 16:10, 1.85:1, 2.35:1 or 2.39:1 (also in the **Aspect ratio** submenu of the picture's context menu). The buttons turn the picture left or right by 90° and flip it upside down or mirror it. The sharpness, the noise reduction and the flips are FFmpeg filters run by the processor (a hardware-decoded frame is copied back for them); the colour, the rotation and the gradient smoothing are done by the graphics card. The adjustments apply to every file until the program is closed; **Reset all** puts them back.

**Save the frame** (`Ctrl+E`, also in the context menu) saves the frame on screen as decoded, without subtitles, as a PNG named after the file and the position into `Pictures\MetalMPV`; **Open the folder** opens that folder.

### Chapters

When the file has chapters, the seek bar is cut at the start of each, and the tooltip over the bar names the chapter under the pointer. The marks are turned off and on in the **Chapters** submenu of the picture's context menu or in the settings (**Playback**). Under the playlist, the **Chapters** section lists them with their starts, the playing one highlighted; a click goes to a chapter, and a click on the header folds the section. The same list is in the **Chapters** submenu. The move is reported on the screen with the chapter's number and title.

### Audio and subtitles

The button with a speech bubble left of the volume (`T`) opens a window with the audio tracks and the subtitles of the file side by side. Each entry names the language first and the track's title after a dash ("Russian — Superbit"); under it, badges show the format, the channels and whether the track is external, forced or the file's default. A long list scrolls. A choice applies at once and the window stays open, so both can be changed in one go; a click outside or `Esc` closes it. **Load from a file…** under the subtitles adds subtitles from a file of their own (SRT, ASS, SSA, VobSub, PGS, WebVTT); a subtitle file dropped onto the playing video is added the same way. Subtitles named like the video next to it are loaded by themselves. The same lists are in the picture's context menu, and every change is reported on the screen.

### File information

`I`, the **i** button in the title row or **File information** in the picture's context menu opens a window over the player; playback and the keys go on while it is open, and `I` or `Esc` closes it. At the top it shows the cover with the title, the artist and the album; a click on the cover shows it as big as the window allows. Below come the file (name, folder, size, container, duration, overall bitrate, number of chapters; a click on the folder shows the file in Explorer), its tags (the common ones first, named in the interface's language, then the others as the file names them) and its tracks, each with a line of its format, size, frame rate, bit depth, HDR, channels, sampling rate and bitrate. A click on a track unfolds its details: the codec's profile, the aspect ratio, the channel layout, the stream's estimated size, the track's flags and tags, and for the video on screen also the chroma subsampling, the scan, the color range, primaries, transfer and matrix, the HDR light levels and whether it is decoded in hardware. Two icon buttons under the header, named in their tooltips, act on it: **Copy all** (two sheets) puts the whole of it on the clipboard as text and shows a tick for a moment, and **Save the cover…** (an arrow into a tray) saves the picture as it is stored. The window stays at the player's top right corner, also when the player's window is resized; with **Pin the information window** turned off in the settings, it is moved by its title and stays where it was left. Its bottom edge can be dragged to make it taller or shorter.

The cover is taken from the file's tags (MP3, FLAC, OGG, Opus, M4A, MP4, WAV, APE and others) or from its attachments (MKV, MKA, WebM); for an audio file without one, a picture named `cover`, `folder` or `front` next to it is used. While an audio file plays, its cover fills the picture with the title, the artist and the album under it.

### File types

The **File types** page of the settings (`F5`) shows whether the player is registered, a row per action with its button on the right — **Register** (**Update** once registered), **Choose…** for the default program and **Unregister** — and the formats the player registers with their extensions and whether the player opens each by default. The installer registers the formats too.

![The icons of the registered file types](images/file_types.png)

The registration is per user and needs no administrator rights. Each format gets a type of its own with an icon (colours after GOM Player's: one hue per family of formats, Play on video, a note on audio) and a name in Explorer's Type column; the player appears in **Open with** for these files and in **Settings → Apps → Default apps**. Windows lets only the user choose the default program: **Choose…** opens that page of Settings, and Windows also offers the player the next time such a file is opened. A format no program opens yet gets the player as its default at once. **Unregister** removes everything the registration wrote; the choice of another program stays as it is.

Registered formats: video MP4 (`.mp4`, `.m4v`), MKV, WebM, AVI (`.avi`, `.divx`), MOV (`.mov`, `.qt`), WMV (`.wmv`, `.asf`), FLV (`.flv`, `.f4v`), MPEG (`.mpg`, `.mpeg`, `.m2v`, `.vob`), TS (`.ts`, `.m2ts`, `.mts`), 3GP (`.3gp`, `.3g2`), OGV and RM (`.rm`, `.rmvb`); audio MP3, FLAC, AAC (`.m4a`, `.aac`), Ogg (`.ogg`, `.oga`), Opus, WAV, WMA, APE, WavPack (`.wv`), MKA, AC3 and DTS.

The installer and other scripts can do the same from the command line: `metalmpv.exe --register` and `metalmpv.exe --unregister`.

### Settings

The settings window (`F5` or the gear button) lists the categories on the left and shows the settings of the chosen one on the right in groups: each row names a setting, explains it underneath and holds its control on the right. **Reset** under the list puts every setting back to its default.

| Category | Setting | Default | Meaning |
|---|---|---|---|
| General | Language | as in Windows | "System" follows the language of Windows |
| General | Pin the information window | on | The information window stays at the player's top right corner; off, it is moved by its title |
| General | Over the other windows | never | Never, always, or while a file plays (not paused); `Ctrl+T` switches it |
| General | Snap to the screen's centre | on | The window being moved stops with its centre on the screen's centre, across or up and down, when it comes within a few pixels of it; moved on, it comes away |
| Updates | Check at start-up | off | See [Update check and privacy](#update-check-and-privacy); **Check now** asks at once |
| General | Add the folder's files | off | One opened file brings the media files of its folder into the playlist |
| Playback | Go on from where it stopped | on | A file of 5 minutes or more opened again starts where it was left, with a message on the screen; `Home` goes to the beginning. A file watched to its last 30 seconds, or stopped in its first 10 seconds, starts from the beginning. The last 200 files are remembered; **Forget all** clears the list |
| Playback | Remember the tracks | on | A file opened again plays the audio track and the subtitles (or no subtitles) chosen in it last time, whatever its length; kept for files with more than one audio track or with subtitles |
| Playback | Like tracks in new files | on | A file opened for the first time plays the audio track and the subtitles most like those last chosen by hand: the same title first, else the same language; subtitles turned off by hand stay off. Meant for series: the voice-over chosen in one episode plays in the next. Nothing alike leaves the usual choice |
| Playback | Left / Right | 10 s | The step of `←` and `→`, on keyframes |
| Playback | Ctrl+Left / Right | 60 s | The step of `Ctrl+←` and `Ctrl+→`, on keyframes |
| Playback | Marks on the seek bar | on | Cuts in the seek bar where the chapters start; also in the context menu |
| Video | Hardware decoding (GPU) | on | Decoding on the graphics card where possible; the decoder in use is shown below |
| Video | File name at the top | on | A strip with the file's name across the top in fullscreen, shown while the pointer is near the top edge |
| Video | Panel transparency | 12 % | Transparency of the control bar and the file's name over the picture in fullscreen, 0–80 % |
| Sound | Device | Windows' default | The output device; while the chosen one is unplugged, the sound goes to Windows' default |
| Sound | Channels | Auto | Auto follows the device's setup in Windows; mono, stereo, 5.1 or 7.1 mixes the sound to that layout |
| Sound | To a receiver as it is | off | Dolby and DTS sound goes out compressed for the receiver to decode: over S/PDIF AC3 and DTS, over HDMI also E-AC3, DTS-HD and TrueHD; the player's volume, normalization and channels do not apply to it, and Windows gives the device to the player alone. A format the device does not take is decoded as usual |
| Sound | Volume boost | none | Lets the volume go up to 130, 150 or 200 %; loud passages may then distort |
| Sound | Normalization | off | Evens the volume out as the film plays: quiet dialogue louder, loud scenes quieter (FFmpeg's `dynaudnorm`) |
| Subtitles and OSD | Subtitles: font | default | A font installed in Windows, for text subtitles (SRT and the like); ASS subtitles keep their own fonts |
| Subtitles and OSD | Subtitles: size | 100 % | The size of all subtitles, 50–300 % |
| Subtitles and OSD | On-screen messages: font | default | A font installed in Windows, for the yellow line in the top left corner |
| Subtitles and OSD | On-screen messages: size | 100 % | 50–200 % of the usual size, which follows the height of the picture |

The list of fonts has a search field at its top. The **Keys** category lists the keyboard shortcuts for reference. The volume, the window and the playlist's visibility and width are remembered between runs.

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
| `T` | The audio and subtitles window |
| `I` | The file information window |
| `F7` | The Video panel |
| `Ctrl+E` | Save the frame into Pictures\MetalMPV |
| `O` | Shows the position, the duration and the chapter playing on the screen |
| `A` | Next audio track |
| `S`, `J` | Next subtitles; after the last, none |
| `V` | Subtitles off, and on again |
| `C` / `X` / `Z` | Faster / slower / normal speed (0.25× to 4×, the voice keeps its pitch; also the **Speed** submenu of the context menu) |
| `Ctrl+=` / `Ctrl+-` | Sound later / earlier by 100 ms against the picture |
| `Ctrl+0` | Sound back in sync |
| `Shift+.` / `Shift+,` | Subtitles later / earlier by 100 ms (`>` and `<`) |
| `Shift+0` | Subtitles back in sync |
| `Page Up` / `Page Down` | Previous / next file of the playlist |
| Media keys | Play/Pause, Stop, Previous and Next of a keyboard, a headset or a remote, while the player's window is active |
| `Enter`, `F11`, double click | Fullscreen |
| `1` / `2` / `3` / `4` | Window for the picture at 50 % / 100 % / 150 % / 200 % of the video's size |
| `5` | The largest window the screen holds, without black bars |
| `` ` `` (`Ё`) | Window fitted to the picture, without black bars |
| `6` | Fullscreen without black bars (the picture cropped); again, with them |
| `Esc` | Closes the file information window, else leaves fullscreen, else closes the program |
| `Ctrl+O` | Open files |
| `Ctrl+U` | Open an address: a direct link to a video or a stream |
| `L`, middle click on the picture | Show or hide the playlist |
| `R` | Repeat: off, the file, the list |
| `Ctrl+T` | Over the other windows: never, always, while playing (also in the settings and the context menu) |
| `Delete` | Remove the selected playlist entry |
| `F5` | Settings |
| `F1` | About |

A seek on keyframes is instant, since nothing has to be decoded before the frame shown; it lands on the keyframe nearest to the target, and the on-screen line reports where. An exact seek decodes from the previous keyframe up to the target frame.

## Update check and privacy

The update check is optional and off by default. With **Check at start-up** enabled on the Updates page of the settings, the program asks the GitHub Releases API (`api.github.com`) for the latest release at most once a day; **Check now** on the same page asks at once. The request carries nothing but the program version in its `User-Agent` header, and nothing is downloaded or installed: when a newer release exists, a button with its version appears in the title row and opens the release page in the browser. Otherwise the program does not access the network; libmpv's network features (streaming, `youtube-dl`) are not used.

## Building

Requirements: Windows 10 or later, a stable Rust toolchain (MSVC target, 2024 edition), the Visual Studio 2022 Build Tools with the C++ workload and Python 3. For `build.py`, [uv](https://docs.astral.sh/uv/).

```
uv run build.py          # target\release\metalmpv.exe with libmpv-2.dll next to it
cargo test
```

`build.py` closes a `metalmpv.exe` running from the project's `target` folder, because it would lock the file, fetches libmpv when it is missing (`tools/get_libmpv.py`) and runs `cargo build --release`. `tools/get_libmpv.py` downloads a pinned build of libmpv from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake) and the standalone `7zr.exe` of 7-Zip to unpack it, checks both against their SHA-256 and puts them into `vendor` (not in the repository); `build.rs` copies the DLL next to the executable. The executable links the C runtime statically (`.cargo/config.toml`) and loads libmpv at run time, so it needs nothing beyond Windows and `libmpv-2.dll`.

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

The script writes the files to `dist` and takes the version from `Cargo.toml`. The installer script is `tools/setup.iss`; `metalmpv.exe --export-icon <file.ico>` writes the installer icon.

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
- [Lofty](https://github.com/Serial-ATA/lofty-rs): the covers stored in tags;
- [image](https://github.com/image-rs/image): decoding the covers;
- [fontdb](https://github.com/RazrFalcon/fontdb): the list of the system's fonts;
- [7-Zip](https://www.7-zip.org): unpacking libmpv during the build;
- [Inno Setup](https://jrsoftware.org/isinfo.php): the installer.

## License

MIT, see [LICENSE](LICENSE). The included `libmpv-2.dll` is licensed under the GNU GPL version 3, so the program as distributed with it is subject to the terms of that license; the sources and details are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). The licences and copyright notices of the Rust libraries and fonts compiled into the program are in [licenses/THIRD_PARTY_NOTICES.txt](licenses/THIRD_PARTY_NOTICES.txt). The program shows all of these texts in its About window (**Licenses…**), and the installer and the portable archive include them as files.
