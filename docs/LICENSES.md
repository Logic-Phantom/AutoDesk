# 라이선스

AutoDesk 자체 코드: **MIT** ([LICENSE](../LICENSE)). 개인용이며 판매·배포 계획 없음.

## 직접 의존성

| 패키지 | 용도 | 라이선스 |
|---|---|---|
| tauri, tauri-build, tauri-plugin-{autostart, global-shortcut, clipboard-manager, notification, dialog, single-instance} | 데스크톱 런타임·OS 기능 | MIT OR Apache-2.0 |
| rusqlite (bundled SQLite) | 로컬 DB | MIT (SQLite는 Public Domain) |
| notify | 파일 감시 | CC0-1.0 |
| sha2, hex, uuid, chrono, serde, serde_json | 해시·ID·시간·직렬화 | MIT OR Apache-2.0 |
| similar | 텍스트 diff | Apache-2.0 |
| encoding_rs | LogWatcher의 EUC-KR 로그 해석 | (Apache-2.0 OR MIT) AND BSD-3-Clause |
| react, react-dom | UI | MIT |
| lucide-react | 아이콘 | ISC |
| @tauri-apps/api, @tauri-apps/plugin-dialog | 프런트-코어 연결 | MIT OR Apache-2.0 |
| vite, vitest, typescript, eslint 등(개발 전용) | 빌드·테스트 | MIT / Apache-2.0 / BSD |

## 전체 의존성 점검 (2026-11-19, `cargo metadata` + node_modules 집계)

- Rust 약 560개: 대부분 MIT / Apache-2.0 / Zlib / BSD / ISC / Unicode-3.0 / BSL-1.0.
- **MPL-2.0**(파일 단위 약한 copyleft): `cssparser`, `selectors`, `dtoa-short`, `option-ext` — Tauri/WebView 쪽 간접 의존성. 해당 파일을 수정해 배포할 때만 소스 공개 의무가 있고, 수정하지 않고 쓰는 것은 자유롭다.
- `r-efi`: MIT OR Apache-2.0 OR LGPL 중 선택 → MIT로 사용.
- npm: MIT·ISC·BSD·Apache 위주, `CC-BY-4.0`(caniuse-lite 데이터, 개발 전용), `BlueOak-1.0.0`(허용형), `MPL-2.0` 2개(개발 도구).
- GPL 등 강한 copyleft 없음. 모두 무료이며 개인 사용·수정에 제약 없음.

## 빌드·배포 도구

| 도구 | 라이선스/조건 |
|---|---|
| Rust toolchain | MIT/Apache-2.0 |
| Node.js | MIT |
| Visual Studio 2022 Build Tools | 무료(개인·오픈소스 개발 사용 허용) |
| NSIS(Tauri가 내려받음) | zlib |
| GitHub Actions | 공개 저장소 무료 |
| WebView2 Runtime(Windows 기본 포함) | Microsoft 무료 재배포 |

의존성을 추가하면 이 문서를 갱신한다.
