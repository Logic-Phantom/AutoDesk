# 빌드·배포

## 준비(Windows)

```powershell
winget install Rustlang.Rustup
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
npm install
```

macOS: `xcode-select --install`, rustup, Node 22+.

## 명령

| 명령 | 내용 |
|---|---|
| `npm run tauri:dev` | 개발 실행(Vite 1430 포트 + Rust 디버그) |
| `npm run tauri:build` | 릴리스 빌드 + 설치 파일 |
| `npm run typecheck` / `npm run lint` / `npm test` | 프런트 검사 |
| `npm run test:rust` | Rust 단위·통합 테스트 |

산출물:

- Windows: `src-tauri/target/release/bundle/nsis/AutoDesk_<버전>_x64-setup.exe`
- macOS: `src-tauri/target/release/bundle/dmg/AutoDesk_<버전>_aarch64.dmg`

## CI

`.github/workflows/ci.yml` — push/PR마다 Windows·macOS 러너에서 typecheck·lint·vitest·cargo test·tauri build, 설치 파일을 artifact로 업로드. `v*` 태그를 푸시하면 GitHub Release에 첨부.

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## 서명

인증서가 없으므로 서명하지 않는다.

- Windows: 설치 시 SmartScreen "Windows의 PC 보호" → **추가 정보 → 실행**.
- macOS: 처음 열 때 차단되면 Finder에서 앱을 **우클릭 → 열기**, 또는 `xattr -dr com.apple.quarantine /Applications/AutoDesk.app`.

업데이트 서버(updater 플러그인)는 서명 키가 필요해 사용하지 않는다. 새 버전은 설치 파일을 다시 실행하면 된다(설치 전에 실행 중인 앱을 `--quit`로 종료).
