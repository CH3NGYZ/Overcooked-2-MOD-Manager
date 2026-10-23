!include LogicLib.nsh
!include FileFunc.nsh

LangString OC2InvalidInstallDirectory 2052 "无法验证此安装目录。管理器不能安装到 BepInEx/plugins 或其子目录，请选择游戏目录外的位置。"
LangString OC2InvalidInstallDirectory 1028 "無法驗證此安裝目錄。管理器不能安裝到 BepInEx/plugins 或其子目錄，請選擇遊戲目錄外的位置。"
LangString OC2InvalidInstallDirectory 1033 "Cannot validate this installation directory. The manager cannot be installed in BepInEx/plugins or its subfolders. Choose a location outside the game directory."
LangString OC2InvalidInstallDirectory 1042 "이 설치 폴더를 확인할 수 없습니다. 관리자를 BepInEx/plugins 또는 그 하위 폴더에 설치할 수 없습니다. 게임 폴더 밖의 위치를 선택하세요."
LangString OC2InvalidInstallDirectory 1041 "このインストール先を確認できません。マネージャーを BepInEx/plugins またはそのサブフォルダーにはインストールできません。ゲームフォルダー外の場所を選択してください。"

LangString OC2MigrationFailed 2052 "自动迁移未完成，已保留原管理器或恢复副本。请查看安装详情后重试。"
LangString OC2MigrationFailed 1028 "自動遷移未完成，已保留原管理器或復原副本。請查看安裝詳細資料後重試。"
LangString OC2MigrationFailed 1033 "Automatic migration did not complete. The original manager or a recovery copy has been preserved. Check the installation details and try again."
LangString OC2MigrationFailed 1042 "자동 이전을 완료하지 못했습니다. 기존 관리자 또는 복구 사본이 보존되었습니다. 설치 세부 정보를 확인한 후 다시 시도하세요."
LangString OC2MigrationFailed 1041 "自動移行が完了しませんでした。元のマネージャーまたは復元コピーは保持されています。インストールの詳細を確認してから再試行してください。"

LangString OC2Migrating 2052 "正在将管理器迁移到默认安装目录…"
LangString OC2Migrating 1028 "正在將管理器遷移到預設安裝目錄…"
LangString OC2Migrating 1033 "Moving the manager to the default installation directory…"
LangString OC2Migrating 1042 "관리자를 기본 설치 폴더로 이전하는 중…"
LangString OC2Migrating 1041 "マネージャーを既定のインストール先へ移動しています…"

!macro NSIS_HOOK_PREINSTALL
  ; The installer runs elevated (perMachine), so it can close an elevated
  ; manager process before replacing the installed executable.
  nsExec::ExecToLog '"$SYSDIR\taskkill.exe" /F /IM OC2ModManager.exe'
  Pop $0
  ; CheckIfAppIsRunning in the template waits for remaining file users.
!macroend

!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$PROFILE\OC2ModManager"
  FileOpen $0 "$PROFILE\OC2ModManager\install-language.txt" w
  ${If} $LANGUAGE == 2052
    FileWrite $0 "zh-CN"
  ${ElseIf} $LANGUAGE == 1028
    FileWrite $0 "zh-TW"
  ${ElseIf} $LANGUAGE == 1042
    FileWrite $0 "ko"
  ${ElseIf} $LANGUAGE == 1041
    FileWrite $0 "ja"
  ${Else}
    FileWrite $0 "en"
  ${EndIf}
  FileClose $0

!macroend

!macro NSIS_HOOK_ONINSTSUCCESS
  ${GetParameters} $R0
  ${GetOptions} $R0 "/OC2AutoLaunch" $R1
  ${IfNot} ${Errors}
    ClearErrors
    Exec '"$INSTDIR\OC2ModManager.exe"'
    ${If} ${Errors}
      CreateDirectory "$PROFILE\OC2ModManager\logs"
      FileOpen $0 "$PROFILE\OC2ModManager\logs\update.log" a
      FileWrite $0 "NSIS: installation succeeded; automatic launch failed.$\r$\n"
      FileClose $0
    ${EndIf}
  ${EndIf}
!macroend
