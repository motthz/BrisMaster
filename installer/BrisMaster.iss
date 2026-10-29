; Installatore di BrisMaster (Inno Setup 6). Lo compila il workflow di release:
;   iscc /DAppVersion=0.4.6 /DExeDir=..\target\release installer\BrisMaster.iss
; Installa per l'utente corrente, senza privilegi di amministratore, in
; %LOCALAPPDATA%\Programs\BrisMaster (così l'app può aggiornarsi da sola), con il
; collegamento sul desktop e nel menu Start e la disinstallazione da Impostazioni → App.
; Le partite salvate e la memoria delle carte (Documenti\BrisMaster) non vengono toccate.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExeDir
  #define ExeDir "..\target\release"
#endif

[Setup]
AppId={{6F3C2A51-8D4B-4E7A-9B1C-2F5E8A7D4C90}
AppName=BrisMaster
AppVersion={#AppVersion}
AppVerName=BrisMaster {#AppVersion}
AppPublisher=Motthz
AppPublisherURL=https://github.com/motthz/BrisMaster
AppSupportURL=https://github.com/motthz/BrisMaster/blob/main/docs/ANTIVIRUS.md
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Installazione di BrisMaster
DefaultDirName={autopf}\BrisMaster
DefaultGroupName=BrisMaster
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=BrisMaster-Setup
SetupIconFile=..\crates\app\assets\brismaster.ico
UninstallDisplayIcon={app}\BrisMaster.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un collegamento sul desktop"; GroupDescription: "Collegamenti:"

[Files]
Source: "{#ExeDir}\brismaster.exe"; DestDir: "{app}"; DestName: "BrisMaster.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\BrisMaster"; Filename: "{app}\BrisMaster.exe"
Name: "{autodesktop}\BrisMaster"; Filename: "{app}\BrisMaster.exe"; Tasks: desktopicon

[Run]
; Anche dopo l'aggiornamento automatico (installazione silenziosa) l'app si riapre da sola.
Filename: "{app}\BrisMaster.exe"; Description: "Avvia BrisMaster"; Flags: nowait postinstall

[UninstallDelete]
; File lasciati dagli aggiornamenti automatici delle versioni fino alla 0.5.1.
Type: files; Name: "{app}\.BrisMaster-*.exe.new"
