; camcraft Windows installer (M13.3): an Inno Setup script that packs the
; SAME dist tree the portable zip is made from. Original text, specced from
; the Inno Setup 6 reference (jrsoftware.org/ishelp); no third-party material.
;
; The zip stays the primary test vehicle; this installer is additive. It
; installs per-user by default (no administrator rights, under
; {localappdata}\Programs\camcraft) and offers an all-users install on the
; first page for those who want Program Files.
;
; Compile (scripts/installer.ps1 does this for you):
;   ISCC.exe /DAppVersion=0.8.0 /DStageDir=dist\camcraft-0.8.0-win-x64 ^
;            /DOutDir=dist scripts\installer\camcraft.iss
; Every /D below has a default so the script also compiles from the IDE.
;
; Signing: when a certificate is configured (CAMCRAFT_SIGN_*, see
; scripts/sign.ps1) installer.ps1 adds /DSignToolName=camcraft and defines
; that sign tool with /Scamcraft=<sign.ps1 call>. ISCC then signs both the
; setup exe and the uninstaller it embeds. Without it nothing is signed and
; the script compiles exactly as an unsigned build always has.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef StageDir
  #define StageDir "..\..\dist\camcraft-" + AppVersion + "-win-x64"
#endif
#ifndef OutDir
  #define OutDir "..\..\dist"
#endif
#define AppName "camcraft"
#define AppExe "camcraft-ui.exe"

[Setup]
; Fixed AppId: upgrades find the previous install and replace it in place.
AppId={{04EB9DC4-9BD1-4F49-B385-33505EDA0E5C}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppName} project
SetupIconFile=..\..\assets\logo\icons\camcraft.ico
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} setup
; Per-user by default: "lowest" needs no elevation and {autopf} then resolves
; to {localappdata}\Programs. The override dialog lets the user pick
; "all users" (elevates, {autopf} becomes Program Files) on the first page.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutDir}
OutputBaseFilename={#AppName}-{#AppVersion}-win-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\{#AppExe}
ShowLanguageDialog=no
#ifdef SignToolName
; Named sign tool, defined on the ISCC command line by installer.ps1.
SignTool={#SignToolName}
SignedUninstaller=yes
#endif
; The uninstaller removes what the installer wrote; user data in the install
; dir (projects, exported NC) is left alone - it is listed, never deleted.

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
; The whole staged dist tree, as the zip ships it: the three exes, assets\
; (posts, machines, ui, scripts, themes), RELEASE_NOTES.md and TESTING.md.
; ignoreversion: our exes carry no VERSIONINFO, so compare by content.
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{group}\{#AppName} hand-test checklist"; Filename: "{app}\TESTING.md"
Name: "{group}\{#AppName} release notes"; Filename: "{app}\RELEASE_NOTES.md"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Start {#AppName} now"; Flags: nowait postinstall skipifsilent
