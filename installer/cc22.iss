; Inno Setup script for CC-22.
; Build with: ISCC.exe installer\cc22.iss  (from the project root)
; Produces installer\output\CC-22-<version>-Setup.exe

#define MyAppName "CC-22"
#define MyAppVersion "1.0.0"
#define MyAppPublisher "Rafa Audio"
#define MyAppURL "https://github.com/overidiz/cc-22"
#define BundleDir "..\target\bundled"

[Setup]
AppId={{B7E3C2A1-CC22-4F0A-9E11-A1B2C3D4E5F6}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL=mailto:rafatoledoreis@gmail.com
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\EULA.md
OutputDir=output
OutputBaseFilename=CC-22-{#MyAppVersion}-Setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
PrivilegesRequired=admin
UninstallDisplayName={#MyAppName} {#MyAppVersion}
SetupIconFile=..\assets\cc22.ico
UninstallDisplayIcon={app}\CC-22.exe

[Types]
Name: "full"; Description: "Full installation (VST3 + CLAP + Standalone)"
Name: "custom"; Description: "Custom"; Flags: iscustom

[Components]
Name: "vst3"; Description: "VST3 plugin"; Types: full custom
Name: "clap"; Description: "CLAP plugin"; Types: full custom
Name: "standalone"; Description: "Standalone application"; Types: full custom

[Files]
; VST3 is a bundle directory.
Source: "{#BundleDir}\CC-22.vst3\*"; DestDir: "{commoncf64}\VST3\CC-22.vst3"; \
  Flags: recursesubdirs createallsubdirs ignoreversion; Components: vst3
; CLAP is a single file.
Source: "{#BundleDir}\CC-22.clap"; DestDir: "{commoncf64}\CLAP"; \
  Flags: ignoreversion; Components: clap
; Standalone + docs.
Source: "{#BundleDir}\CC-22.exe"; DestDir: "{app}"; Flags: ignoreversion; Components: standalone
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\EULA.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autodesktop}\CC-22"; Filename: "{app}\CC-22.exe"; Components: standalone
Name: "{group}\CC-22"; Filename: "{app}\CC-22.exe"; Components: standalone

[Run]
Filename: "{app}\CC-22.exe"; Description: "Launch CC-22 standalone"; \
  Flags: nowait postinstall skipifsilent; Components: standalone
