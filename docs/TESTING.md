# 테스트

## 자동 테스트

```bash
npm run test:rust   # Rust 44개
npm test            # Vitest 21개
npm run typecheck && npm run lint
```

| 영역 | 테스트 |
|---|---|
| DB | 마이그레이션·설정 왕복, 작업 idempotency(중복 병합), 일시정지/재개 전환·시도 횟수, 모듈 연속 오류 카운터 |
| fsutil | 제외 패턴, 이름 충돌(`a (3).txt`), 덮어쓰지 않는 이동, 경로 탈출(`..`·상위 폴더), 감시 루트 검증(앱 데이터 겹침) |
| AutoBackup | 후보 필터, 실제 변경시에만 스냅샷(같은 내용 재저장 무시)·원본 불변·manifest, 보관 개수, 크기 제한, 텍스트 diff |
| Download Butler | 확장자 규칙, 다운로드 중 임시 파일/숨김 파일 무시, 시그니처 판별·확장자 불일치 |
| AutoPaste | 한글·영문 줄 끝 공백, NBSP·U+200B, 이모지 ZWJ 보존, 빈 줄 축소, 공백만 있는 줄, 코드 들여쓰기 보존, URL·경로 불변, CRLF 유지/변환, HTML 옵션, 빈/깨끗한 입력 |
| 업무 모드 | 따옴표 인자 분리, 보호 프로세스(explorer·AutoDesk) 거부, `javascript:`/`file:` 주소 거부 |
| 도구(Rust) | DOCX 본문 추출, 저장 확장자 제한·BOM, 바이너리 파일 거부, 이미지 PNG→JPG 원본 보존, 리사이즈 모드 5종, 용량 제한 품질 탐색, 이름 충돌, OCR 2배 확대, 한글 글자 간 공백 붙이기 |
| Table Converter | CSV 따옴표·줄바꿈, 형식 자동 감지, 엑셀 TSV→Markdown(파이프 이스케이프), CSV→JSON 타입·앞자리 0 보존, JSON 키 합집합, HTML colspan, TSV↔CSV, Markdown 역변환 |
| Compare Desk | 같음/수정/추가/삭제 분류, 삭제·수정 혼합 시 최적 짝짓기, 공백·대소문자 무시, 글자 단위 강조, CSV 키 열 매칭, 리포트 이스케이프 |
| AI | 응답 검증(빈 응답·과도한 길이 거부) |
| 제거 | 삭제 대상 경로는 앱 식별자를 포함해야 함 |
| UI | 형식 함수(바이트·시간·상태 색·단축키 표기) |

## 수동 QA 체크리스트(설치본)

- [ ] 설치 → 실행 → 트레이 아이콘 표시
- [ ] 창 닫기(백그라운드 ON) → 트레이 유지, 알림 1회
- [ ] 백그라운드 OFF → 창 닫기 → 작업 관리자에 `AutoDesk.exe` 없음
- [ ] 전체 자동화 OFF → 감시 폴더 파일 저장해도 버전 안 생김 → ON 후 저장하면 생김
- [ ] AutoBackup 모듈만 OFF → Download Butler는 계속 제안 생성
- [ ] 로그인 자동 실행 ON → `HKCU\…\Run`에 `AutoDesk` 값 → 재로그인 시 트레이로 시작 → OFF → 값 삭제
- [ ] 다운로드 폴더에 PDF 복사 → 승인 센터에 제안 → 이동 → 이력에서 되돌리기
- [ ] 텍스트 복사 → 단축키 → 정리 알림 → DB/로그에 원문 없음
- [ ] 설정 → 완전 제거 → 보고서 확인, 데이터 폴더·Run 값·설치 폴더 없음, 원본 파일 그대로
- [ ] (macOS) 위 항목 반복 — 미검증 시 `NOT_VERIFIED_ON_MACOS`

결과는 [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md)에 기록한다.
