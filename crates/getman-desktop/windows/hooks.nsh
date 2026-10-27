; Getman's additions to the Windows setup program: the getman command.
;
; The setup puts the command line, getman.exe, in $INSTDIR\bin, and adds
; that folder to your PATH unless you untick the box on the welcome page.
; A setup that asks nothing (passive or silent, as updates run) adds it
; on a first install and leaves PATH as it is on an update. Uninstalling
; takes the folder off PATH again.
;
; PATH is edited with PowerShell rather than NSIS strings, which are cut
; at 1024 characters and would shorten a long PATH.

!include nsDialogs.nsh
!include LogicLib.nsh

; Empty until the welcome page asks, then ${BST_CHECKED} or ${BST_UNCHECKED}.
Var GetmanOnPath
Var GetmanOnPathBox

; The welcome page is the first page, so it takes these.
!define MUI_PAGE_CUSTOMFUNCTION_SHOW GetmanWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE GetmanWelcomeLeave

Function GetmanWelcomeShow
  ${NSD_CreateCheckbox} 120u 172u 195u 12u "Add the getman command to PATH"
  Pop $GetmanOnPathBox
  SetCtlColors $GetmanOnPathBox "" "FFFFFF"
  ${NSD_Check} $GetmanOnPathBox
FunctionEnd

Function GetmanWelcomeLeave
  ${NSD_GetState} $GetmanOnPathBox $GetmanOnPath
FunctionEnd

; Adds (ACTION "add") or removes (ACTION "remove") $INSTDIR\bin in the
; user's PATH, keeping every other entry as it is, unexpanded. It stops
; at the first error, so a PATH it could not read is never written back.
!macro GetmanPath ACTION
  System::Call 'Kernel32::SetEnvironmentVariable(t "GETMAN_BIN", t "$INSTDIR\bin")'
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$ErrorActionPreference = 'Stop'; $$key = 'HKCU:\Environment'; $$bin = $$env:GETMAN_BIN.TrimEnd('\'); $$path = (Get-Item $$key).GetValue('Path', '', 'DoNotExpandEnvironmentNames'); $$keep = @($$path -split ';' | Where-Object { $$_ -and $$_.TrimEnd('\') -ne $$bin }); if ('${ACTION}' -eq 'add') { $$keep += $$bin }; Set-ItemProperty -Path $$key -Name Path -Value ($$keep -join ';') -Type ExpandString"`
  Pop $0
  ; Tell open programs, such as Explorer, that PATH changed.
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $GetmanOnPath == ""
  ${AndIf} $UpdateMode <> 1
    StrCpy $GetmanOnPath ${BST_CHECKED}
  ${EndIf}
  ${If} $GetmanOnPath == ${BST_CHECKED}
    !insertmacro GetmanPath "add"
  ${ElseIf} $GetmanOnPath == ${BST_UNCHECKED}
    !insertmacro GetmanPath "remove"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro GetmanPath "remove"
  ${EndIf}
!macroend
