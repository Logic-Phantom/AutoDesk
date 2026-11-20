//! 자동화 모듈 레지스트리. MVP는 Rust에 컴파일된 고정 모듈만 사용한다(동적 플러그인 로딩 없음).

pub mod autopaste;
pub mod backup;
pub mod downloads;

use serde::Serialize;

pub const BACKUP: &str = "backup";
pub const DOWNLOADS: &str = "downloads";
pub const AUTOPASTE: &str = "autopaste";

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
}

pub const REGISTRY: &[ModuleMeta] = &[
    ModuleMeta {
        id: BACKUP,
        name: "자동 백업",
        description: "등록한 폴더의 파일이 바뀌면 이전 버전을 자동 보관합니다. 원본은 건드리지 않습니다.",
        default_enabled: true,
    },
    ModuleMeta {
        id: DOWNLOADS,
        name: "다운로드 정리",
        description: "다운로드 폴더에 새 파일이 들어오면 종류별 폴더로 분류합니다. 기본은 미리보기(승인 후 이동)입니다.",
        default_enabled: true,
    },
    ModuleMeta {
        id: AUTOPASTE,
        name: "클립보드 정리",
        description: "단축키를 누르면 클립보드 텍스트의 불필요한 공백·특수문자를 정리합니다. 원문은 저장하지 않습니다.",
        default_enabled: true,
    },
];

pub fn meta(id: &str) -> Option<&'static ModuleMeta> {
    REGISTRY.iter().find(|m| m.id == id)
}

pub fn default_config(id: &str) -> String {
    match id {
        BACKUP => serde_json::to_string(&backup::BackupConfig::default()),
        DOWNLOADS => serde_json::to_string(&downloads::DownloadsConfig::default()),
        AUTOPASTE => serde_json::to_string(&autopaste::AutoPasteConfig::default()),
        _ => Ok("{}".into()),
    }
    .unwrap_or_else(|_| "{}".into())
}
