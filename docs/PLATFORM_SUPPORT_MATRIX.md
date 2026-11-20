# 플랫폼 지원표

같은 코드베이스에서 Windows 10/11(x64)와 macOS 11+(Apple Silicon/Intel, CI 러너 기준 arm64)를 빌드한다.
OS 차이는 `platform.rs`, `uninstall.rs`(cfg 분기), `ai.rs`(CLI 탐색 경로), `hooks.nsh`(Windows 전용)에만 있다.

| 기능 | Windows | macOS | 비고 |
|---|---|---|---|
| 트레이/메뉴 막대 | ✅ | ✅(빌드) | macOS는 Dock 아이콘도 유지 |
| 창 닫기 → 트레이 상주 / 종료 | ✅ | ✅(빌드) | |
| 로그인 자동 실행 | HKCU Run 값 `AutoDesk` | `~/Library/LaunchAgents/AutoDesk.plist` | 관리자 권한 불필요 |
| 파일 감시 | ReadDirectoryChangesW | FSEvents | notify 크레이트 |
| 다운로드 폴더 탐지 | Known Folder API | `~/Downloads` | Tauri path API |
| 전역 단축키 | Ctrl+Alt+Shift+V | ⌘⌥⇧V | macOS는 손쉬운 사용 권한이 필요할 수 있음 |
| 클립보드 텍스트 | ✅ | ✅ | |
| 업무 모드 플로팅 버튼 | 투명 창 ✅ | 투명 창(macOSPrivateApi) 빌드만 | 작업 영역 오른쪽 아래 고정 |
| 업무 모드 — 프로그램 닫기 | 최상위 창에 `WM_CLOSE` | `osascript … quit` | 강제 종료는 옵션(Windows `TerminateProcess`, macOS `pkill`) |
| 업무 모드 — 화면 잠금 / 모니터 끄기 | `LockWorkStation` / `SC_MONITORPOWER` | `pmset displaysleepnow` | |
| 업무 모드 — PC 종료 예약 | `shutdown /s /t 60`, 취소 `/a` | 미지원 | |
| 화면 캡처(영역·전체·창) | ✅ xcap | 빌드만(화면 기록 권한 필요) | |
| OCR | Windows.Media.Ocr ✅ | Vision(osascript JXA) — CI에서 실제 인식 테스트 | Windows는 언어팩의 광학 문자 인식, macOS는 13+에서 한국어 |
| 일일 요약(집계·저장·정시 알림) | ✅ | ✅(공통 코드) | |
| 표 변환 · 문서 비교 | ✅ | ✅(웹뷰 공통 코드) | |
| 이미지 변환 | ✅ | ✅(빌드) | WebP는 무손실 출력만 |
| 알림 | 설치본에서만 정상(개발 실행은 PowerShell 이름) | 첫 실행 시 권한 요청 | |
| 폴더 접근 | 제한 없음 | 문서/데스크톱/다운로드 접근 시 OS 권한 창 | 거부되면 해당 모듈만 `설정 필요` |
| 설치 형식 | NSIS(현재 사용자) | .app / .dmg | 둘 다 서명 안 함 |
| 앱 내 완전 제거 | PowerShell 일회성 스크립트 + `uninstall.exe /S` | bash 일회성 스크립트 + 번들 삭제 | |
| OS 제거 경로 | 설정 → 앱 → 제거(hooks.nsh) | Finder 휴지통은 정리 안 됨(문서화) | |
| Claude Code 다듬기 | `claude.exe`/`claude.cmd` 탐색 | `claude` 탐색(+Homebrew 경로) | |

검증 수준은 [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md). macOS 실기기 검증 전 항목은 `NOT_VERIFIED_ON_MACOS`.
