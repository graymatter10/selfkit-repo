; Tauri NSIS installer hooks shared by every Selfkit app
; (tauri.conf.json > bundle > windows > nsis > installerHooks).
; Docs: docs/platforms/windows.md.
;
; Uninstall keeps your data. The confirm page shows Tauri's checkbox, relabelled in
; app-english.nsh to "Also delete my Selfkit data for <App>" (unchecked). When it is
; ticked, or the silent uninstaller gets /DELETEDATA, the hooks delete this app's
; Selfkit data (see selfkit-data.nsh) and Tauri deletes its WebView data
; (%APPDATA%\com.selfkit.<app>, %LOCALAPPDATA%\com.selfkit.<app>). Nothing that
; belongs to another app or is shared is touched.
;
; Shortcut: Start menu > Selfkit > <App>, for silent installs too (PREINSTALL hook).
;
; Silent use: <App>_x.y.z_x64-setup.exe /S           install for this user
;              uninstall.exe /S                        uninstall, keep data
;              uninstall.exe /S /DELETEDATA            uninstall and delete this app's data

!include "${__FILEDIR__}\selfkit-data.nsh"
!include "FileFunc.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; Silent installs skip the Start-menu page, which leaves the folder empty and puts the
  ; shortcut straight into Programs. Use the same "Selfkit" folder as the page default.
  ${If} $AppStartMenuFolder == ""
    StrCpy $AppStartMenuFolder "Selfkit"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ${GetOptions} $CMDLINE "/DELETEDATA" $R0
  ${IfNot} ${Errors}
    StrCpy $DeleteAppDataCheckboxState 1
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; com.selfkit.agenda -> agenda (the name selfkit_core::paths uses). BUNDLEID is
  ; defined after this file is included, so this runs inside the macro.
  !ifndef SK_APP
    !searchreplace SK_APP "${BUNDLEID}" "com.selfkit." ""
  !endif
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    !insertmacro SK_DELETE_APP_DATA "${SK_APP}"
  ${EndIf}
!macroend
