; "Selfkit Setup": one installer for every Selfkit app and the agent (per user, no admin
; except for the one-time WebView2 runtime install on PCs that lack it).
; Design: a single NSIS script with one section per component. Each section carries that
; component's own installer (the Tauri NSIS installer of the app, or the agent installer)
; and runs it silently, so every component keeps its own Apps & features entry and can be
; removed on its own. "Selfkit" in Apps & features removes all of them at once.
; The WebView2 runtime's offline installer is embedded once (when WEBVIEW2 is defined) and
; run only when WebView2 is missing, so the whole suite installs without internet.
;
; Build (CI does this, see .github/workflows/windows.yml):
;   makensis /DVERSION=0.1.0 /DSTAGE=<dir> [/DWEBVIEW2=<dir>\MicrosoftEdgeWebView2RuntimeInstallerX64.exe]
;            /DOUTFILE=<path>\SelfkitSetup_0.1.0_x64.exe installer\windows\setup.nsi
;   <dir> holds Agenda-setup.exe, Chrono-setup.exe, Dusk-setup.exe, Hisab-setup.exe,
;   Reflect-setup.exe, SelfkitAgent-setup.exe.
; Silent: SelfkitSetup.exe /S [/APPS=agent,agenda,chrono,dusk,hisab,reflect]  (default: all)
;         "Uninstall Selfkit.exe" /S [/DELETEDATA]
; Docs: docs/platforms/windows.md.
Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "pass /DVERSION=x.y.z"
!endif
!ifndef STAGE
  !error "pass /DSTAGE=dir with the component installers"
!endif
!ifndef OUTFILE
  !define OUTFILE "SelfkitSetup_${VERSION}_x64.exe"
!endif

!define PRODUCT "Selfkit"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT}"
!define APPUNINST "Software\Microsoft\Windows\CurrentVersion\Uninstall"
!define WEBVIEW2GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "Sections.nsh"
!include "StrFunc.nsh"
!include "nsDialogs.nsh"
!include "x64.nsh"
!include "${__FILEDIR__}\selfkit-data.nsh"
${StrStr}
${UnStrStr}

Name "${PRODUCT}"
Caption "Selfkit Setup"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCT}"
RequestExecutionLevel user
ShowInstDetails show
ShowUninstDetails show
BrandingText "Selfkit"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Selfkit Setup"
VIAddVersionKey "CompanyName" "Selfkit"
VIAddVersionKey "FileDescription" "Selfkit Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Selfkit"

!define MUI_ICON "${__FILEDIR__}\..\..\selfkit-agent\icons\icon.ico"
!define MUI_UNICON "${__FILEDIR__}\..\..\selfkit-agent\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Selfkit Setup"
!define MUI_WELCOMEPAGE_TEXT "This installs the Selfkit apps you pick, for you only. Everything works offline and your data stays on this PC, encrypted before it is ever synced.$\r$\n$\r$\nEach app also appears on its own in Apps & features, so you can remove any of them later.$\r$\n$\r$\nClick Next to continue."
!insertmacro MUI_PAGE_WELCOME
!define MUI_COMPONENTSPAGE_SMALLDESC
!define MUI_COMPONENTSPAGE_TEXT_TOP "Tick what you want to install. Already installed items are updated."
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TEXT "Selfkit is installed. Find the apps in the Start menu under Selfkit."
!insertmacro MUI_PAGE_FINISH

UninstPage custom un.DataPage un.DataPageLeave
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Failed

; Run one embedded component installer silently.
!macro SK_RUN_SETUP file label
  SetOutPath "$PLUGINSDIR"
  File "/oname=${file}" "${STAGE}\${file}"
  DetailPrint "Installing ${label}..."
  ExecWait '"$PLUGINSDIR\${file}" /S' $0
  ${If} $0 = 0
    DetailPrint "${label} installed."
  ${Else}
    DetailPrint "${label} failed (exit code $0)."
    StrCpy $Failed "$Failed ${label}"
  ${EndIf}
  Delete "$PLUGINSDIR\${file}"
!macroend

Section "-WebView2"
  ; Only needed by the apps, and only when the runtime is missing.
  ${IfNot} ${SectionIsSelected} 1
  ${AndIfNot} ${SectionIsSelected} 2
  ${AndIfNot} ${SectionIsSelected} 3
  ${AndIfNot} ${SectionIsSelected} 4
  ${AndIfNot} ${SectionIsSelected} 5
    Return
  ${EndIf}
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2GUID}" "pv"
  ${If} $0 == ""
  ${OrIf} $0 == "0.0.0.0"
    ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\${WEBVIEW2GUID}" "pv"
  ${EndIf}
  ${If} $0 != ""
  ${AndIf} $0 != "0.0.0.0"
    DetailPrint "WebView2 runtime $0 is present."
    Return
  ${EndIf}
  !ifdef WEBVIEW2
    SetOutPath "$PLUGINSDIR"
    File "/oname=webview2-offline.exe" "${WEBVIEW2}"
    DetailPrint "Installing the WebView2 runtime (the apps need it)..."
    ExecWait '"$PLUGINSDIR\webview2-offline.exe" /silent /install' $0
    DetailPrint "WebView2 installer exit code $0"
    Delete "$PLUGINSDIR\webview2-offline.exe"
  !else
    DetailPrint "WebView2 runtime missing; each app installer will download it."
  !endif
SectionEnd

; Section indexes 1..5 are the apps (used above), 6 is the agent.
Section "Agenda" SecAgenda
  !insertmacro SK_RUN_SETUP "Agenda-setup.exe" "Agenda"
SectionEnd
Section "Chrono" SecChrono
  !insertmacro SK_RUN_SETUP "Chrono-setup.exe" "Chrono"
SectionEnd
Section "Dusk" SecDusk
  !insertmacro SK_RUN_SETUP "Dusk-setup.exe" "Dusk"
SectionEnd
Section "Hisab" SecHisab
  !insertmacro SK_RUN_SETUP "Hisab-setup.exe" "Hisab"
SectionEnd
Section "Reflect" SecReflect
  !insertmacro SK_RUN_SETUP "Reflect-setup.exe" "Reflect"
SectionEnd
Section "Selfkit agent (background sync, tracking, backups)" SecAgent
  !insertmacro SK_RUN_SETUP "SelfkitAgent-setup.exe" "Selfkit agent"
SectionEnd

Section "-Finish"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall Selfkit.exe"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "Selfkit (all apps)"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "Selfkit"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" '"$INSTDIR\Uninstall Selfkit.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" '"$INSTDIR\Uninstall Selfkit.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\Uninstall Selfkit.exe" /S'
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1
  ${If} $Failed != ""
    MessageBox MB_ICONEXCLAMATION "These could not be installed:$Failed$\r$\nSee the details list." /SD IDOK
    SetErrorLevel 2
  ${EndIf}
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAgenda} "Tasks, offline and private."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecChrono} "Where your screen time goes."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDusk} "Sleep, estimated and logged."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecHisab} "Money, offline and private."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecReflect} "Your days at a glance."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAgent} "Runs in the background: syncs the apps, makes encrypted backups, records app time for Chrono and shows reminders. Recommended."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; /APPS=agenda,hisab,agent picks the components (mainly for silent installs).
!macro SK_PICK id name
  ${StrStr} $R2 ",$R1," ",${name},"
  ${If} $R2 == ""
    !insertmacro UnselectSection ${id}
  ${Else}
    !insertmacro SelectSection ${id}
  ${EndIf}
!macroend

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "Selfkit needs 64-bit Windows." /SD IDOK
    Abort
  ${EndIf}
  InitPluginsDir
  StrCpy $Failed ""
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/APPS=" $R1
  ${IfNot} ${Errors}
    !insertmacro SK_PICK ${SecAgenda} "agenda"
    !insertmacro SK_PICK ${SecChrono} "chrono"
    !insertmacro SK_PICK ${SecDusk} "dusk"
    !insertmacro SK_PICK ${SecHisab} "hisab"
    !insertmacro SK_PICK ${SecReflect} "reflect"
    !insertmacro SK_PICK ${SecAgent} "agent"
  ${EndIf}
FunctionEnd

; ---------- Uninstaller: removes every installed Selfkit component ----------

Var DeleteData
Var DeleteDataBox

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
  !insertmacro MUI_HEADER_TEXT "Uninstall Selfkit" "Remove every Selfkit app and the agent from this PC."
  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 0 100% 56u "All installed Selfkit apps (Agenda, Chrono, Dusk, Hisab, Reflect) and the Selfkit agent will be removed.$\r$\n$\r$\nYour Selfkit data is kept, so you can install again later and carry on, unless you tick the box below."
  Pop $0
  ${NSD_CreateCheckbox} 0 66u 100% 12u "Also delete ALL my Selfkit data on this PC (every app, settings, keys)"
  Pop $DeleteDataBox
  ${If} $DeleteData = 1
    ${NSD_Check} $DeleteDataBox
  ${EndIf}
  ${NSD_CreateLabel} 0 82u 100% 24u "Deleting cannot be undone. Data you synced to your own server or other devices is not touched."
  Pop $0
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

; Run one component's own uninstaller silently and wait for it.
!macro SK_UNINSTALL product
  ReadRegStr $1 HKCU "${APPUNINST}\${product}" "UninstallString"
  ${If} $1 != ""
    ; Strip the quotes around the path.
    StrCpy $2 $1 1
    ${If} $2 == '"'
      StrCpy $1 $1 "" 1
      ${UnStrStr} $2 $1 '"'
      StrLen $3 $2
      StrLen $4 $1
      IntOp $4 $4 - $3
      StrCpy $1 $1 $4
    ${EndIf}
    ${GetParent} $1 $2
    DetailPrint "Removing ${product}..."
    ${If} $DeleteData = 1
      ExecWait '"$1" /S /DELETEDATA _?=$2' $0
    ${Else}
      ExecWait '"$1" /S _?=$2' $0
    ${EndIf}
    DetailPrint "${product}: exit code $0"
    ; With _?= the uninstaller cannot delete itself; clean up after it.
    Delete "$1"
    RMDir "$2"
  ${EndIf}
!macroend

Section "Uninstall"
  !insertmacro SK_UNINSTALL "Agenda"
  !insertmacro SK_UNINSTALL "Chrono"
  !insertmacro SK_UNINSTALL "Dusk"
  !insertmacro SK_UNINSTALL "Hisab"
  !insertmacro SK_UNINSTALL "Reflect"
  !insertmacro SK_UNINSTALL "Selfkit Agent"
  ${If} $DeleteData = 1
    !insertmacro SK_DELETE_ALL_DATA
  ${EndIf}
  Delete "$INSTDIR\Uninstall Selfkit.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINSTKEY}"
SectionEnd
