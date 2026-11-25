!include LogicLib.nsh
!include FileFunc.nsh
!include "${__FILEDIR__}\nsis\deferred-update.nsh"
!include "${__FILEDIR__}\nsis\manager-languages.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; The installer runs as the current user. Close the ordinary manager process
  ; before replacing its executable; elevated file users stay protected below.
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /IM OC2ModManager.exe'
  Pop $0
  ; CheckIfAppIsRunning in the template waits for remaining file users.
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $OC2DeferredUpdate = 1
    Goto oc2_skip_install_language
  ${EndIf}
  ; 数据根在用户「文档」下（程序用 Shell 已知文件夹 Documents 定位），
  ; 安装语言标记必须写到同一处，首次启动才读得到。
  CreateDirectory "$DOCUMENTS\OC2ModManager"
  FileOpen $0 "$DOCUMENTS\OC2ModManager\install-language.txt" w
  FileWrite $0 "$(OC2InstallLanguage)"
  FileClose $0
  oc2_skip_install_language:
!macroend

!macro NSIS_HOOK_ONINSTSUCCESS
  ${If} $OC2DeferredUpdate != 1
    ${GetParameters} $R0
    ${GetOptions} $R0 "/OC2AutoLaunch" $R1
    ${IfNot} ${Errors}
      StrCpy $OC2RestartAfterUpdate 1
    ${EndIf}
  ${EndIf}
  ${If} $OC2RestartAfterUpdate = 1
    ClearErrors
    Exec '"$INSTDIR\OC2ModManager.exe"'
    ${If} ${Errors}
      ${If} $OC2DeferredUpdate = 1
        !insertmacro OC2_UPDATE_RESULT "restart_failed"
        SetErrorLevel 1
        Return
      ${EndIf}
      CreateDirectory "$DOCUMENTS\OC2ModManager\logs"
      FileOpen $0 "$DOCUMENTS\OC2ModManager\logs\update.log" a
      FileWrite $0 "NSIS: installation succeeded; automatic launch failed.$\r$\n"
      FileClose $0
    ${EndIf}
  ${EndIf}
!macroend
