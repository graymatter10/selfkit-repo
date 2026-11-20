; Selfkit user data on Windows: shared helpers for every Selfkit uninstaller.
; Docs: docs/platforms/windows.md ("Uninstall and your data").
;
; Data folder = selfkit_core::paths::data_dir():
;   %SELFKIT_DATA_DIR% if set, else %APPDATA%\selfkit (dirs::data_dir() = Roaming AppData).
; What belongs to one app <a> (nothing else is touched when only <a> is removed):
;   <a>.db, <a>.db-wal, <a>.db-shm, <a>-photos\, files\<a>\, backups\<a>\,
;   sync\state-<a>.json, sync\<a>.lock, and for Hisab exports\.
; Shared by every app (deleted only by "delete all Selfkit data"):
;   settings.db*, keys.json, device_id, sync\, files\, backups\, logs\, agent files.

!ifndef SELFKIT_DATA_NSH
!define SELFKIT_DATA_NSH
!include "LogicLib.nsh"

; Put the Selfkit data folder in $R9.
!macro SK_DATA_DIR
  ReadEnvStr $R9 SELFKIT_DATA_DIR
  ${If} $R9 == ""
    SetShellVarContext current
    StrCpy $R9 "$APPDATA\selfkit"
  ${EndIf}
!macroend

; Delete one app's Selfkit data (app = lowercase id: agenda, chrono, dusk, hisab, reflect).
!macro SK_DELETE_APP_DATA app
  Push $R9
  !insertmacro SK_DATA_DIR
  DetailPrint "Deleting Selfkit data of ${app} in $R9"
  Delete "$R9\${app}.db"
  Delete "$R9\${app}.db-wal"
  Delete "$R9\${app}.db-shm"
  RMDir /r "$R9\${app}-photos"
  RMDir /r "$R9\files\${app}"
  RMDir /r "$R9\backups\${app}"
  Delete "$R9\sync\state-${app}.json"
  Delete "$R9\sync\${app}.lock"
  !if "${app}" == "hisab"
    RMDir /r "$R9\exports"
  !endif
  ; Remove the parent folders only when they are now empty.
  RMDir "$R9\files"
  RMDir "$R9\backups"
  RMDir "$R9\sync"
  RMDir "$R9"
  Pop $R9
!macroend

; Delete the agent's own files (settings, health, reminders state, logs, backup schedule).
; The apps' local backup copies (backups\<a>\) belong to each app and stay (C99).
!macro SK_DELETE_AGENT_DATA
  Push $R9
  !insertmacro SK_DATA_DIR
  DetailPrint "Deleting Selfkit agent data in $R9"
  Delete "$R9\agent.json"
  Delete "$R9\health.json"
  Delete "$R9\agent-reminders.json"
  RMDir /r "$R9\logs"
  Delete "$R9\backups\state.json"
  RMDir "$R9\backups"
  RMDir "$R9"
  Pop $R9
!macroend

!macro SK_DELETE_WEBVIEW_DATA app
  SetShellVarContext current
  RMDir /r "$APPDATA\com.selfkit.${app}"
  RMDir /r "$LOCALAPPDATA\com.selfkit.${app}"
!macroend

; Delete ALL Selfkit data on this PC (every app, shared settings, keys, agent files).
; The whole folder is removed only when it is the default ...\selfkit folder;
; a custom SELFKIT_DATA_DIR only loses the files Selfkit is known to write.
!macro SK_DELETE_ALL_DATA
  !insertmacro SK_DELETE_APP_DATA agenda
  !insertmacro SK_DELETE_APP_DATA chrono
  !insertmacro SK_DELETE_APP_DATA dusk
  !insertmacro SK_DELETE_APP_DATA hisab
  !insertmacro SK_DELETE_APP_DATA reflect
  !insertmacro SK_DELETE_AGENT_DATA
  Push $R9
  !insertmacro SK_DATA_DIR
  Delete "$R9\settings.db"
  Delete "$R9\settings.db-wal"
  Delete "$R9\settings.db-shm"
  Delete "$R9\keys.json"
  Delete "$R9\device_id"
  RMDir /r "$R9\sync"
  RMDir /r "$R9\files"
  SetShellVarContext current
  ${If} $R9 == "$APPDATA\selfkit"
    RMDir /r "$R9"
  ${Else}
    RMDir "$R9"
  ${EndIf}
  RMDir /r "$LOCALAPPDATA\Selfkit"
  ; WebView data of every app (Tauri keeps it under the bundle id).
  !insertmacro SK_DELETE_WEBVIEW_DATA agenda
  !insertmacro SK_DELETE_WEBVIEW_DATA chrono
  !insertmacro SK_DELETE_WEBVIEW_DATA dusk
  !insertmacro SK_DELETE_WEBVIEW_DATA hisab
  !insertmacro SK_DELETE_WEBVIEW_DATA reflect
  Pop $R9
!macroend

!endif
