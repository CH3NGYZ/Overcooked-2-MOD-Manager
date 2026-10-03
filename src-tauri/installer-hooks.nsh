!include LogicLib.nsh
!include FileFunc.nsh

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
