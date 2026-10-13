# Third-party licenses

MetalMPV's own source code is licensed under the MIT License (see [LICENSE](LICENSE)).

## libmpv

The installer and the portable archive include `libmpv-2.dll`, the library of the [mpv](https://mpv.io) media player, which decodes and renders every file the program plays. The program loads it from its own folder at run time; the DLL can be replaced with another build of libmpv (client API 2.x).

The included build comes from [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake), release [20261012](https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261012), file `mpv-dev-x86_64-20261012-git-88eb9f2e73.7z` (mpv commit [`88eb9f2e73`](https://github.com/mpv-player/mpv/commit/88eb9f2e73)). It is built with mpv's GPL features and with FFmpeg configured as `--enable-gpl --enable-version3`, so the DLL as distributed is licensed under the **GNU General Public License, version 3** (see [licenses/GPL-3.0.txt](licenses/GPL-3.0.txt)). Consequently, the program as distributed together with this DLL is subject to the terms of the GPL version 3; the MIT-licensed source code of MetalMPV is compatible with them.

The complete corresponding source code of the DLL is available from:

- mpv: <https://github.com/mpv-player/mpv> (commit `88eb9f2e73`);
- the build scripts, with the exact versions of FFmpeg and the other libraries it includes: <https://github.com/shinchiro/mpv-winbuild-cmake> (release `20261012`, its workflow run and `packages/` folder).

`tools/get_libmpv.py` downloads exactly this build and checks its SHA-256 (`a739eef92a907dfb8ea997564a3fa565b2a4f376f20a25755e7645d0c70fe5e6`).

## egui / eframe

The user interface uses [egui and eframe](https://github.com/emilk/egui), licensed under the MIT License or the Apache License 2.0, and further Rust crates under permissive licenses (MIT, Apache 2.0, BSD, ISC, Zlib, BSL, Unicode), compiled into `metalmpv.exe`, together with the fonts egui embeds (Ubuntu Font Licence, SIL Open Font License 1.1, MIT). Their licence texts and copyright notices are in [licenses/THIRD_PARTY_NOTICES.txt](licenses/THIRD_PARTY_NOTICES.txt), written by `tools/notices.py` from `Cargo.lock`; the program also shows them in its About window (**Licenses…**).
