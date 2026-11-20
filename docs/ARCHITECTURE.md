# 아키텍처

## 한눈에 보기

```text
React UI (src/)                         ← 상태를 소유하지 않음. 명령만 보내고 이벤트로 다시 읽음
  Dashboard · Automations · Activity · Approval Center · Settings
        │  invoke(...)            ▲ emit("core://changed"), emit("core://navigate")
        ▼                         │
Rust Core (src-tauri/src/)  ── 프로세스 1개, 관리자 권한 없음, 서비스/데몬 없음
  lib.rs         앱 진입점: 플러그인, 창, 닫기 정책, 종료 처리
  core.rs        Lifecycle · 전역/모듈 스위치 · 공유 Watch Service · 엔진 스레드 · 작업 기록
  shortcuts.rs   전역 단축키 등록/해제(클립보드 정리, AI 다듬기)
  tray.rs        트레이/메뉴 막대(체크 상태는 Core에서 다시 그림)
  commands.rs    Tauri 명령(입력 검증)
  db.rs          SQLite(마이그레이션 v3) — settings(업무 모드·도구 설정 포함), modules, watch_targets, jobs, activity, file_versions, file_moves
                 (v2의 LogWatcher 테이블은 v3에서 삭제)
  fsutil.rs      SHA-256, 안정성 확인, 충돌 없는 이름, 안전 이동, 경로 검증, 제외 패턴
  modules/       backup.rs · downloads.rs · autopaste.rs (순수 로직 + 테스트)
  workspace.rs   업무 모드: 실행(opener)·정상 종료 요청(WM_CLOSE / osascript quit)·잠금·종료 예약, 보호 프로세스 목록
  tools/         capture(xcap 캡처, 오버레이/편집 창, 메모리 보관+세대 번호) · ocr(Windows.Media.Ocr) · images(일괄 변환) · files(DOCX 추출, 저장 확장자 제한)
  ai.rs          (선택) Claude Code CLI 호출
  platform.rs    OS 어댑터: autostart, 설치 위치, 앱 소유 경로
  uninstall.rs   완전 제거(미리보기 + 일회성 정리 도우미)
        │
        ├─ notify (Windows ReadDirectoryChangesW / macOS FSEvents)
        ├─ tauri-plugin-autostart (HKCU Run / LaunchAgent)
        ├─ tauri-plugin-global-shortcut · clipboard-manager · notification · dialog · single-instance
        └─ NSIS 설치 프로그램 + hooks.nsh (Windows), .app/.dmg (macOS)
```

## 이벤트 파이프라인

```text
OS 파일 이벤트 (Create/Modify)
  → notify 콜백 → mpsc 채널 → 엔진 스레드
  → debounce 1.5초(같은 경로 이벤트 병합, 대기열 상한 5000)
  → 전역 스위치 최종 확인(OFF면 버림)
  → route(): 자기 자신이 만든 변경(self-write) 제외 → 구독 모듈 판별(모듈 OFF면 Core가 차단)
  → enqueue(): idempotency key = sha256(모듈|경로|크기|수정시각) → jobs에 INSERT OR IGNORE(중복 병합)
  → run_job(): RUNNING → 모듈 처리(안정성 재확인, 해시 비교, 이동/스냅샷) → COMPLETED/SKIPPED/WAITING_APPROVAL/FAILED
  → activity 기록 + 알림 + core://changed
```

- **공유 감시자**: 경로마다 OS 감시는 한 번만 건다. 자동 백업과 다운로드 정리가 같은 폴더를 보면 하나의 감시를 공유하고 라우팅으로 나눈다.
- **전역 OFF/모듈 OFF**: 원하는 감시 목록을 다시 계산해 필요 없는 경로는 `unwatch`, 대상이 없으면 감시자 자체를 drop한다. UI만 바뀌는 토글은 없다.
- **격리**: 모듈 처리는 `catch_unwind`로 감싸 한 모듈의 panic이 엔진을 멈추지 않는다. 같은 모듈이 연속 5회 실패하면 그 모듈만 `ERROR_PAUSED`.
- **복구**: 앱 시작·전역 ON·모듈 ON 시 `QUEUED/RUNNING` 작업을 다시 처리(최대 3회). 남은 `*.tmp-snapshot` 파일 삭제.
- **동시성**: MVP는 엔진 스레드 1개에서 순차 처리한다(같은 파일 경합·순서 문제 없음). 단축키 작업만 별도 스레드.

## 생명주기

[LIFECYCLE_AND_TOGGLES.md](LIFECYCLE_AND_TOGGLES.md) 참고. 요약:

| 동작 | 결과 |
|---|---|
| 창 닫기 + 백그라운드 ON | 창만 숨김(트레이 상주), 자동화 계속 |
| 창 닫기 + 백그라운드 OFF | `Core::shutdown()` → 앱 종료 |
| 트레이 → 프로그램 종료 | `Core::shutdown()` → 앱 종료 |
| 두 번째 실행 | single-instance가 기존 창을 띄우고 새 프로세스는 즉시 종료 |
| `AutoDesk.exe --quit` | 실행 중 인스턴스에 정상 종료 요청(설치 제거 프로그램이 사용) |
| 로그인 자동 실행 | `--autostart` 인자로 창 없이 트레이로 시작 |

`Core::shutdown()`: 신규 이벤트 차단 → 감시자 drop → 단축키 전부 해제 → 엔진 스레드 정지(최대 5초 대기) → WAL 체크포인트. 프로세스가 끝나면 남는 스레드/프로세스가 없다.

## 데이터 위치

모두 Tauri path API의 `app_data_dir()` 아래(식별자 `io.github.logicphantom.autodesk`):

```text
<app_data_dir>/
  database/autodesk.db   SQLite(WAL)
  backups/<watch-target-id>/<경로해시16>/  *.snap + manifest.json(DB 없이도 원본 경로 식별)
  logs/autodesk.log      JSON lines, 1MB 회전, 개인 데이터·클립보드 내용 없음
  temp/                  AI 작업용 빈 폴더 등
```

Windows: `%APPDATA%\io.github.logicphantom.autodesk` (WebView 캐시는 `%LOCALAPPDATA%\…`), macOS: `~/Library/Application Support/io.github.logicphantom.autodesk`.

## 연동 방식(다른 개인 프로젝트와의 차이)

AutoMateHQ·JARVIS는 GitHub Actions/Cloudflare Workers에서 도는 **서버형** 자동화다. AutoDesk는 **내 PC 안에서만** 도는 로컬 앱이라 서버·토큰·Secret이 없다. 대신 같은 원칙을 이어받았다.

- 데이터가 없으면 꾸며내지 않는다(통계·이력은 DB 실기록만, 미구현 모듈은 UI에 노출 안 함).
- AI는 기본 꺼짐, 켜도 결과 검증 후 적용(비거나 지나치게 길면 버림).
- 비밀 값은 저장소·로그에 남기지 않는다(애초에 저장할 비밀 값이 없음).

## 창 구성(v0.3.0)

같은 프런트 번들을 URL 해시로 나눠 여러 창이 공유한다.

| 창(label) | 해시 | 만들어지는 때 | 비고 |
|---|---|---|---|
| main | (없음, `?page=` 로 첫 화면 지정) | 앱 시작·트레이/독에서 열 때 | 닫기 = 창 해제(백그라운드 ON이면 앱은 계속, WebView 메모리 반환). 로그인 자동 실행 때는 만들지 않음 |
| dock | `#/dock` | 업무 모드 '플로팅 버튼 표시' ON | 투명·항상 위·작업 표시줄 제외, 내용 크기에 맞춰 오른쪽 아래 기준으로 크기 조절. Alt+F4 등 닫기 요청은 무시하고 설정에서 끌 때만 destroy |
| overlay | `#/overlay` | 영역 캡처 | 커서가 있는 모니터 전체를 덮음(물리 픽셀 위치·크기), 선택 후 닫힘 |
| editor | `#/editor` | 캡처 완료 | 이미 열려 있으면 재사용(`capture://updated`). 창이 사라지면(Rust `Destroyed` 이벤트) 그 편집기가 보던 세대의 캡처만 삭제 — JS `onCloseRequested`는 destroy 권한이 필요해 쓰지 않음 |

- Windows에서는 **동기 명령(메인 스레드)에서 창을 만들면 교착**되므로 창을 만들거나 닫는 명령은 모두 async(필요 시 `spawn_blocking`)로 둔다.
- 캡처 PNG는 `CaptureStore{png, gen}`(메모리)에만 있고 IPC 원시 바이트(`ipc::Response`)로 전달한다. 디스크 임시 파일 없음.
- 도구는 사용자가 열 때만 동작한다. 백그라운드 엔진(감시·작업 큐)과 분리돼 CPU를 쓰지 않는다. 캡처 단축키만 전역 등록(전체 자동화 스위치와 무관, 설정에서 끔).
