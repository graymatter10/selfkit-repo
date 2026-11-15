; Tiny launcher that starts selfkit-agent.exe ("run") with its console window hidden.
; The agent is a console program (its CLI prints to the terminal), so starting it
; straight from the HKCU Run key would show an empty console window at every logon.
; This launcher sits next to selfkit-agent.exe and is what the Run key points at.
; Build: makensis /DOUTFILE=selfkit-agent-start.exe agent-launcher.nsi
; Docs: docs/platforms/windows.md.
Unicode true
!ifndef OUTFILE
  !define OUTFILE "selfkit-agent-start.exe"
!endif
!ifdef ICON
  Icon "${ICON}"
!endif
Name "Selfkit agent launcher"
OutFile "${OUTFILE}"
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
ShowInstDetails nevershow
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "Selfkit agent launcher"
VIAddVersionKey "CompanyName" "Selfkit"
VIAddVersionKey "FileDescription" "Starts the Selfkit agent in the background"
VIAddVersionKey "FileVersion" "1.0.0"
VIAddVersionKey "LegalCopyright" "Selfkit"

Section
  ; SW_HIDE is passed on to the new console, so no window appears.
  ExecShell "open" "$EXEDIR\selfkit-agent.exe" "run" SW_HIDE
SectionEnd
