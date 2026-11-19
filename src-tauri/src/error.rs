//! 공통 오류 타입. 내부 오류 코드(code)와 사용자용 한국어 메시지(message)를 분리한다.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("NOT_FOUND", message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("DB_ERROR", format!("데이터베이스 오류: {e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        let code = match e.kind() {
            NotFound => "FILE_NOT_FOUND",
            PermissionDenied => "PERMISSION_DENIED",
            AlreadyExists => "ALREADY_EXISTS",
            StorageFull => "DISK_FULL",
            _ => "IO_ERROR",
        };
        let msg = match code {
            "FILE_NOT_FOUND" => "파일을 찾을 수 없습니다".to_string(),
            "PERMISSION_DENIED" => "파일 접근 권한이 없습니다(다른 프로그램이 사용 중일 수 있음)".to_string(),
            "ALREADY_EXISTS" => "같은 이름의 파일이 이미 있습니다".to_string(),
            "DISK_FULL" => "디스크 공간이 부족합니다".to_string(),
            _ => format!("파일 작업 오류: {e}"),
        };
        Self::new(code, msg)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("JSON_ERROR", format!("설정 형식 오류: {e}"))
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::new("TAURI_ERROR", format!("앱 내부 오류: {e}"))
    }
}
