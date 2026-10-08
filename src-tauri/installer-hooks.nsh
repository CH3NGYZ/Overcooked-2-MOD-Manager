!include LogicLib.nsh
!include FileFunc.nsh

!macro NSIS_HOOK_PREINSTALL
  ; The installer runs elevated (perMachine), so it can close an elevated
  ; manager process before replacing the installed executable.
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /T /IM OC2ModManager.exe'
  Pop $0
  Sleep 300
!macroend

!macro NSIS_HOOK_POSTINSTALL
  FileOpen $0 "$INSTDIR\install-language.txt" w
  ${If} $LANGUAGE == 2052
    FileWrite $0 "zh-CN"
  ${ElseIf} $LANGUAGE == 1028
    FileWrite $0 "zh-TW"
  ${ElseIf} $LANGUAGE == 1042
    FileWrite $0 "ko"
  ${Else}
    FileWrite $0 "en"
  ${EndIf}
  FileClose $0

  ${GetParameters} $R0
  ${GetOptions} $R0 "/OC2AutoLaunch" $R1
  ${IfNot} ${Errors}
    Exec '"$INSTDIR\OC2ModManager.exe"'
  ${EndIf}
!macroend
