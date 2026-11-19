; AutoDesk NSIS 설치/제거 훅
; - 제거 전에 실행 중인 AutoDesk에 정상 종료를 요청하고(단일 인스턴스로 --quit 전달),
;   그래도 남아 있으면 "설치 폴더의 AutoDesk.exe" 경로와 정확히 일치하는 프로세스만 종료한다.
; - 로그인 자동 실행 등록은 AutoDesk가 만든 정확한 값 이름("AutoDesk")만 지운다.
; - 앱 데이터(설정·DB·백업) 삭제는 Tauri 제거 화면의 "앱 데이터 삭제" 체크박스로 사용자가 선택한다.

!macro NSIS_HOOK_PREINSTALL
  ; 업데이트 설치 시에도 실행 중인 앱을 정상 종료시킨다.
  IfFileExists "$INSTDIR\AutoDesk.exe" 0 +3
    ExecWait '"$INSTDIR\AutoDesk.exe" --quit'
    Sleep 1500
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$INSTDIR\AutoDesk.exe" 0 +3
    ExecWait '"$INSTDIR\AutoDesk.exe" --quit'
    Sleep 2000
  nsExec::Exec `powershell.exe -NoProfile -NonInteractive -Command "Get-Process -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq '$INSTDIR\AutoDesk.exe' } | Stop-Process -Force"`
  Pop $0
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "AutoDesk"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "AutoDesk"
!macroend
