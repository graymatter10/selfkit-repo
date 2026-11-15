; Selfkit agent installer (per user, no admin).
; Build (CI does this, see .github/workflows/windows.yml):
;   makensis /DVERSION=0.1.0 /DAGENT_EXE=<path>\selfkit-agent.exe /DLAUNCHER_EXE=<path>\selfkit-agent-start.exe
;            /DOUTFILE=<path>\SelfkitAgent_0.1.0_x64-setup.exe installer\windows\agent.nsi
; What it does: copies the agent to %LOCALAPPDATA%\Programs\Selfkit Agent, registers the
; browser native-messaging host (selfkit-agent install-native-host), adds autostart
; (HKCU Run "Selfkit agent" -> selfkit-agent-start.exe, which starts "selfkit-agent run"
; hidden), starts the agent, and adds "Selfkit Agent" to Apps & features.
; Uninstall: stops the agent, removes autostart, the native host and the program files.
; Your data stays unless you tick "Also delete the Selfkit agent's data" (or pass /DELETEDATA).
; Silent: setup.exe /S [/NOSTART]   uninstall.exe /S [/DELETEDATA]
; Docs: docs/platforms/windows.md.
Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "pass /DVERSION=x.y.z"
!endif
!ifndef AGENT_EXE
  !error "pass /DAGENT_EXE=path\to\selfkit-agent.exe"
!endif
!ifndef LAUNCHER_EXE
  !error "pass /DLAUNCHER_EXE=path\to\selfkit-agent-start.exe"
!endif
!ifndef OUTFILE
  !define OUTFILE "SelfkitAgent_${VERSION}_x64-setup.exe"
!endif

!define PRODUCT "Selfkit Agent"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"
!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define RUNVALUE "Selfkit agent"
!define HOST "com.selfkit.agent"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "nsDialogs.nsh"
!include "x64.nsh"
!include "${__FILEDIR__}\selfkit-data.nsh"

Name "${PRODUCT}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCT}"
RequestExecutionLevel user
ShowInstDetails show
ShowUninstDetails show
BrandingText "Selfkit"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "CompanyName" "Selfkit"
VIAddVersionKey "FileDescription" "${PRODUCT} setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Selfkit"

!define MUI_ICON "${__FILEDIR__}\..\..\selfkit-agent\icons\icon.ico"
!define MUI_UNICON "${__FILEDIR__}\..\..\selfkit-agent\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "The Selfkit agent runs in the background on this PC: it records which app is in front for Chrono (app names only, never window titles), syncs your Selfkit apps, makes encrypted backups and shows Agenda reminders.$\r$\n$\r$\nIt is installed for you only and starts when you sign in.$\r$\n$\r$\nClick Install to continue."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TEXT "The Selfkit agent is installed and running.$\r$\n$\r$\nTo record browser time, add the Chrono extension to your browser. Set up keys and sync with: selfkit-agent init (see the Selfkit docs)."
!insertmacro MUI_PAGE_FINISH

UninstPage custom un.DataPage un.DataPageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var NoStart
Var DeleteData
Var DeleteDataBox

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "The Selfkit agent needs 64-bit Windows." /SD IDOK
    Abort
  ${EndIf}
  StrCpy $NoStart 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/NOSTART" $R1
  ${IfNot} ${Errors}
    StrCpy $NoStart 1
  ${EndIf}
FunctionEnd

!macro SK_STOP_AGENT
  ; Stop this user's running agent (the exe would be locked otherwise).
  ReadEnvStr $R1 USERNAME
  nsExec::Exec 'taskkill /F /T /IM selfkit-agent.exe /FI "USERNAME eq $R1"'
  Pop $R2
  Sleep 500
!macroend

Section "Selfkit agent" SecAgent
  SectionIn RO
  !insertmacro SK_STOP_AGENT
  SetOutPath "$INSTDIR"
  File "/oname=selfkit-agent.exe" "${AGENT_EXE}"
  File "/oname=selfkit-agent-start.exe" "${LAUNCHER_EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Browser native-messaging host for the Chrono extension (manifests in
  ; %LOCALAPPDATA%\Selfkit\native-host, HKCU registry keys for Chrome, Chromium, Edge, Firefox).
  DetailPrint "Registering the browser native-messaging host"
  nsExec::ExecToLog '"$INSTDIR\selfkit-agent.exe" install-native-host'
  Pop $R0
  ${If} $R0 != 0
    DetailPrint "install-native-host failed ($R0); run it again later from a terminal"
  ${EndIf}

  ; Autostart at sign-in (per user, no admin).
  WriteRegStr HKCU "${RUNKEY}" "${RUNVALUE}" '"$INSTDIR\selfkit-agent-start.exe"'

  ; Apps & features entry.
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "Selfkit"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" '"$INSTDIR\selfkit-agent.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTKEY}" "EstimatedSize" "$0"

  ${If} $NoStart = 0
    DetailPrint "Starting the agent"
    ExecShell "open" "$INSTDIR\selfkit-agent-start.exe"
  ${EndIf}
SectionEnd

; ---------- Uninstaller ----------

Function un.onInit
  StrCpy $DeleteData 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/DELETEDATA" $R1
  ${IfNot} ${Errors}
    StrCpy $DeleteData 1
  ${EndIf}
FunctionEnd

Function un.DataPage
  !insertmacro MUI_HEADER_TEXT "Uninstall ${PRODUCT}" "Remove the Selfkit agent from this PC."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 48u "The agent will be stopped and removed, with its autostart entry and browser connection.$\r$\n$\r$\nYour Selfkit data is kept unless you tick the box below. App data (Agenda, Hisab, ...) is never deleted here."
  Pop $0
  ${NSD_CreateCheckbox} 0 60u 100% 12u "Also delete the Selfkit agent's data (agent settings, logs, local backups)"
  Pop $DeleteDataBox
  ${If} $DeleteData = 1
    ${NSD_Check} $DeleteDataBox
  ${EndIf}
  nsDialogs::Show
FunctionEnd

Function un.DataPageLeave
  ${NSD_GetState} $DeleteDataBox $DeleteData
  ${If} $DeleteData == ${BST_CHECKED}
    StrCpy $DeleteData 1
  ${Else}
    StrCpy $DeleteData 0
  ${EndIf}
FunctionEnd

Section "Uninstall"
  !insertmacro SK_STOP_AGENT
  DeleteRegValue HKCU "${RUNKEY}" "${RUNVALUE}"
  ; An older manual setup may have used a logon task (selfkit-agent install-service).
  nsExec::Exec 'schtasks /Delete /TN "Selfkit agent" /F'
  Pop $R0
  ${If} ${FileExists} "$INSTDIR\selfkit-agent.exe"
    nsExec::ExecToLog '"$INSTDIR\selfkit-agent.exe" install-native-host --uninstall'
    Pop $R0
  ${EndIf}
  ; Belt and braces: the same keys and manifests, removed directly.
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\${HOST}"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\${HOST}"
  SetShellVarContext current
  RMDir /r "$LOCALAPPDATA\Selfkit\native-host"
  RMDir "$LOCALAPPDATA\Selfkit"

  Delete "$INSTDIR\selfkit-agent.exe"
  Delete "$INSTDIR\selfkit-agent-start.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTKEY}"

  ${If} $DeleteData = 1
    !insertmacro SK_DELETE_AGENT_DATA
  ${EndIf}
SectionEnd
