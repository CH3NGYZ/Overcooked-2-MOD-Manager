Var OC2DeferredUpdate
Var OC2AdminDeferredUpdate
Var OC2RestartAfterUpdate
Var OC2StagedMain
Var OC2UpdateOutcome

!ifndef OC2_UPDATE_EXIT_WAIT_MS
  !define OC2_UPDATE_EXIT_WAIT_MS 30000
!endif

!macro OC2_UPDATE_RESULT Outcome
  ${If} $OC2DeferredUpdate = 1
    StrCpy $OC2UpdateOutcome "${Outcome}"
    Call OC2WriteUpdateResult
  ${EndIf}
!macroend

!macro OC2_STAGE_MAIN_BINARY
  ${If} $OC2DeferredUpdate = 1
    ${IfNot} ${Errors}
      GetTempFileName $OC2StagedMain $INSTDIR
      ${IfNot} ${Errors}
        File "/oname=$OC2StagedMain" "${MAINBINARYSRCPATH}"
      ${EndIf}
    ${EndIf}
    ${If} ${Errors}
      !insertmacro OC2_UPDATE_RESULT "stage_executable_failed"
      SetErrorLevel 1
      Abort
    ${EndIf}
  ${Else}
    File "${MAINBINARYSRCPATH}"
  ${EndIf}
!macroend

!macro OC2_DEFERRED_UPDATE_FUNCTIONS
Function OC2WriteUpdateResult
  Push $0
  ClearErrors
  FileOpen $0 "$EXEDIR\update-result.txt" w
  ${IfNot} ${Errors}
    FileWriteUTF16LE $0 "${VERSION}|$OC2UpdateOutcome|$INSTDIR|restart=$OC2RestartAfterUpdate$\r$\n"
    FileClose $0
  ${EndIf}
  Pop $0
FunctionEnd

Function OC2WaitForUpdateOwner
  ${GetParameters} $R0
  ${GetOptions} $R0 "/OC2AdminDeferred" $R2
  ${IfNot} ${Errors}
  !ifdef OC2_ADMIN_DEFERRED_UPDATE
    ${If} $R2 != ""
      Goto oc2_update_wait_failed
    ${EndIf}
    StrCpy $OC2AdminDeferredUpdate 1
  !else
    Goto oc2_update_wait_failed
  !endif
  ${EndIf}
  ${GetOptions} $R0 "/OC2WaitForPid" $R1
  ${If} ${Errors}
    ${If} $OC2AdminDeferredUpdate = 1
      Goto oc2_update_wait_failed
    ${EndIf}
    Return
  ${EndIf}
  StrCpy $OC2DeferredUpdate 1
  StrCpy $OC2RestartAfterUpdate 0
  ${IfNot} ${Silent}
    Goto oc2_update_wait_failed
  ${EndIf}
  ${GetOptions} $R0 "/UPDATE" $R2
  ${If} ${Errors}
  ${OrIf} $R2 != ""
    Goto oc2_update_wait_failed
  ${EndIf}
  StrCpy $UpdateMode 1

  StrCpy $R2 $R1 1
  ${If} $R2 != "="
    Goto oc2_update_wait_failed
  ${EndIf}
  StrCpy $R1 $R1 "" 1
  StrLen $R2 $R1
  ${If} $R2 = 0
  ${OrIf} $R2 > 10
    Goto oc2_update_wait_failed
  ${EndIf}
  StrCpy $R3 0
  StrCpy $R4 0
  ${DoWhile} $R3 < $R2
    StrCpy $R5 $R1 1 $R3
    ${If} $R5 S< "0"
    ${OrIf} $R5 S> "9"
      Goto oc2_update_wait_failed
    ${EndIf}
    System::Int64Op $R4 * 10
    Pop $R4
    System::Int64Op $R4 + $R5
    Pop $R4
    IntOp $R3 $R3 + 1
  ${Loop}
  System::Int64Op $R4 > 4294967295
  Pop $R2
  ${If} $R2 != 0
  ${OrIf} $R4 == 0
    Goto oc2_update_wait_failed
  ${EndIf}

  ${GetOptions} $R0 "/OC2AutoLaunch" $R2
  ${IfNot} ${Errors}
    ${If} $R2 != ""
    ${OrIf} $OC2AdminDeferredUpdate = 1
      Goto oc2_update_wait_failed
    ${EndIf}
    StrCpy $OC2RestartAfterUpdate 1
  ${EndIf}

  System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i R4) p.R5 ?e'
  Pop $R6
  ${If} $R5 P= 0
    ${If} $R6 != 87
      Goto oc2_update_wait_failed
    ${EndIf}
  ${Else}
    System::Call 'kernel32::WaitForSingleObject(p R5, i ${OC2_UPDATE_EXIT_WAIT_MS}) i.R6'
    System::Call 'kernel32::CloseHandle(p R5)'
    ${If} $R6 != 0
      Goto oc2_update_wait_failed
    ${EndIf}
  ${EndIf}
  Return

  oc2_update_wait_failed:
    !insertmacro OC2_UPDATE_RESULT "wait_failed"
    SetErrorLevel 1
    Quit
FunctionEnd

Function OC2CheckDeferredTarget
  ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    Return
  ${EndIf}
  System::Call 'kernel32::CreateFileW(w "$INSTDIR\${MAINBINARYNAME}.exe", i 0x40000000, i 0, p 0, i 3, i 0x80, p 0) p.R0'
  ${If} $R0 P<> -1
    System::Call 'kernel32::CloseHandle(p R0)'
    Return
  ${EndIf}
  !insertmacro OC2_UPDATE_RESULT "target_unavailable"
  SetErrorLevel 1
  Abort
FunctionEnd

Function OC2ReplaceDeferredMain
  System::Call 'kernel32::MoveFileExW(w "$OC2StagedMain", w "$INSTDIR\${MAINBINARYNAME}.exe", i 9) i.R0'
  ${If} $R0 = 0
    !insertmacro OC2_UPDATE_RESULT "replace_executable_failed"
    SetErrorLevel 1
    Abort
  ${EndIf}
  StrCpy $OC2StagedMain ""
FunctionEnd
!macroend
