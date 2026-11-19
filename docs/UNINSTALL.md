# 제거

AutoDesk는 서비스·데몬·예약 작업을 만들지 않는다. 앱이 OS에 남기는 것은 **로그인 자동 실행 등록 1개**와 **앱 데이터 폴더**뿐이다.

## 권장: 앱 안에서 완전 제거 (Windows · macOS 공통)

설정 → **AutoDesk 완전 제거** → 대상 확인 → `제거` 입력.

1. (선택) 백업을 `문서/AutoDesk-Backups-YYYYMMDD`로 내보냄 — 실패하면 제거를 중단
2. 감시자·단축키·엔진 정지
3. 로그인 자동 실행 해제 후 OS 상태 재확인(실패 시 보고서에 표시)
4. 임시 폴더에 일회성 정리 스크립트를 만들고 실행 → AutoDesk 종료
5. 스크립트: 이 PID가 끝날 때까지 최대 30초 대기 → 앱 식별자를 포함한 데이터 폴더만 삭제 → 자동 실행 잔여 항목 확인
   → Windows는 `uninstall.exe /S`, macOS는 `AutoDesk.app` 번들 삭제 → 결과 보고서(`%TEMP%`/`$TMPDIR`의 `AutoDesk-uninstall-report.txt`)를 열고 → 스크립트 자신 삭제

지우지 않는 것: 감시 폴더의 원본 파일, 다운로드 폴더와 분류된 파일, WebView2/WebKit 같은 공용 런타임, 다른 앱의 등록·데이터.

## Windows: 설정 → 앱 → AutoDesk 제거

NSIS 제거 프로그램 + `src-tauri/windows/hooks.nsh`:

- `NSIS_HOOK_PREUNINSTALL`: `AutoDesk.exe --quit`로 실행 중인 앱에 정상 종료 요청 → 2초 후에도 남아 있으면 **설치 폴더의 AutoDesk.exe 경로와 정확히 일치하는 프로세스만** 종료 → `HKCU\…\Run`과 `…\Explorer\StartupApproved\Run`의 `AutoDesk` 값만 삭제
- 앱 데이터(설정·DB·백업)는 제거 화면의 **"앱 데이터 삭제" 체크박스**(Tauri 기본 제공)로 선택. 체크하지 않으면 데이터가 남는다 → 깨끗하게 지우려면 앱 안의 완전 제거를 쓰거나 체크박스를 선택한다.
- 설치는 현재 사용자 범위(`%LOCALAPPDATA%\AutoDesk`), 관리자 권한 불필요.

## macOS: Finder에서 휴지통으로 옮기는 경우 (제한)

Finder에서 `AutoDesk.app`을 휴지통으로 옮기는 것만으로는 **정리 코드가 실행되지 않는다**. 이 경우 남을 수 있는 것:

```text
~/Library/LaunchAgents/AutoDesk.plist                 (로그인 자동 실행을 켰던 경우)
~/Library/Application Support/io.github.logicphantom.autodesk
~/Library/Caches/io.github.logicphantom.autodesk
~/Library/WebKit/io.github.logicphantom.autodesk
~/Library/Logs/io.github.logicphantom.autodesk
```

LaunchAgent가 남아 있어도 실행 파일이 없으므로 다시 실행되지는 않지만, 깨끗한 정리를 위해 **앱 안의 완전 제거를 공식 방법으로 사용**한다. 수동 정리:

```bash
launchctl unload ~/Library/LaunchAgents/AutoDesk.plist 2>/dev/null; rm -f ~/Library/LaunchAgents/AutoDesk.plist
rm -rf ~/Library/{"Application Support",Caches,WebKit,Logs}/io.github.logicphantom.autodesk
```

## 제거 검증 체크리스트

| 항목 | Windows | macOS |
|---|---|---|
| 실행 중 제거 시 정상 종료 | 확인 필요(설치본) | NOT_VERIFIED_ON_MACOS |
| 자동 실행 ON 상태에서 제거 → 등록 삭제 | 확인 필요(설치본) | NOT_VERIFIED_ON_MACOS |
| 재로그인 후 프로세스 재실행 없음 | 확인 필요 | NOT_VERIFIED_ON_MACOS |
| 데이터 폴더가 선택대로 정리 | 확인 필요 | NOT_VERIFIED_ON_MACOS |
| 원본 파일 보존 | 코드상 대상 아님(식별자 포함 경로만 삭제) | 동일 |
| 실패 항목 보고 | 보고서 파일 | 보고서 파일 |

실제 결과는 [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md)에 갱신한다.

확인 명령(Windows, PowerShell):

```powershell
Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run | Select-Object AutoDesk
Get-Process AutoDesk -ErrorAction SilentlyContinue
Test-Path "$env:APPDATA\io.github.logicphantom.autodesk"
```
