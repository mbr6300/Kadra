; Kadra – Windows-Installer (NSIS). Aufruf über make_windows.sh:
;   makensis -DVERSION=x.y.z -DEXE=<kadra.exe> -DOUT=<Kadra-Setup.exe> installer/kadra.nsi
; Installiert pro Benutzer (ohne Administratorrechte) nach %LOCALAPPDATA%\Programs\Kadra.

Unicode true
!include "MUI2.nsh"

!ifndef VERSION
  !error "VERSION fehlt (-DVERSION=x.y.z)"
!endif

!define APP "Kadra"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP} ${VERSION}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
InstallDirRegKey HKCU "Software\${APP}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} Setup"
VIAddVersionKey "LegalCopyright" "© 2026 mbr6300 · GNU GPL v3.0 oder neuer"

!define MUI_ICON "..\assets\windows\kadra.ico"
!define MUI_UNICON "..\assets\windows\kadra.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\kadra.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Kadra jetzt starten"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "German"

; Laufende Instanz beenden (läuft sonst unsichtbar im Infobereich weiter und sperrt die .exe).
!macro StopKadra
  nsExec::Exec 'taskkill /IM kadra.exe /F'
  Pop $0
  Sleep 300
!macroend

Section "Kadra (erforderlich)" SecMain
  SectionIn RO
  !insertmacro StopKadra
  SetOutPath "$INSTDIR"
  File "/oname=kadra.exe" "${EXE}"
  File "/oname=LICENSE.txt" "..\LICENSE"

  CreateDirectory "$SMPROGRAMS\${APP}"
  CreateShortcut "$SMPROGRAMS\${APP}\${APP}.lnk" "$INSTDIR\kadra.exe"
  CreateShortcut "$SMPROGRAMS\${APP}\${APP} deinstallieren.lnk" "$INSTDIR\uninstall.exe"

  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\${APP}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "mbr6300"
  WriteRegStr HKCU "${UNINST_KEY}" "URLInfoAbout" "https://github.com/mbr6300/Kadra"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\kadra.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Verknüpfung auf dem Desktop" SecDesktop
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\kadra.exe"
SectionEnd

Section /o "Beim Anmelden starten" SecAutostart
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP}" '"$INSTDIR\kadra.exe"'
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} "Programmdateien und Startmenü-Eintrag."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Symbol auf dem Desktop anlegen."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutostart} "Kadra beim Anmelden im Infobereich starten, damit die Tastenkürzel sofort bereit sind."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  !insertmacro StopKadra
  Delete "$INSTDIR\kadra.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APP}\${APP}.lnk"
  Delete "$SMPROGRAMS\${APP}\${APP} deinstallieren.lnk"
  RMDir "$SMPROGRAMS\${APP}"
  Delete "$DESKTOP\${APP}.lnk"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${APP}"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\${APP}"
  ; Aufnahmen und Einstellungen (%APPDATA%\Kadra) bleiben erhalten.
SectionEnd
