<p align="center">
  <img src="images/icon.png" width="128" height="128" alt="MetalMPV icon">
</p>

<h1 align="center">MetalMPV</h1>

<p align="center">A fast video player for Windows built on libmpv</p>

<p align="center">
  <a href="https://github.com/Fan4Metal/metalmpv/releases/latest"><img src="https://img.shields.io/github/v/release/Fan4Metal/metalmpv?label=release" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/Windows-10%2B-0078D6" alt="Windows 10 or later">
  <img src="https://img.shields.io/badge/Rust-B7410E?logo=rust&logoColor=white" alt="Rust">
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2B%20GPL--3.0-blue" alt="MIT license, GPL-3.0 with libmpv"></a>
</p>

<p align="center"><b>English</b> | <a href="README.ru.md">Русский</a></p>

MetalMPV is a video player for Windows with a classic layout: the picture fills the window, the seek bar and the buttons sit under it with Play in the middle, and the playlist opens on the right. Playback is done by [libmpv](https://mpv.io), the library of the mpv player, so practically every video and audio format plays, with hardware decoding where the graphics card supports it. The program is built for opening files quickly and for instant seeking on keyframes.

![MetalMPV playing a video, with the control bar under the picture](images/screenshot.png)

*Screenshot of version 0.1.0.*

## Features

- **Fast start**: the playback engine starts while the window is being created; on a warm start the first frame of a 1080p file appears about 0.4 s after the program is launched.
- **Instant seeking on keyframes** with the arrow keys, exact seeking and frame stepping on other keys (see [Keyboard shortcuts](#keyboard-shortcuts)); every seek is reported on the screen.
- **Every common format**: MP4, MKV, AVI, MOV, WebM, TS, WMV and more, audio files included; subtitles next to the video are loaded automatically.
- **Audio tracks and subtitles** in one window by the volume (`T`), as tabs or both lists side by side; subtitles from a file of their own by **Load from a file…** or by dropping the file onto the playing video (see [Audio and subtitles](#audio-and-subtitles)).
- **File information** (`I`): the cover, the tags, the size, the container, the details of every track and how the playback goes in a window that stays over the player while it plays; the cover of an audio file fills the picture (see [File information](#file-information)).
- **Window for the picture**: 50 %, 100 %, 150 % or 200 % of the video's size, the largest window without black bars, the window fitted to the picture, or fullscreen without black bars (`1`–`6`, `` ` ``).
- **Chapters**: marks on the seek bar, which can be turned off, and a list under the playlist to go to a chapter (see [Chapters](#chapters)).
- **A-B repeat, bookmarks, going to a time**: a stretch played over and over and saved as a file of its own (see [A-B repeat](#a-b-repeat)), places in files kept in a menu (see [Bookmarks](#bookmarks)), and a time typed after `Ctrl+G` (see [Go to time](#go-to-time)).
- **Hardware decoding** (NVDEC, D3D11VA and others, chosen by libmpv), which can be turned off in the settings.
- **Sound settings**: the output device, the channels (automatic or a layout chosen by hand), a volume boost up to 200 %, normalization that evens the volume out, clearer speech, stereo spread over 5.1, the subwoofer's level and a cut of the infrasound, an equalizer, and Dolby and DTS sound sent to a receiver undecoded; the sound can be moved against the picture with `Ctrl+=` and `Ctrl+-`.
- **Video, sound and subtitles panel** (`U`): brightness, contrast, saturation, hue and gamma, zoom and pan, sharpness, noise reduction and debanding, a forced aspect ratio, deinterlacing, HDR tone mapping, rotation and flips, saving the frame; the output device, channels, boost, delay, normalization, clearer speech and a ten-band equalizer with presets of one's own; the look of the subtitles with looks of one's own, a preview, their delay, the black bars and ASS (see [Video, sound and subtitles panel](#video-sound-and-subtitles-panel)).
- **Resume**: a film opened again goes on from where it was stopped (see [Settings](#settings)).
- **Playlist**: files opened together, dropped onto the window or added with the **+** button of the playlist; optionally the other media files of the folder of an opened file; a search, IPTV lists under their groups with the channels' logos, and lines of how far each file was watched.
- **One window**: a file opened from Explorer while the player runs is played in the window already open.
- **File types**: the player can be registered with Windows for its video and audio formats, each with an icon of its own, and chosen as the default player (see [File types](#file-types)).
- **English or Russian interface**: it follows the language of Windows and can be changed in the settings (`F5`).
- **Portable**: the [portable archive](#installation) keeps its settings next to the program.
- **Privacy**: no network access beyond the addresses opened, the optional update check and the logos of IPTV channels, which can be turned off (see [Update check and privacy](#update-check-and-privacy)).

## Installation

The installer and the portable archive are on the [Releases](https://github.com/Fan4Metal/metalmpv/releases) page.

The installer, `MetalMPV_<version>_Setup.exe`, needs no administrator rights: the program is placed in `%LOCALAPPDATA%\Programs\MetalMPV`. The installed program keeps its settings in `%APPDATA%\MetalMPV`. Its **File types** task, on by default, registers the player for its formats (see [File types](#file-types)); on the last page it offers to open Windows Settings to choose the player as the default. Uninstalling removes the registration; the settings in `%APPDATA%\MetalMPV` (with the remembered positions and the channel logos) are kept.

The portable archive, `MetalMPV_<version>_portable.zip`, holds the program in a `MetalMPV` folder and runs without installation. It also holds an empty `metalmpv.ron`. While that file is next to `metalmpv.exe` and the folder is writable, the settings are kept in it, so the program can be carried on a removable drive. Without the file, the program keeps them in `%APPDATA%\MetalMPV`, as the installed program does. The About window shows where the settings are kept. Beside the settings file lies a copy of it made at each start (`app.ron.bak` or `metalmpv.ron.bak`); a settings file found damaged, for instance by a power cut while it was being written, is replaced by that copy.

Both contain `libmpv-2.dll` (about 115 MB unpacked), the playback engine, which must stay next to `metalmpv.exe`.

The executables are not code-signed, so Windows SmartScreen may warn about an unknown publisher on first launch.

## Usage

A file is opened with the **Open** button (the eject symbol, `Ctrl+O`), by dropping it onto the window, from Explorer (**Open with**), or from the command line: `metalmpv.exe file.mkv [more files]`. Several files make a playlist; at the end of a file the next one starts. Files dropped onto the window or the playlist are added to the end of the playlist, and play at once only when the playlist was empty or nothing plays (a file the playlist has already plays then from its place; added out of sight, the on-screen message says how many); a subtitle file dropped with a video while nothing plays goes with that video. An M3U or M3U8 playlist opened, dropped or given on the command line is replaced by its entries (relative paths are from the playlist's folder, `file:///` addresses are taken as paths, and an `.m3u` file not in UTF-8 is read in the system's code page, as older players wrote it; an HLS stream's playlist, with `#EXT-X-` tags, is played as one stream instead); so is a playlist at an address (`http://…/list.m3u`) and one listed inside another, once mpv has read it. An IPTV list's `#EXTINF` lines name the entries: the playlist, the window title and saved frames show the channel's name instead of the stream's file name, the tooltip of an entry shows its group, and the channel's logo (`tvg-logo`) is shown beside the name (see **Channel logos** in the settings); **Save the list** writes the names, logos and groups back. `Ctrl+U` or **Open an address…** in the context menu plays a direct link to a video or a stream (http, https, rtsp, rtmp, udp and others; an address on the command line works too); pages of YouTube and other sites are not supported, since the player does not use yt-dlp. While nothing plays, the start screen lists the files played last under the RECENT heading, centred under the hint (up to eight, as many as the window's height allows; the rest of the thirty kept scroll in their place); a click plays one, and **Clear the list** empties it, as does **Forget all** under Resume in the settings. With **Remember the recent files** off, no files are kept. The taskbar button shows the position as its progress (yellow while paused), and its thumbnail has the buttons **Previous file**, **Play**/**Pause** and **Next file**. While a video plays, the display does not turn off and the computer does not sleep; while only sound plays, the computer does not sleep, and when playback pauses or ends, Windows' power settings apply again.

The window has its own title row: it is moved by dragging the title, the picture or the free space of the control bar, resized by its edges, and maximised by a double click on the title. A double click on the picture or `Enter` switches to fullscreen, where only the picture is left; the control bar at the bottom and the file's name at the top appear over it independently, each while the pointer is near its edge, and the pointer itself hides after half a second of rest away from them. The strip with the name can be turned off, and the transparency of both is set in the settings. `L` or a middle click shows the playlist over the right side of the screen, as tall as it and resized by its left edge; `Esc` or the same key hides it, and leaving fullscreen does so too.

`1`, `2`, `3` and `4` size the window so that the picture is shown at 50 %, 100 %, 150 % or 200 % of the video's size (at 100 %, a pixel of the video on a pixel of the screen), `5` makes the largest window the screen holds with the picture's shape, so that there are no black bars, and `` ` `` (the key left of `1`) fits the window to the picture shown, removing the black bars. A maximised window is restored first and fullscreen is left; a window that would not fit the screen is made smaller, the picture keeping its shape, and keeps its centre where possible. `6` switches to fullscreen with the picture enlarged to fill the screen, its edges cropped, so that there are no black bars; `6` again shows the picture whole with the black bars, still in fullscreen, and leaving fullscreen does so too. The same commands are in the **Zoom** submenu of the picture's context menu. The interface has four layouts, chosen in the **Interface** submenu of the context menu and switched in turn by `8`: **Full** (the title row and the control bar of two rows), **Without the title row**, **Compact** (no title row, the control bar in one row: Play, previous and next, the time and the seek bar from the left, the volume and the other buttons on the right; Open and Stop remain in the context menu and on the keys, and in a narrow window the less needed buttons are left out so that the seek bar keeps its room) and **Only the picture**. Without the title row, the title row with the window's buttons appears over the top of the picture while the pointer is there. The layout is kept between starts (also chosen on the General page of the settings); in fullscreen the control bar is compact in the compact layout. On a change of layout the window grows or shrinks by the difference, so the picture keeps its place and size; a maximised window or one in fullscreen keeps its size and is fitted once restored. `7` (or **Only the picture**) hides the title row, the control bar and the playlist, leaving only the picture; the title row appears over its top and the control bar of the layout chosen over its bottom while the pointer is near the edge, and the window is still moved by dragging the picture and resized by its edges. `7` again, `Esc` or **Show the interface** at the top of the context menu brings back the layout shown before (the full one after a start with only the picture); so does asking for the playlist (`L`, the middle click, the context menu), which then shows it.

The control bar holds the seek bar (a click or a drag moves there; the time under the pointer is shown), the time (a click on it switches between the position with the duration, the position alone and the time remaining; the choice is kept), the buttons **Open**, **Stop**, **Previous file**, **Play/Pause**, **Next file**, **Settings** and **Playlist**, the Video, sound and subtitles panel and the tracks buttons, the sound on/off button with the volume, and the fullscreen button. A right click on the picture opens a menu with the same commands.

The playlist (`L`) lists the files with the current one highlighted, and scrolls to the current one when it changes. A click selects an entry, `Ctrl`+click adds one to the selection or takes it out, `Shift`+click selects a range of the entries shown (a search or a folded group leaves the others out of the selection, as does **Select all**); a double click plays an entry. The selected entries are moved by dragging them (a line shows where they go; not while searching or in groups). The right button opens an entry's menu: **Play**, **Show in its folder**, **Copy the path** (the paths of all the selected entries), **Select all** and **Remove from the list**. Under an entry a thin line shows how far it was watched, and a tick at the right marks a file watched to its end (for files of five minutes or more, while **Go on from where it stopped** is on); **Show how far files went** in an entry's menu turns the lines off and on (the ticks stay), and the choice is kept. The entries of an IPTV list with groups (`group-title`) are shown under their groups, which fold by a click on their headers; a group's menu folds or unfolds them all, and **Show the groups** in the menu of an entry or of a group turns them off and on, and the choice is kept. The magnifier at the right of the buttons, or `Ctrl+F`, shows a search field: the list then shows the entries whose names contain the text typed, `Enter` plays the first one, and `Esc` in the field closes it; outside the field `Esc` does what it always does. The icon buttons above the list add files (**+**), remove the selected entries (**−**, `Delete`), sort the list by name, numbers by value (the bars with an arrow), clear it (the bin), save it as an M3U8 playlist (the arrow into a tray), and after a line, shuffle it (crossing arrows; the current file plays on, first in the list); their names are shown in the tooltips. The loop button next to it, `R` or the **Repeat** submenu of the context menu switches the repeat mode, which is kept: off, the file (played again without a gap) or the list (the first file after the last; **Next** and **Previous** go round).

The button at the right end of the playlist's buttons (a window with an arrow), or **Playlist in a window of its own** in the picture's context menu, moves the list into a window of its own and back. That window is frameless as the player's, with a title row of its own (dragged to move the window, a close button on the right) and edges that resize it; it has no taskbar button, stays above the player and is minimised with it; the first time it stands at the player's right edge, as tall as the player. Moved near an edge of the player, it is drawn to it and lined up with the player's top, bottom or side; once it touches an edge, it goes along when the player is moved or resized (not while the player is maximised or in fullscreen). In fullscreen it is hidden while it would cover the player's screen. The player's keys work in it, except `Esc`, which closes it. Its place and size are remembered.

### Video, sound and subtitles panel

`U` (also `F7`), the sliders button left of the tracks button in the control bar, or **Video, sound and subtitles…** in the picture's context menu opens a panel over that button, as the tracks button does (at the bottom right corner when only the picture is shown), as tall as its tallest tab, whichever is shown; in a window too low for it, it takes the height there is above the button and its tabs scroll under the head. Playback and the keys go on while it is open; a click outside it, the button again, `U`, `F7` or `Esc` closes it. Its head holds the tabs **Video**, **Sound** and **Subtitles**, the tab shown underlined (in the tabs, a click on a section's title folds the section or unfolds it, and the panel keeps that between runs), and the camera, **Save the frame** (`Ctrl+E`, also in the context menu), which saves the frame on screen as decoded, without subtitles, as a PNG or a JPG (see Video in the settings) named after the file and the position into `Pictures\MetalMPV` or the folder chosen; a right click on it offers **Open the folder**.

Values are set by sliders whose rail is lit from the starting point to the knob, so that a change shows at once; the value beside each can also be dragged or typed and is shown in the accent colour while it differs from the default, which a double click on the slider or the round arrow beside it brings back. On and off are switches. Each setting's tooltip says how it applies: to which subtitles, on the processor or the graphics card, whether it is kept between runs.

On the Video tab, sliders set the brightness, the contrast, the saturation, the hue and the gamma from −100 to 100; the zoom, from 50 to 400 %, and under it the navigator: the picture in small with the part on screen framed; the frame is dragged, or put where a click lands, to pan the picture (up to ±50 % of it either way; the figures beside **Pan** take it typed, a double click on the navigator centres it), and **Navigator on the picture** shows the same in the picture's bottom right corner while it is zoomed in past the window (above where the control bar comes over the picture); the sharpness (AMD's contrast adaptive sharpening) and the noise reduction (from 0, off, to 100 %; 40 % is FFmpeg's usual strength, more blurs). Gradient smoothing removes the steps in skies and dark scenes. The aspect ratio is the file's or one forced on it: 4:3, 16:9, 16:10, 1.85:1, 2.35:1 or 2.39:1 (also in the **Aspect ratio** submenu of the picture's context menu). Deinterlacing weaves interlaced pictures, as in TV recordings, into whole frames: automatically for frames marked interlaced (the default), always or never. **HDR on SDR** chooses how an HDR picture is brought down to an ordinary screen: automatically, BT.2390 (the standard's), Hable (filmic), Möbius (bright), Reinhard (soft) or clipped. A row of four icon buttons, named in their tooltips, turns the picture left or right by 90° and flips it upside down or mirrors it (the flips are lit while on). The sharpness, the noise reduction and the flips are FFmpeg filters run by the processor (while any of them is on, hardware decoding copies each frame back to memory for them); the rest is done by the graphics card. The adjustments apply to every file until the program is closed, and between runs with **Remember** on; **Reset all** puts them back.

The Sound tab (also opened by the Equalizer action, which has no default key) starts with what is playing: the track's channels drawn as speakers round the listener (and, when the sound is mixed to other channels for the device, an arrow to the device's), its format as the decoder names it (E-AC3 Atmos, DTS-HD MA and the like; the codec's full name in the tooltip), its channels, sampling rate and bitrate of the moment, and what the device receives (the rate, the channels and the sample format, or the bitstream passed to a receiver). Then it holds the output device (while the chosen one is unplugged, the sound goes to Windows' default); the channels (Auto follows the device's setup in Windows; mono, stereo, 5.1 or 7.1 mixes the sound to that layout); **Receiver**, Dolby and DTS sound sent out compressed for a receiver to decode (over S/PDIF AC3 and DTS, over HDMI also E-AC3, DTS-HD and TrueHD; the player's volume, normalization, equalizer and channels do not apply to it, Windows gives the device to the player alone, and a format the device does not take is decoded as usual); the volume's boost (up to 130, 150 or 200 %; loud passages may then distort); the sound's delay against the picture (a slider of ±5 s; the keys go further, and it is not kept between runs); the normalization (FFmpeg's `dynaudnorm`: quiet dialogue louder, loud scenes quieter; its **Strength** soft, medium or strong raises quiet passages up to 3, 5 or 10 times, the strong following the quickest); and **Clearer speech**, which raises the voice over the music: in a stereo track through FFmpeg's dialogue enhancer, in a 5.1 or larger one by raising the centre channel, a mono one as it is; its **Strength**, +1 to +20 dB (+4 by default), is how much louder the voice is than the rest: the centre is raised up to 12 dB, past that the other channels are lowered instead, so that a loud voice does not distort; in stereo the enhancer works as strongly first (up to its limit at 12 dB); and **Stereo to 5.1**, which spreads a stereo track over 5.1 for speakers that take it (FFmpeg's `surround`: what both sides share, mostly the voice, goes to the centre, what differs to the surrounds, the low end to the subwoofer; Clearer speech then raises that centre), not while Channels makes the sound mono or stereo. A function's settings show only while it is on. Under it, **Subwoofer up to** sets where the subwoofer's sound ends (40–200 Hz, 80 by default, the cinemas' crossover; it fades out up to twice that), and **Bass to the subwoofer only** takes the low end out of the other speakers instead of only copying it to the subwoofer, for small front speakers. The **Subwoofer** section holds its **Level** in dB (−20 to +10): for a 5.1 or 7.1 track against the track's own level, for Stereo to 5.1 against a 5.1's standard, 10 dB under the other channels, as receivers play that channel 10 dB louder; and **Cut the infrasound**, which takes out what lies under the frequency set (15–40 Hz, 25 by default; 24 dB an octave) in all channels: a subwoofer hardly plays it, but its cone is driven to the limit and the bass bubbles, and films with deep bass hold much of it. Below them is the ten-band equalizer, 31 Hz to 16 kHz, ±12 dB a band (a double click or the round arrow under a band puts it back to 0), with a scale of ±12 dB, the 0 dB line and a curve through the bands; its switch turns it on, and the list beside it holds the presets Flat, Rock, Pop, Jazz, Classical, Bass, Voice and Treble. The plus beside the list saves the gains under a name typed in the row that appears (Enter saves, Esc gives up; a name already there is replaced); such gains follow the presets in the list under "Mine", at most twenty, and the bin deletes the ones chosen. All but the delay are kept between runs; the equalizer, the normalization and clearer speech do not apply to sound sent to a receiver undecoded.

The Subtitles tab starts with ready-made looks: soft with a shadow (the default), classic as in mpv, thin outline, bold with a thick outline, shadow only, cream, pale mint, on a dark box, and dark on a light box; each sets the colours, the outline, the shadow, boldness and the box, and leaves the font, the size and the height as they are (the list says "Custom" once anything of it is changed). The plus beside the list saves the whole look, the font, the size and the height included, under a name in the same way; such looks follow the ready-made ones under "Mine", at most twenty, and the bin deletes the one chosen. They are kept between runs, and Reset in the settings keeps them. Under the list, large sample text shows the look roughly, in the interface's font and at a fixed size, on a ground from dark to light. Below, the tab sets the font and the size (of all subtitles, ASS included), bold letters, the colour of the letters and of their outline, the outline's thickness, the shadow, a slightly see-through box behind each line in place of the outline (of the outline's colour, the thickness becoming its margin), the height of the subtitles on the picture, **In the black bars** (on by default: where the picture has black bars, subtitles, ASS ones too, go in them; off, over the picture), **Style for ASS too** (the font, the colours and the outline replace those of ASS subtitles as well, whose signs and effects then lose their look) and their delay (a slider of ±5 s, not kept between runs). Under the delay a timeline shows a stretch of time round the position, the vertical line in the middle, with the lines of a subtitle file of its own (SRT, WebVTT, ASS) as blocks moved by the delay. Dragging the blocks sets the delay, and a line's start stops for a moment on the position line; so a pause where a line is spoken and its block put on the line match the subtitles to the speech. The arrow buttons at its ends put the line before or after the one on screen at the position (`Ctrl+Shift+←` and `Ctrl+Shift+→`, for any subtitles), and − and + widen or narrow the stretch, from 10 s to 5 min 20 s. **Reset all** puts the look back. Otherwise the size and the height apply to all subtitles, the font and the colours to text subtitles (SRT and the like). All but the delay are kept between runs.

### Chapters

When the file has chapters, the seek bar is cut at the start of each, and the tooltip over the bar names the chapter under the pointer. The marks are turned off and on in the **Chapters** submenu of the picture's context menu or in the settings (**Playback**). Under the playlist, the **Chapters** section lists them with their starts, the playing one highlighted; a click goes to a chapter, and a click on the header folds the section. The same list is in the **Chapters** submenu. The move is reported on the screen with the chapter's number and title.

### Go to time

`Ctrl+G` or **Go to time…** in the picture's context menu opens a window with the position playing in its field, selected, so that a time typed replaces it. The field takes `1:23:45`, `83:20`, a number of seconds (`5000`), a fraction after a point or a comma (`1:23,5`), a share of the file (`50%`), or a move from the position playing (`+30`, `-1:00`, `+10%`); under the field the window shows the time this names, with its share of the file, or says that the text is not a time in this file. `Enter` or **Go** moves there to the exact frame; a time past the end goes to the end.

### Bookmarks

`Ctrl+B` or **Bookmarks → Add a bookmark here** in the picture's context menu keeps the file playing and the position. The **Bookmarks** submenu lists them, the latest first, each by the file's name and the time with a thin line under it showing where in the file it is, those of the file playing marked: a click goes there, in the file playing at once, else playing its file from that place (its playlist entry, or the file opened); the cross beside one deletes it, and **Delete all bookmarks** deletes them all. Up to 30 are kept between runs, the oldest giving way; Reset in the settings keeps them.

### A-B repeat

`[` marks the start of a stretch at the position playing and `]` its end; from then on, playback goes back to the start each time it reaches the end, until `\` turns the repeat off. An end marked without a start repeats from the beginning of the file, and an end before the start swaps the two. The points appear as yellow ticks on the seek bar with the stretch between them highlighted, the on-screen message gives them to a tenth of a second, and the **A-B repeat** submenu of the picture's context menu does the same. The points belong to the file: another file starts without them.

`Ctrl+Shift+S` or **Save the stretch…** in the same submenu saves the stretch between the points as a file of its own, while playback goes on. The window names the file (an MP4 next to the source, or in the folder of saved frames for an address, with the times in its name; **…** chooses another; a name typed without a folder goes to that folder, and one without `.mp4`, `.mkv` or `.mov` gets `.mp4`; the file playing and a file already there are not written over, except one the **…** dialog was told to replace) and how it is encoded: H.264 or HEVC, on the processor (x264, x265) or an NVIDIA card (NVENC), the quality (22 at first; lower is better and larger), and, if wanted, the subtitles shown burnt into the picture in their look; the sound is the audio track playing, in AAC; the audio and subtitle delays and the deinterlacing are those of the player. The stretch is cut exactly at the points. A progress bar, **Cancel** and **Hide** follow: hidden, the window lets the player be used while the saving goes on, and `Ctrl+Shift+S` shows it again; the on-screen message names the file saved, or says why it was not (NVENC needs an NVIDIA card). The choices are kept.

### Audio and subtitles

The button with a speech bubble left of the volume (`T`) opens a window with the audio tracks and the subtitles of the file as two tabs; the button at the right of the tabs shows them side by side in two columns instead, and the same button in that view turns back to tabs (the choice is kept between runs). Each entry names the language first and the track's title after a dash ("Russian — Superbit"); under it, badges show the format, the channels and whether the track is external, forced or the file's default. A long list scrolls. A choice applies at once and the window stays open, so both can be changed in one go; as with the Video, sound and subtitles panel, which it replaces at that place, playback and the keys go on while it is open, and a click outside, the button again, `T` or `Esc` closes it. **Load from a file…** under the subtitles adds subtitles from a file of their own (SRT, ASS, SSA, VobSub, PGS, WebVTT); a subtitle file dropped onto the playing video is added the same way. Subtitles named like the video next to it are loaded by themselves. The same lists are in the picture's context menu, and every change is reported on the screen.

### File information

`I`, the **i** button in the title row or **File information** in the picture's context menu opens a window over the player; playback and the keys go on while it is open, and `I` or `Esc` closes it. At the top it shows the cover with the title, the artist and the album; a click on the cover shows it as big as the window allows. Under them the main facts stand in rows of chips, so that nothing needs scrolling for: the file (format, size, duration, overall bitrate), the picture (codec, size, aspect ratio, frame rate, bit depth, HDR, bitrate) and the sound, a chip per audio track (codec, channels, language) with the playing one outlined; a click on an audio chip plays that track, and with more than four, the others are folded into one chip that lists them in its tooltip. The subtitles' chip gives their language and count. Each chip's tooltip says more: the container's name, the codec's full name and profile, the track's name and details. The chips are coloured: the format in the colours of its file icon, the picture, the sound and the subtitles each in a hue of their own. A channel of an IPTV list is shown by the list's name and group, with its logo in place of the cover once fetched. Below come sections that fold with a click on their heading and are copied as text from its context menu. The file: name, folder, size, container, duration, overall bitrate, number of chapters, the program that wrote a Matroska file and when, and the file's dates; a click on the folder shows the file in Explorer. The playback, refreshed every second: the bitrates of the moment with a line of the last minute's, the frame rate shown against the screen's, the dropped frames, what the sound device receives (the format passed through, if any) and, for a stream, the data read ahead. The tags: the common ones first, named in the interface's language, then the others as the file names them; a long one is cut short, with **more** to show it all. The tracks, grouped as video, audio and subtitles, the playing one marked with a dot, each with a line of its format, size, frame rate, bit depth, HDR, channels, sampling rate and bitrate; a sound track's format is named precisely once the decoder reports it (DTS-HD MA, DTS:X, E-AC3 Atmos, TrueHD Atmos, HE-AAC). A click on a track unfolds its details, and the video on screen is unfolded from the start: the codec's profile and level, the aspect ratio, the channel layout, the bitrate and the stream's size (from the statistics mkvmerge writes into a Matroska file, else estimated), the number of frames, the track's flags and tags, and for the video on screen also the chroma subsampling, the scan, the colour range, primaries, transfer and matrix, the HDR format and light levels with the mastering display, and whether it is decoded in hardware. Every value can be selected, and its context menu copies it or its row. Two icon buttons at the right end of the title's line, named in their tooltips, act on it: **Copy all** (two sheets) puts the whole of it on the clipboard as text, the values in a column, and shows a tick for a moment, and **Save the cover…** (an arrow into a tray) saves the picture as it is stored. The window stays at the player's top right corner, also when the player's window is resized. Its bottom edge can be dragged to make it taller or shorter.

The cover is taken from the file's tags (MP3, FLAC, OGG, Opus, M4A, MP4, WAV, APE and others) or from its attachments (MKV, MKA, WebM); for an audio file without one, a picture named `cover`, `folder` or `front` next to it is used. While an audio file plays, its cover fills the picture with the title, the artist and the album under it.

### File types

The **File types** page of the settings (`F5`) shows whether the player is registered, a row per action with its button on the right — **Register** (**Update** once registered), **Choose…** for the default program and **Unregister** — and the formats the player registers with their extensions and whether the player opens each by default: the extensions it opens are bright, and a format it opens only some extensions of shows how many. **Choose…** is available once the player is registered. The installer registers the formats too.

![The icons of the registered file types](images/file_types.png)

The registration is per user and needs no administrator rights. Each format gets a type of its own with an icon (one hue per family of formats, Play on video, a note on audio, a list on playlists) and a name in Explorer's Type column; the player appears in **Open with** for these files and in **Settings → Apps → Default apps**. Windows lets only the user choose the default program: **Choose…** opens that page of Settings, and Windows also offers the player the next time such a file is opened. A format no program opens yet gets the player as its default at once. **Unregister** removes everything the registration wrote; the choice of another program stays as it is.

Registered formats: video MP4 (`.mp4`, `.m4v`), MKV, WebM, AVI (`.avi`, `.divx`), MOV (`.mov`, `.qt`), WMV (`.wmv`, `.asf`), FLV (`.flv`, `.f4v`), MPEG (`.mpg`, `.mpeg`, `.m2v`, `.vob`), TS (`.ts`, `.m2ts`, `.mts`), 3GP (`.3gp`, `.3g2`), OGV and RM (`.rm`, `.rmvb`); audio MP3, FLAC, AAC (`.m4a`, `.aac`), Ogg (`.ogg`, `.oga`), Opus, WAV, WMA, APE, WavPack (`.wv`), MKA, AC3 and DTS; playlists M3U (`.m3u`, `.m3u8`).

The installer and other scripts can do the same from the command line: `metalmpv.exe --register` and `metalmpv.exe --unregister`.

### Settings

The settings window (`F5` or the gear button) lists the categories on the left and shows the settings of the chosen one on the right in groups: each row names a setting, explains it underneath and holds its control on the right; the round arrow at the right of a slider puts its value back to the default. **Reset** under the list puts the settings of the window back to their defaults (those of the Video, sound and subtitles panel are reset on the panel); the remembered positions, the recent files, the repeat mode and the playlist's place are kept.

| Category | Setting | Default | Meaning |
|---|---|---|---|
| General | Language | as in Windows | "As in Windows" follows the language of Windows |
| General | Accent colour | blue | Blue, teal, green, orange, red, pink, purple, graphite, or a colour of one's own (the last swatch, a colour wheel: a picker with a field for the code, shown as `#RRGGBB`, `RGB(r, g, b)` or `HSL(h, s%, l%)` by a click on its name and taking any of them, and fields for red, green and blue, 0–255): the Play button, the seek bar and the volume, the switches, the playing file of the playlist and the selection, and the screen of the icon in the window, on the taskbar button of the running player and on the start screen (the icon of the program's file and of its shortcuts stays blue) |
| General | Layout | full | Full, without the title row, compact or only the picture; `8` switches the layouts, `7` leaves only the picture and brings the layout back |
| General | Over the other windows | never | Never, always, or while a file plays (not paused); `Ctrl+T` switches it |
| General | Window when a file opens | keeps its size | Keeps its size, fitted to the picture (the black bars taken off, the width kept, as `` ` `` does) or 100 % of the video (a pixel of the video on a pixel of the screen, made smaller if the screen is, as `2` does); not while maximised or in fullscreen, nor for sound alone |
| General | Snap to the screen's centre | on | While the window is moved, its centre is drawn to the screen's centre, across or up and down, when it comes within 8 pixels of it; moved on, it comes away |
| General | Add the folder's files | off | One opened file brings the media files of its folder into the playlist |
| General | Channel logos | on | The pictures an IPTV list gives its channels are fetched from their sites for the visible rows of the playlist, only over HTTPS and never from the local network, at most 512 KB each, PNG or JPEG only, and kept for 30 days in the `logos` folder next to the settings |
| General | On-screen messages: show messages | on | The line in the top left corner after a seek, a volume change and the like; off, only errors, files saved and the line saying where the data of an incomplete file ends are shown |
| General | On-screen messages: font | default | A font installed in Windows, for the line in the top left corner |
| General | On-screen messages: size | 100 % | 50–200 % of the usual size, which follows the height of the picture |
| General | On-screen messages: shown for | 2 s | How long a message stays on the screen (0.5–10 s); it fades out in its last 0.4 s |
| General | On-screen messages: look | yellow letters, thin black outline, shadow | As for the subtitles: the colour of the letters and of the outline, the outline's thickness (0–4), the shadow (0–4), bold letters, and a slightly see-through box of the outline's colour in place of the outline, the thickness then being its margin; **Default** brings the yellow line back; a sample line under them shows the font, the size and the look |
| Playback | Go on from where it stopped | on | A file of 5 minutes or more opened again starts where it was left, with a message on the screen; `Home` goes to the beginning. A file watched to its last 30 seconds, or stopped in its first 10 seconds, starts from the beginning. The last 500 files are remembered; **Forget all** clears the list |
| Playback | Remember the recent files | on | The start screen lists the last thirty files played, those that do not fit scrolling in the list; when off, none are kept and the list is cleared |
| Playback | Left / Right | 10 s | The step of `←` and `→`, on keyframes |
| Playback | Ctrl+Left / Right | 60 s | The step of `Ctrl+←` and `Ctrl+→`, on keyframes |
| Playback | Marks on the seek bar | on | Cuts in the seek bar where the chapters start; also in the context menu |
| Playback | Stay in the file | on | A file whose download has not finished shows its whole length, but its data ends earlier. Where the data ends, whether reached by playing or by a seek past it, playback stops, paused, with a message on the screen that stays until playback goes on; Play then goes on from where there was data (the place before the seek, or the end of the data, which may have grown meanwhile). When off, the file ends there and the next one in the playlist plays. An end within the last 15 seconds is taken as the end of the file. In a format whose duration may be an estimate (MP3, AAC and the like, unlike MKV and MP4), an early end counts as missing data only where the file shows it: zeros where data is to come, as a download keeps the room for it, or fewer bytes than its header declares; elsewhere it is the end of the file |
| Playback | Media keys | always | Always (also while another window is active, through Windows' media controls), while the player is active, or off: the player takes none and they go on to Windows and the other programs |
| Tracks | Sound: languages | none | Up to three languages in order: a file opened for the first time plays the first of them it has; none of them, or none chosen, the track the file marks |
| Tracks | Subtitles: shown | as the file marks them | As the file marks them (a default or forced track), off, forced only (the signs and the lines in other languages), or in the languages below |
| Tracks | Subtitles: languages | none | Up to three, for "In the languages below": the first of them the file has; none of them, only forced subtitles in the sound's language |
| Tracks | Load from the folder | names with the video's | Subtitle files loaded with a video, from its folder and its `Subs`, `Subtitles` or `Sub` folder: the same name (`Film.srt`), names with the video's in them (`Film.rus.srt`, `Film.Forced.ass`), all in the folder, or none |
| Tracks | Remember the tracks | on | A file opened again plays the audio track and the subtitles (or no subtitles) chosen in it last time, whatever its length (subtitles from a file of their own by that file, loaded again if needed); kept for files with more than one audio track or with subtitles |
| Tracks | Like tracks in new files | on | A file opened for the first time plays the audio track and the subtitles most like those last chosen by hand: the same title first, else the same language, never a track in another language, and among equals forced subtitles or a commentary as the last one was or was not; subtitles turned off by hand stay off, and no subtitles are chosen while **Subtitles: shown** is off. Meant for series: the voice-over chosen in one episode plays in the next. Nothing alike leaves the usual choice |
| Video | Hardware decoding (GPU) | on | Decoding on the graphics card where possible; the decoder in use is shown below |
| Video | File name at the top | on | A strip with the file's name across the top in fullscreen, shown while the pointer is near the top edge |
| Video | Panel transparency | 12 % | Transparency of the control bar and the file's name over the picture in fullscreen, 0–80 % |
| Video | Saving a frame: folder | Pictures\MetalMPV | Where **Save the frame** writes; **Choose…** picks another folder, **Default** goes back |
| Video | Saving a frame: format | PNG | PNG keeps every pixel; a JPG file is several times smaller, at a little loss |
| Updates | Check at start-up | off | See [Update check and privacy](#update-check-and-privacy); **Check now** asks at once, and the date of the last check is shown |

The sound and the look of the subtitles are set on the Video, sound and subtitles panel (see above), and kept between runs. The list of fonts has a search field at its top. The **Keyboard shortcuts** category lists every action with its keys, which can be changed (see [Keyboard shortcuts](#keyboard-shortcuts)). The volume, the window and the playlist's visibility and width are remembered between runs.

## Keyboard shortcuts

The keys below are the defaults. In the settings, the **Keyboard shortcuts** category lists the actions in groups, each with its keys: a click on a key waits for the key combination (or the mouse's back or forward button, with `Ctrl`, `Alt` or `Shift` if wanted) that takes its place, **+** adds one more, the cross removes it, and `Esc` gives up. A combination another action had is taken from that action, and the page says so, with **Undo** beside. A search field above the groups leaves the actions whose name or keys contain the text typed. A few actions without a default key can be given one: Stop, A-B repeat: start, end, off (one key for the three in turn), Load subtitles, Equalizer, Reset the equalizer (flat, off), Reset the subtitles' look, Chapter marks on the seek bar, Time: both, elapsed, remaining, Shuffle, Sort and Clear the playlist, Show the file in its folder, Open the folder of saved frames, Save the cover…, Playlist in a window of its own / in the player, IPTV groups in the playlist, How far files were watched in the playlist, Minimize, Maximize and Close the program. **Reset the keys** puts them all back. The menus and tooltips show the keys as they are set. The mouse wheel, the double and middle clicks and the media keys stay as described; `Ctrl+V` and `Shift+Insert` cannot be chosen, and `Ctrl+Insert` is taken as `Ctrl+C`.

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `←` / `→`, the mouse's back / forward buttons | Back / forward by the short step (10 s), on keyframes; the mouse's buttons with `Ctrl` or `Shift` as the arrows |
| `Ctrl+←` / `Ctrl+→` | Back / forward by the long step (60 s), on keyframes |
| `Shift+←` / `Shift+→` | Back / forward by 1 s to the exact frame |
| `,` / `.` (or `D` / `F`) | Previous / next frame |
| `Home` / `End` | Start / end of the file |
| `Ctrl+Page Up` / `Ctrl+Page Down` | Previous / next chapter (more than 5 s into a chapter, back to its start; before the first, the file's start; past the last, a message says so) |
| `↑` / `↓`, mouse wheel | Volume up / down |
| `M` | Mute |
| `T` | The audio and subtitles window |
| `I` | The file information window |
| `U`, `F7` | The Video, sound and subtitles panel |
| `Ctrl+E` | Save the frame (into Pictures\MetalMPV unless another folder is chosen) |
| `Tab` | Shows the position, the duration and the chapter playing on the screen, going on while it is shown |
| `Ctrl+G` | Go to time: a window for a time in the file |
| `Ctrl+B` | Add a bookmark at the position playing (the **Bookmarks** submenu of the context menu) |
| `A` | Next audio track |
| `S`, `J` | Next subtitles; after the last, none |
| `V` | Subtitles off, and on again |
| `C` / `X` / `Z` | Faster / slower / normal speed (0.25× to 4×, the voice keeps its pitch; also the **Speed** submenu of the context menu) |
| `Ctrl+=` / `Ctrl+-` | Sound later / earlier by 100 ms against the picture |
| `Ctrl+0` | Sound back in sync |
| `Shift+.` / `Shift+,` | Subtitles later / earlier by 100 ms (`>` and `<`) |
| `Shift+0` | Subtitles back in sync |
| `Ctrl+Shift+←` / `Ctrl+Shift+→` | The subtitle line before / after the one on screen shown now (the delay set for it) |
| `E` / `N` / `B` | Equalizer / normalization / clearer speech on or off |
| `Shift+B` | Stereo to 5.1 on or off |
| `Page Up` / `Page Down` | Previous / next file of the playlist |
| Media keys | Play/Pause, Stop, Previous and Next of a keyboard, a headset or a remote, also while another window is active (through Windows' media controls, whose flyout over the volume's shows the title playing); see the **Media keys** setting |
| `Enter`, `Alt+Enter`, `F11`, double click | Fullscreen |
| `1` / `2` / `3` / `4` | Window for the picture at 50 % / 100 % / 150 % / 200 % of the video's size |
| `5` | The largest window the screen holds, without black bars |
| `` ` `` | Window fitted to the picture, without black bars |
| `6` | Fullscreen without black bars (the picture cropped); again, with them |
| `Q` | Reset the picture's settings: colour, zoom, filters, turns (as **Reset all** on the panel's Video tab) |
| `Ctrl+R` | Turn the picture 90° clockwise |
| `Ctrl+A` | Aspect ratio in turn: as in the file, 4:3, 16:9, 16:10, 1.85:1, 2.35:1, 2.39:1 |
| `Ctrl+D` | Deinterlacing: auto, on, off |
| `Ctrl+1` / `Ctrl+2` | Brightness down / up; `Ctrl+3`/`Ctrl+4` contrast, `Ctrl+5`/`Ctrl+6` gamma, `Ctrl+7`/`Ctrl+8` saturation |
| `Alt+=` / `Alt+-` | Zoom the picture in / out |
| `Alt+arrows` | Move the picture |
| `Alt+Backspace` | The picture's zoom and position as they were |
| `7` | Only the picture, without the title row, the control bar and the playlist; again, the interface returns |
| `8` | Interface: full, without the title row, compact, only the picture, in turn (also in the context menu) |
| `Esc` | Closes the file information window, else the tracks window, else the Video, sound and subtitles panel, else the playlist shown in fullscreen, else leaves fullscreen, else brings the interface back after `7`, else closes the program |
| `Ctrl+O` | Open files |
| `Ctrl+U` | Open an address: a direct link to a video or a stream |
| `L`, middle click on the picture | Show or hide the playlist; in fullscreen, over the right side of the picture |
| `R` | Repeat: off, the file, the list |
| `[` / `]` | A-B repeat: start / end at the position playing |
| `\` | A-B repeat off |
| `Ctrl+Shift+S` | Save the A-B stretch as a file |
| `Ctrl+T` | Over the other windows: never, always, while playing (also in the settings and the context menu) |
| `Delete` | Remove the selected playlist entries |
| `Ctrl+F` | Search the playlist |
| `Insert` | Add files to the playlist |
| `Ctrl+S` | Save the playlist as M3U8 |
| `F5`, `Ctrl+,` | Settings |
| `F1` | About |

A seek on keyframes is instant, since nothing has to be decoded before the frame shown; it lands on the keyframe nearest to the target, and the on-screen line reports where. An exact seek decodes from the previous keyframe up to the target frame.

## Update check and privacy

The update check is optional and off by default. With **Check at start-up** enabled on the Updates page of the settings, the program asks the GitHub Releases API (`api.github.com`) for the latest release at most once a day; **Check now** on the same page asks at once. The request carries nothing but the program version in its `User-Agent` header, and nothing is downloaded or installed: when a newer release exists, a button with its version appears in the title row and opens the release page in the browser. Otherwise the program accesses the network only for what is opened: an address or a playlist with addresses is played from its server, and with **Channel logos** on, the logos named by an IPTV list are fetched from their sites (over HTTPS only, never from the local network; see the settings). The streams of an IPTV list come from whoever runs its servers, which see the address of the computer and what it plays. `youtube-dl`/yt-dlp is not used.

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
| `src/playlist.rs`, `src/input.rs`, `src/keymap.rs`, `src/format.rs` | Playlist, commands and keys, texts of times and of the on-screen line |
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

Releases on GitHub are built by the **Release** workflow (`.github/workflows/release.yml`). Between releases the version in `Cargo.toml` carries `-dev`. A release drops the suffix (after `python tools/notices.py` if `Cargo.lock` has changed: a test checks the notices), is committed as `Version X.Y.Z`, and pushing a matching tag publishes it with the installer and the portable archive; the next version with `-dev` follows:

```
git tag v0.1.0
git push origin v0.1.0
```

The workflow stops if the tag does not match the version in `Cargo.toml` or the version still carries a suffix such as `-dev`. Running the workflow manually (Actions → Release → Run workflow) builds the files and attaches them to the run, without making a release.

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
