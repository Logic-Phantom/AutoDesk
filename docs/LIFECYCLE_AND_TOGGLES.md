# 실행 상태와 ON/OFF

네 가지 스위치는 서로 독립이며, 모두 실제 OS 등록·감시자·단축키 상태와 연결된다.

| 스위치 | 저장 | 실제로 바뀌는 것 |
|---|---|---|
| 전체 자동화(Master) | `app_settings.master_enabled` | OFF: 모든 감시자 drop, 단축키 해제, 대기 작업 `QUEUED → PAUSED_GLOBAL`. ON: 다시 등록 후 대기 작업 재처리 |
| 백그라운드 상주 | `app_settings.background_mode_enabled` | 창 닫기 동작(숨김 vs 종료). OFF로 바꾸면 로그인 자동 실행도 함께 해제(확인 후) |
| 로그인 시 자동 실행 | OS 등록이 진실 | Windows `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 값 `AutoDesk`, macOS `~/Library/LaunchAgents/AutoDesk.plist`. 켠 뒤 `is_enabled()`로 다시 읽어 실제로 반영됐을 때만 성공. 화면 표시는 매번 OS 상태를 조회 |
| 모듈별 | `automation_modules.enabled` | 해당 모듈의 감시 경로/단축키만 해제. 대기 작업 `QUEUED → PAUSED_MODULE`. 다른 모듈은 계속 |

백그라운드 상주가 꺼진 상태에서 로그인 자동 실행을 켜려 하면 "백그라운드도 함께 켤까요?"를 묻고, 거절하면 설정하지 않는다.

## 상태 모델

앱: `APP_STARTING · FOREGROUND_ACTIVE · BACKGROUND_ACTIVE · GLOBAL_PAUSED · SHUTTING_DOWN · UNINSTALLING`
(창 보임 여부와 Master 값에서 Core가 계산한다. `ERROR_RECOVERY`는 별도 상태 대신 시작 시 복구 루틴으로 처리)

모듈: `DISABLED · RUNNING · PAUSED_GLOBAL · WAITING_PERMISSION(설정 필요/폴더 접근 불가) · ERROR_PAUSED · STOPPING`
(`STARTING`은 동기 등록이라 화면에 나타날 틈이 없어 생략)

작업: `QUEUED · RUNNING · WAITING_APPROVAL · PAUSED_GLOBAL · PAUSED_MODULE · COMPLETED · SKIPPED · CANCELLED · FAILED`
(`DETECTED`는 debounce 대기열(메모리), `RETRY_WAIT`는 다음 복구 시 재시도로 대체)

모든 전환은 `core.rs`에서만 일어난다. React는 boolean을 직접 바꾸지 않고 명령을 보낸 뒤 `core://changed`를 받아 다시 그린다.

## 트레이 메뉴

AutoDesk 열기 · 전체 자동화(체크) · 백그라운드 상태 · 모듈 3개 빠른 ON/OFF(체크) · 확인 대기 N건 · 최근 처리 내역 · 설정 · 프로그램 종료.
상태가 바뀔 때마다 Core 값으로 메뉴를 다시 만든다.
