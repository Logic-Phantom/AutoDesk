# 구현 상태 (2026-11-19, v0.1.0)

검증 환경: Windows 11 Home 10.0.26200 x64, Rust 1.100, Node 24, Tauri 2.12, 릴리스 빌드 + NSIS 설치본(2.63 MiB).
앱 명령은 설치본을 WebView2 원격 디버깅 포트로 띄워 실제 `invoke`로 호출해 확인했다(테스트 전용, 앱 코드에는 포함 안 됨).

## Windows — 실제 확인함

| 항목 | 결과 |
|---|---|
| 첫 실행 시 데이터 폴더·DB·로그 생성, `--autostart`로 창 없이 시작 | ✅ |
| 유휴 메모리 | 약 34 MB(작업 관리자 Working Set, 측정 1회) |
| 전체 자동화 OFF → OS 감시 2→0개, ON → 2개 복구 | ✅ |
| 모듈 하나 OFF(Download Butler) → 감시 2→1개, 나머지 계속 | ✅ |
| 로그인 자동 실행 ON → `HKCU\…\Run\AutoDesk = "…\AutoDesk.exe" --autostart`, OFF → 값 삭제(실제 상태 재조회) | ✅ |
| 위험 경로(드라이브 루트) 감시 등록 거부 | ✅ |
| Download Butler: 다운로드 폴더에 PDF 생성 → 분류 → 승인 대기(미리보기 모드, 이동 안 함) | ✅ |
| AutoBackup: 실제 내용 변경 2회 → 스냅샷 2개 + manifest, 같은 내용 재저장·`~$` 파일 무시, 원본 불변 | ✅ |
| AutoPaste: 전역 단축키 → 줄 끝 공백·빈 줄 정리, CRLF 유지, 이력엔 "4곳 정리"만, 로그에 원문 없음 | ✅ |
| `AutoDesk.exe --quit` → 실행 중 인스턴스 정상 종료(로그 "정상 종료"), 프로세스 없음 | ✅ |
| NSIS 제거(`uninstall.exe /S`) → 실행 중 앱 정상 종료, Run 값 삭제, 설치 폴더·시작 메뉴·제거 등록 삭제 | ✅ (데이터는 제거 화면 체크박스 선택 시 삭제 — Tauri 템플릿 확인) |
| 앱 내 완전 제거(자동 실행 ON 상태) → 프로세스 종료, Run·StartupApproved 값, Roaming·Local 데이터, 설치본 삭제, 보고서 생성, 도우미 자기 삭제, 감시 폴더 원본 보존 | ✅ |
| 확인 문구 불일치 시 제거 거부 | ✅ |
| 실행 중인 앱 위에 업데이트 설치 → 기존 앱 정상 종료, 설정·백업 유지 | ✅ |
| 백업 버전 비교(텍스트 diff) 화면 | ✅ (스크린샷 확인) |
| 승인 센터에서 이동 승인 → 하위 폴더로 이동 | ✅ |
| CI(GitHub Actions) Windows·macOS 빌드 | ✅ |

검증 중 발견·수정한 버그

1. 백업 라우팅에서 `for` 식의 임시 DB 잠금이 루프 내내 유지돼 같은 스레드에서 재잠금 → 엔진 교착. 목록을 먼저 꺼내도록 수정하고, 종료 경로는 `try_lock`으로 바꿔 엔진이 멈춰도 종료가 막히지 않게 함.
2. 완전 제거 도우미가 부모의 Job 객체에 묶여 앱 종료와 함께 사라짐 → `CREATE_BREAKAWAY_FROM_JOB`(불가 시 일반 실행)으로 수정.
3. 백업 비교(diff) 응답의 필드 이름이 프런트 계약과 달라 "NaN KB"로 표시 → `rename_all_fields`로 수정, 계약 테스트 추가.

## Windows — 아직 직접 확인하지 않음

- 재부팅/재로그인 후 실제 자동 시작(등록 값은 확인)
- 트레이 메뉴 클릭, 창 닫기 → 트레이 상주 알림(코드 경로는 `CloseRequested` 처리)
- Download Butler 되돌리기 실행, 자동(AUTO) 모드, 다른 드라이브로의 이동
- 백업 복원(사본/덮어쓰기) 실행, 용량 제한 정리(단위 테스트로만 확인)
- Claude Code 다듬기(이 PC에 `claude` CLI가 PATH에 없음 — 설치 후 확인 필요)
- 화면은 전 페이지를 스크린샷으로 점검함(docs/guide/images). 창 크기 변경·고배율 DPI는 미확인

## macOS — NOT_VERIFIED_ON_MACOS

코드와 CI(macos-latest) 빌드는 준비됨. 실기기에서 트레이, LaunchAgent 등록/해제, FSEvents 감시, 단축키(손쉬운 사용 권한), 알림 권한, 앱 내 완전 제거(bash 도우미), Gatekeeper 우회 절차 모두 미검증.

## 자동 테스트

Rust 30개 통과, Vitest 4개 통과, TypeScript strict typecheck·ESLint 통과.

## 다음 작업

1. 설치본으로 남은 Windows 수동 QA([TESTING.md](TESTING.md) 체크리스트)
2. macOS 실기기 QA
3. Claude Code CLI 설치 후 AI 다듬기 실사용 확인
4. 후속 모듈 중 하나(LogWatcher 또는 Daily Digest) 설계·구현
