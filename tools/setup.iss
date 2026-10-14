; MetalMPV installer (Inno Setup 6).
; Built by tools\make_release.py, which passes the version and the icon path:
;   ISCC /DMyAppVersion=0.1.0 /DVersionInfoVersion=0.1.0.0 /DAppIcon=..\target\app.ico tools\setup.iss

#define MyAppName "MetalMPV"
#define MyAppFileStem "MetalMPV"
#define MyAppExeName "metalmpv.exe"
#define MyAppPublisher "Fan4_Metal"
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#ifndef VersionInfoVersion
  #define VersionInfoVersion "0.0.0.0"
#endif
#ifndef AppIcon
  #define AppIcon "..\target\app.ico"
#endif

[Setup]
; A new GUID for every app (the same one in tools\make_release.py).
AppId={{4ABB9378-64A4-4E8E-B41D-B43385F9D268}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
VersionInfoVersion={#VersionInfoVersion}
AppCopyright=Copyright (C) 2026 {#MyAppPublisher}
AppPublisher={#MyAppPublisher}
; Per-user installation without administrator rights:
; {autopf} points to %LOCALAPPDATA%\Programs.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename={#MyAppFileStem}_{#MyAppVersion}_Setup
SetupIconFile={#AppIcon}
UninstallDisplayIcon={app}\{#MyAppExeName}
; No licence to accept: the GPL v3, under which the program goes with
; libmpv, asks for no acceptance. A page before the installation says
; which licences apply (InfoBeforeFile in [Languages]).
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Windows 10 or later: the DLL and the DWM calls need it.
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; A running copy is closed (Restart Manager) before its exe is replaced.
CloseApplications=yes
; File associations change: Explorer is told to refresh its icons.
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"; InfoBeforeFile: "installer_notice.en.txt"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"; InfoBeforeFile: "installer_notice.ru.txt"

[CustomMessages]
english.AssocGroup=File types:
russian.AssocGroup=Типы файлов:
english.AssocTask=Register MetalMPV for video and audio files and playlists (MP4, MKV, AVI, MOV, WMV, MPEG, TS, MP3, FLAC, M3U and others)
russian.AssocTask=Зарегистрировать MetalMPV для видео- и аудиофайлов и плейлистов (MP4, MKV, AVI, MOV, WMV, MPEG, TS, MP3, FLAC, M3U и других)
english.AssocStatus=Registering file types...
russian.AssocStatus=Регистрация типов файлов...
english.ChooseDefault=Choose MetalMPV as the default player (opens Windows Settings)
russian.ChooseDefault=Выбрать MetalMPV проигрывателем по умолчанию (откроются параметры Windows)

[Tasks]
Name: "assoc"; Description: "{cm:AssocTask}"; GroupDescription: "{cm:AssocGroup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; The playback engine, loaded from the program's folder.
Source: "..\target\release\libmpv-2.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\licenses\GPL-3.0.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\licenses\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.ru.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
; The file types are registered by the program itself (src\assoc.rs, under
; HKCU): a ProgID with an icon for each format, "Open with" entries and
; Default apps capabilities. Windows lets only the user choose the default
; program, so the last page offers to open Settings for that.
Filename: "{app}\{#MyAppExeName}"; Parameters: "--register"; StatusMsg: "{cm:AssocStatus}"; Flags: runhidden waituntilterminated; Tasks: assoc
Filename: "ms-settings:defaultapps?registeredAppUser={#MyAppName}"; Description: "{cm:ChooseDefault}"; Flags: shellexec nowait postinstall skipifsilent unchecked; Tasks: assoc
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Removes everything --register wrote; nothing happens if it was not run.
Filename: "{app}\{#MyAppExeName}"; Parameters: "--unregister"; RunOnceId: "UnregisterFileTypes"; Flags: runhidden waituntilterminated

; Optional blocks, taken from the sibling projects when needed:
; - Explorer context menu for folders and drives ([Registry] under
;   HKCU\Software\Classes\{Directory,Drive}\shell\<Verb>, task "contextmenu"):
;   disk_flashlight/tools/setup.iss.
; - Asking a running copy to quit through its own --quit ([Code]
;   PrepareToInstall and [UninstallRun]) and offering to delete the settings
;   on uninstallation (CurUninstallStepChanged): qrec/tools/setup.iss.
; - MinVersion=10.0.19041 (here 10.0) when an API needs Windows 10 2004: qrec.
