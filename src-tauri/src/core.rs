//! Application Core: 생명주기(Lifecycle), 전역/모듈 스위치, 공유 파일 감시, 이벤트 처리, 작업 기록.
//!
//! 모든 상태 전환은 이 모듈 한 곳에서 일어난다. UI는 명령만 보내고 `core://changed` 이벤트로 갱신한다.
//! 백그라운드 작업은 이 앱 프로세스 안의 스레드로만 돈다(별도 서비스/데몬 없음).
//! 앱이 종료되면 감시자·단축키·엔진 스레드가 함께 정리된다.

use crate::db::{self, WatchTarget};
use crate::error::{AppError, AppResult};
use crate::fsutil;
use crate::modules::{self, autopaste::AutoPasteConfig, backup, downloads, logwatch};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// 파일 이벤트가 이 시간 동안 조용해야 처리한다(debounce).
const DEBOUNCE: Duration = Duration::from_millis(1500);
/// 연속 오류가 이만큼 쌓이면 해당 모듈만 ERROR_PAUSED로 전환한다.
const MAX_CONSECUTIVE_ERRORS: i64 = 5;
/// debounce 대기열 상한(이벤트 폭주 방지).
const MAX_PENDING: usize = 5000;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data: PathBuf,
    pub db: PathBuf,
    pub backups: PathBuf,
    pub logs: PathBuf,
    pub temp: PathBuf,
}

impl AppPaths {
    pub fn from_app(app: &AppHandle) -> AppResult<Self> {
        let data = app.path().app_data_dir().map_err(|e| AppError::new("PATH_ERROR", e.to_string()))?;
        let p = Self {
            db: data.join("database").join("autodesk.db"),
            backups: data.join("backups"),
            logs: data.join("logs"),
            temp: data.join("temp"),
            data,
        };
        for d in [p.db.parent().unwrap_or(&p.data), &p.backups, &p.logs, &p.temp] {
            std::fs::create_dir_all(d)?;
        }
        Ok(p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppStatus {
    AppStarting,
    ForegroundActive,
    BackgroundActive,
    GlobalPaused,
    ShuttingDown,
    Uninstalling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModuleStatus {
    Disabled,
    Running,
    PausedGlobal,
    WaitingPermission,
    ErrorPaused,
    Stopping,
}

impl ModuleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "DISABLED",
            Self::Running => "RUNNING",
            Self::PausedGlobal => "PAUSED_GLOBAL",
            Self::WaitingPermission => "WAITING_PERMISSION",
            Self::ErrorPaused => "ERROR_PAUSED",
            Self::Stopping => "STOPPING",
        }
    }
}

/// 메모리 상의 실행 상태. 영속 설정은 DB(app_settings)에 있다.
#[derive(Debug, Clone)]
pub struct Runtime {
    pub master: bool,
    pub background: bool,
    pub starting: bool,
    pub shutting_down: bool,
    pub uninstalling: bool,
    pub window_visible: bool,
    pub shortcut_error: Option<String>,
    pub ai_shortcut_error: Option<String>,
    pub tray_hint_shown: bool,
}

enum EngineMsg {
    Fs(PathBuf),
    Recover,
    Stop,
}

#[derive(Default)]
struct WatchService {
    watcher: Option<RecommendedWatcher>,
    active: BTreeMap<PathBuf, bool>, // path -> recursive
}

pub struct Core {
    pub app: AppHandle,
    pub paths: AppPaths,
    db: Mutex<Connection>,
    rt: Mutex<Runtime>,
    watch: Mutex<WatchService>,
    /// 앱이 직접 만든 파일 변경(Undo 이동 등)은 다시 처리하지 않는다.
    self_writes: Mutex<HashMap<PathBuf, Instant>>,
    engine_tx: Mutex<Option<Sender<EngineMsg>>>,
    engine_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // 한 스레드의 panic이 전체를 멈추지 않도록 poison을 무시한다.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleView {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub enabled: bool,
    pub status: ModuleStatus,
    pub status_reason: Option<String>,
    pub last_run_at: Option<String>,
    pub last_error_code: Option<String>,
    pub today_count: i64,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub status: AppStatus,
    pub master_enabled: bool,
    pub background_mode: bool,
    pub launch_at_login: bool,
    pub launch_at_login_error: Option<String>,
    pub modules: Vec<ModuleView>,
    pub today_completed: i64,
    pub today_failed: i64,
    pub pending_approvals: i64,
    pub recent: Vec<db::ActivityRow>,
    pub data_dir: String,
    pub downloads_dir: Option<String>,
    pub platform: &'static str,
    pub version: String,
}

impl Core {
    pub fn init(app: &AppHandle) -> AppResult<Arc<Self>> {
        let paths = AppPaths::from_app(app)?;
        let conn = db::open(&paths.db)?;
        for m in modules::REGISTRY {
            db::ensure_module(&conn, m.id, m.default_enabled, &modules::default_config(m.id))?;
        }
        let master = db::get_setting_or(&conn, "master_enabled", true);
        let background = db::get_setting_or(&conn, "background_mode_enabled", true);
        // 시작 시 남은 임시 스냅샷 정리(강제 종료 복구)
        cleanup_temp_snapshots(&paths.backups);
        let _ = conn.execute("DELETE FROM jobs WHERE status IN ('COMPLETED','SKIPPED','CANCELLED','FAILED') AND created_at < ?1",
            [ (chrono::Utc::now() - chrono::Duration::days(14)).to_rfc3339() ]);
        let core = Arc::new(Self {
            app: app.clone(),
            paths,
            db: Mutex::new(conn),
            rt: Mutex::new(Runtime {
                master,
                background,
                starting: true,
                shutting_down: false,
                uninstalling: false,
                window_visible: false,
                shortcut_error: None,
                ai_shortcut_error: None,
                tray_hint_shown: false,
            }),
            watch: Mutex::new(WatchService::default()),
            self_writes: Mutex::new(HashMap::new()),
            engine_tx: Mutex::new(None),
            engine_thread: Mutex::new(None),
        });
        Ok(core)
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.db)
    }

    pub fn runtime(&self) -> Runtime {
        lock(&self.rt).clone()
    }

    pub fn update_runtime(&self, f: impl FnOnce(&mut Runtime)) {
        f(&mut lock(&self.rt));
    }

    // ---------------- lifecycle ----------------

    /// 엔진 스레드를 시작하고, 현재 설정에 맞게 감시/단축키를 적용한다.
    pub fn start(self: &Arc<Self>) {
        let (tx, rx) = mpsc::channel();
        *lock(&self.engine_tx) = Some(tx.clone());
        let me = Arc::clone(self);
        let handle = std::thread::Builder::new()
            .name("autodesk-engine".into())
            .spawn(move || me.engine_loop(rx))
            .ok();
        *lock(&self.engine_thread) = handle;
        self.update_runtime(|r| r.starting = false);
        self.apply();
        let _ = tx.send(EngineMsg::Recover);
        self.log("INFO", "core", "AutoDesk 엔진 시작");
    }

    /// 앱 종료: 신규 이벤트 차단 → 감시/단축키 해제 → 엔진 정지 → DB 체크포인트. 여러 번 불러도 안전하다.
    pub fn shutdown(&self) {
        {
            let mut rt = lock(&self.rt);
            if rt.shutting_down {
                return;
            }
            rt.shutting_down = true;
        }
        self.stop_watchers();
        crate::shortcuts::unregister_all(&self.app);
        if let Some(tx) = lock(&self.engine_tx).take() {
            let _ = tx.send(EngineMsg::Stop);
        }
        if let Some(h) = lock(&self.engine_thread).take() {
            // 진행 중인 파일 작업이 끝날 때까지 최대 5초 기다린다(무한 대기 금지).
            let deadline = Instant::now() + Duration::from_secs(5);
            while !h.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            if h.is_finished() {
                let _ = h.join();
            }
        }
        // 엔진이 제한 시간 안에 끝나지 않았더라도 종료가 멈추지 않도록 DB는 try_lock으로만 정리한다.
        if let Ok(conn) = self.db.try_lock() {
            for m in modules::REGISTRY {
                let _ = db::set_module_status(&conn, m.id, ModuleStatus::Stopping.as_str());
            }
            db::checkpoint(&conn);
        }
        self.log("INFO", "core", "AutoDesk 엔진 정상 종료");
    }

    pub fn app_status(&self) -> AppStatus {
        let rt = self.runtime();
        if rt.uninstalling {
            AppStatus::Uninstalling
        } else if rt.shutting_down {
            AppStatus::ShuttingDown
        } else if rt.starting {
            AppStatus::AppStarting
        } else if !rt.master {
            AppStatus::GlobalPaused
        } else if rt.window_visible {
            AppStatus::ForegroundActive
        } else {
            AppStatus::BackgroundActive
        }
    }

    /// 설정이 바뀔 때마다 호출: 모듈 상태 계산 → 감시 대상 동기화 → 단축키 동기화 → UI/트레이 갱신.
    pub fn apply(self: &Arc<Self>) {
        if self.runtime().shutting_down {
            return;
        }
        self.sync_watchers();
        crate::shortcuts::sync(self);
        self.persist_module_statuses();
        self.notify_changed();
    }

    pub fn notify_changed(&self) {
        let _ = self.app.emit("core://changed", ());
        crate::tray::refresh(&self.app);
    }

    pub fn set_master(self: &Arc<Self>, enabled: bool) -> AppResult<()> {
        db::set_setting(&self.db(), "master_enabled", &enabled)?;
        self.update_runtime(|r| r.master = enabled);
        {
            let conn = self.db();
            if enabled {
                // 일시정지된 작업을 다시 대기열로
                db::move_jobs(&conn, "PAUSED_GLOBAL", "QUEUED", None)?;
            } else {
                db::move_jobs(&conn, "QUEUED", "PAUSED_GLOBAL", None)?;
            }
            db::add_activity(&conn, "core", "master", "completed", None,
                if enabled { "전체 자동화를 켰습니다" } else { "전체 자동화를 일시정지했습니다" }, None, None)?;
        }
        self.apply();
        if enabled {
            self.send(EngineMsg::Recover);
        }
        Ok(())
    }

    pub fn set_background_mode(self: &Arc<Self>, enabled: bool) -> AppResult<()> {
        db::set_setting(&self.db(), "background_mode_enabled", &enabled)?;
        self.update_runtime(|r| r.background = enabled);
        self.notify_changed();
        Ok(())
    }

    pub fn set_module_enabled(self: &Arc<Self>, id: &str, enabled: bool) -> AppResult<()> {
        modules::meta(id).ok_or_else(|| AppError::invalid("알 수 없는 모듈입니다"))?;
        {
            let conn = self.db();
            db::set_module_enabled(&conn, id, enabled)?;
            if enabled {
                db::move_jobs(&conn, "PAUSED_MODULE", "QUEUED", Some(id))?;
            } else {
                db::move_jobs(&conn, "QUEUED", "PAUSED_MODULE", Some(id))?;
            }
        }
        self.apply();
        if enabled {
            self.send(EngineMsg::Recover);
        }
        Ok(())
    }

    pub fn module_config<T: for<'de> Deserialize<'de> + Default>(&self, id: &str) -> T {
        db::get_module(&self.db(), id).ok().and_then(|m| serde_json::from_str(&m.config_json).ok()).unwrap_or_default()
    }

    pub fn set_module_config(self: &Arc<Self>, id: &str, config: serde_json::Value) -> AppResult<()> {
        // 모듈별 타입으로 검증한 뒤 저장한다.
        let normalized = match id {
            modules::BACKUP => {
                let mut c: backup::BackupConfig = serde_json::from_value(config)?;
                c.max_versions = c.max_versions.clamp(1, 200);
                c.max_file_size_mb = c.max_file_size_mb.clamp(1, 4096);
                c.max_total_mb = c.max_total_mb.clamp(50, 1024 * 1024);
                serde_json::to_string(&c)?
            }
            modules::DOWNLOADS => {
                let c: downloads::DownloadsConfig = serde_json::from_value(config)?;
                if c.mode != "preview" && c.mode != "auto" {
                    return Err(AppError::invalid("모드는 preview 또는 auto여야 합니다"));
                }
                serde_json::to_string(&c)?
            }
            modules::AUTOPASTE => {
                let mut c: AutoPasteConfig = serde_json::from_value(config)?;
                c.max_blank_lines = c.max_blank_lines.min(5);
                if c.ai_enabled && !c.ai_consent {
                    return Err(AppError::invalid("AI 다듬기를 켜려면 외부 전송 동의가 필요합니다"));
                }
                serde_json::to_string(&c)?
            }
            modules::LOGWATCH => {
                let mut c: logwatch::LogWatchConfig = serde_json::from_value(config)?;
                c.error_patterns.retain(|p| !p.trim().is_empty());
                c.warn_patterns.retain(|p| !p.trim().is_empty());
                if c.error_patterns.is_empty() {
                    return Err(AppError::invalid("오류 패턴을 하나 이상 입력하세요"));
                }
                c.renotify_minutes = c.renotify_minutes.clamp(1, 24 * 60);
                c.context_lines = c.context_lines.min(20);
                serde_json::to_string(&c)?
            }
            _ => return Err(AppError::invalid("알 수 없는 모듈입니다")),
        };
        db::set_module_config(&self.db(), id, &normalized)?;
        self.apply();
        Ok(())
    }

    // ---------------- module status ----------------

    pub fn module_status(&self, id: &str) -> (ModuleStatus, Option<String>) {
        let rt = self.runtime();
        let row = match db::get_module(&self.db(), id) {
            Ok(r) => r,
            Err(e) => return (ModuleStatus::ErrorPaused, Some(e.message)),
        };
        if !row.enabled {
            return (ModuleStatus::Disabled, None);
        }
        if rt.shutting_down {
            return (ModuleStatus::Stopping, None);
        }
        if row.consecutive_errors >= MAX_CONSECUTIVE_ERRORS || row.status == "ERROR_PAUSED" && row.consecutive_errors > 0 {
            return (ModuleStatus::ErrorPaused, Some(format!("오류가 반복되어 일시정지됨({}) — 껐다 켜면 다시 시작합니다", row.last_error_code.unwrap_or_default())));
        }
        if !rt.master {
            return (ModuleStatus::PausedGlobal, Some("전체 자동화가 꺼져 있습니다".into()));
        }
        match id {
            modules::BACKUP => {
                let targets = db::list_watch_targets(&self.db(), Some(modules::BACKUP)).unwrap_or_default();
                let ok = targets.iter().filter(|t| t.enabled && Path::new(&t.path).is_dir()).count();
                if targets.is_empty() {
                    return (ModuleStatus::WaitingPermission, Some("감시할 폴더를 추가하세요".into()));
                }
                if ok == 0 {
                    return (ModuleStatus::WaitingPermission, Some("등록한 폴더에 접근할 수 없습니다".into()));
                }
            }
            modules::DOWNLOADS => {
                if self.download_dirs().is_empty() {
                    return (ModuleStatus::WaitingPermission, Some("다운로드 폴더를 찾을 수 없습니다".into()));
                }
            }
            modules::LOGWATCH => {
                let targets = db::list_watch_targets(&self.db(), Some(modules::LOGWATCH)).unwrap_or_default();
                if targets.is_empty() {
                    return (ModuleStatus::WaitingPermission, Some("감시할 로그 파일을 추가하세요".into()));
                }
                if !targets.iter().any(|t| t.enabled && Path::new(&t.path).is_file()) {
                    return (ModuleStatus::WaitingPermission, Some("등록한 로그 파일을 찾을 수 없습니다".into()));
                }
            }
            modules::AUTOPASTE => {
                if let Some(e) = rt.shortcut_error {
                    return (ModuleStatus::ErrorPaused, Some(e));
                }
            }
            _ => {}
        }
        (ModuleStatus::Running, None)
    }

    pub fn is_module_running(&self, id: &str) -> bool {
        self.module_status(id).0 == ModuleStatus::Running
    }

    fn persist_module_statuses(&self) {
        for m in modules::REGISTRY {
            let (s, _) = self.module_status(m.id);
            let _ = db::set_module_status(&self.db(), m.id, s.as_str());
        }
    }

    /// 모듈 실행 결과 반영. 반복 오류면 해당 모듈만 중지하고 알린다(다른 모듈은 계속).
    fn record_run(self: &Arc<Self>, module_id: &str, err: Option<&AppError>) {
        let count = db::record_module_run(&self.db(), module_id, err.map(|e| e.code)).unwrap_or(0);
        if count == MAX_CONSECUTIVE_ERRORS {
            let _ = db::set_module_status(&self.db(), module_id, "ERROR_PAUSED");
            let name = modules::meta(module_id).map(|m| m.name).unwrap_or(module_id);
            self.notify(&format!("{name} 일시정지"), "오류가 반복되어 이 모듈만 멈췄습니다. 자동화 화면에서 확인하세요.");
            self.apply();
        }
    }

    // ---------------- watchers ----------------

    pub fn download_dirs(&self) -> Vec<PathBuf> {
        let cfg: downloads::DownloadsConfig = self.module_config(modules::DOWNLOADS);
        let mut dirs = Vec::new();
        if cfg.watch_default_dir {
            if let Ok(d) = self.app.path().download_dir() {
                if d.is_dir() {
                    dirs.push(d);
                }
            }
        }
        let targets = db::list_watch_targets(&self.db(), Some(modules::DOWNLOADS)).unwrap_or_default();
        for t in targets {
            if t.enabled && Path::new(&t.path).is_dir() {
                dirs.push(PathBuf::from(t.path));
            }
        }
        dirs.sort();
        dirs.dedup();
        dirs
    }

    fn desired_watches(&self) -> BTreeMap<PathBuf, bool> {
        let mut want: BTreeMap<PathBuf, bool> = BTreeMap::new();
        if !self.runtime().master || self.runtime().shutting_down {
            return want;
        }
        if self.is_module_running(modules::BACKUP) {
            let targets = db::list_watch_targets(&self.db(), Some(modules::BACKUP)).unwrap_or_default();
            for t in targets {
                if t.enabled && Path::new(&t.path).is_dir() {
                    let e = want.entry(PathBuf::from(&t.path)).or_insert(false);
                    *e |= t.recursive;
                }
            }
        }
        if self.is_module_running(modules::DOWNLOADS) {
            for d in self.download_dirs() {
                want.entry(d).or_insert(false);
            }
        }
        if self.is_module_running(modules::LOGWATCH) {
            // 파일 자체가 아니라 폴더를 감시해야 로그 로테이션(삭제 후 재생성)에도 계속 따라간다.
            let targets = db::list_watch_targets(&self.db(), Some(modules::LOGWATCH)).unwrap_or_default();
            for t in targets {
                if let Some(parent) = Path::new(&t.path).parent() {
                    if t.enabled && parent.is_dir() {
                        want.entry(parent.to_path_buf()).or_insert(false);
                    }
                }
            }
        }
        want
    }

    /// 같은 경로는 감시자를 하나만 둔다. 필요 없어진 경로는 즉시 해제, 대상이 없으면 감시자 자체를 내린다.
    fn sync_watchers(&self) {
        let want = self.desired_watches();
        let mut ws = lock(&self.watch);
        if want.is_empty() {
            ws.watcher = None; // drop → OS 감시 해제
            ws.active.clear();
            return;
        }
        if ws.watcher.is_none() {
            let tx = lock(&self.engine_tx).clone();
            let Some(tx) = tx else { return };
            let created = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    use notify::EventKind::*;
                    if matches!(ev.kind, Create(_) | Modify(_)) {
                        for p in ev.paths {
                            let _ = tx.send(EngineMsg::Fs(p));
                        }
                    }
                }
            });
            match created {
                Ok(w) => ws.watcher = Some(w),
                Err(e) => {
                    drop(ws);
                    self.log("ERROR", "watch", &format!("감시자 생성 실패: {e}"));
                    return;
                }
            }
        }
        let current = ws.active.clone();
        for (path, rec) in &current {
            if want.get(path) != Some(rec) {
                if let Some(w) = ws.watcher.as_mut() {
                    let _ = w.unwatch(path);
                }
                ws.active.remove(path);
            }
        }
        let mut failures = Vec::new();
        for (path, rec) in &want {
            if ws.active.contains_key(path) {
                continue;
            }
            let mode = if *rec { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
            let result = ws.watcher.as_mut().map(|w| w.watch(path, mode));
            match result {
                Some(Ok(())) => {
                    ws.active.insert(path.clone(), *rec);
                }
                Some(Err(e)) => failures.push(format!("{}: {e}", fsutil::short_path(path))),
                None => {}
            }
        }
        drop(ws);
        for f in failures {
            self.log("WARN", "watch", &format!("폴더 감시 실패 {f}"));
        }
    }

    fn stop_watchers(&self) {
        let mut ws = lock(&self.watch);
        ws.watcher = None;
        ws.active.clear();
    }

    pub fn active_watch_count(&self) -> usize {
        lock(&self.watch).active.len()
    }

    pub fn mark_self_write(&self, p: &Path) {
        let mut sw = lock(&self.self_writes);
        sw.retain(|_, t| t.elapsed() < Duration::from_secs(30));
        sw.insert(p.to_path_buf(), Instant::now());
    }

    fn is_self_write(&self, p: &Path) -> bool {
        lock(&self.self_writes).get(p).map(|t| t.elapsed() < Duration::from_secs(15)).unwrap_or(false)
    }

    fn send(&self, msg: EngineMsg) {
        if let Some(tx) = lock(&self.engine_tx).as_ref() {
            let _ = tx.send(msg);
        }
    }

    // ---------------- engine ----------------

    fn engine_loop(self: Arc<Self>, rx: Receiver<EngineMsg>) {
        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(400)) {
                Ok(EngineMsg::Stop) => break,
                Ok(EngineMsg::Fs(p)) => {
                    if pending.len() < MAX_PENDING || pending.contains_key(&p) {
                        pending.insert(p, Instant::now());
                    }
                }
                Ok(EngineMsg::Recover) => self.recover_jobs(),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let ready: Vec<PathBuf> = pending.iter().filter(|(_, t)| t.elapsed() >= DEBOUNCE).map(|(p, _)| p.clone()).collect();
            for p in ready {
                pending.remove(&p);
                let rt = self.runtime();
                if rt.shutting_down {
                    break;
                }
                if !rt.master {
                    continue; // 전역 OFF: 최종 차단
                }
                // 모듈 하나가 panic해도 엔진 전체가 멈추지 않게 격리한다.
                let me = Arc::clone(&self);
                let path = p.clone();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || me.route(&path)));
            }
        }
    }

    /// 파일 이벤트를 구독 중인 모듈로 보낸다. OFF 모듈은 Core가 최종 차단한다.
    fn route(self: &Arc<Self>, path: &Path) {
        if self.is_self_write(path) || !path.is_file() {
            return;
        }
        if self.is_module_running(modules::BACKUP) {
            // 주의: 목록을 먼저 꺼내 DB 잠금을 놓은 뒤 처리한다(for 식의 임시 잠금이 루프 내내 유지되는 것 방지).
            let targets = db::list_watch_targets(&self.db(), Some(modules::BACKUP)).unwrap_or_default();
            for t in targets {
                let root = Path::new(&t.path);
                let inside = if t.recursive { path.starts_with(root) } else { path.parent() == Some(root) };
                if t.enabled && inside {
                    self.enqueue(modules::BACKUP, "backup.snapshot", path, Some(&t));
                }
            }
        }
        if self.is_module_running(modules::DOWNLOADS) {
            if let Some(parent) = path.parent() {
                if self.download_dirs().iter().any(|d| d == parent) {
                    self.enqueue(modules::DOWNLOADS, "download.classify", path, None);
                }
            }
        }
        if self.is_module_running(modules::LOGWATCH) {
            let targets = db::list_watch_targets(&self.db(), Some(modules::LOGWATCH)).unwrap_or_default();
            for t in targets.into_iter().filter(|t| t.enabled && Path::new(&t.path) == path) {
                // 로그는 append 위치(offset)로 상태를 이어가므로 작업 큐를 거치지 않고 바로 읽는다.
                let r = self.handle_log(&t);
                if let Err(e) = &r {
                    let _ = db::add_activity(&self.db(), modules::LOGWATCH, "log.read", "failed",
                        Some(&fsutil::short_path(path)), &e.message, Some(e.code), None);
                }
                self.record_run(modules::LOGWATCH, r.as_ref().err());
                self.notify_changed();
            }
        }
    }

    // ---------------- LogWatcher ----------------

    fn log_offset(&self, target_id: &str) -> AppResult<Option<u64>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .db()
            .query_row("SELECT offset FROM log_offsets WHERE target_id=?1", [target_id], |r| r.get::<_, i64>(0))
            .optional()?
            .map(|v| v.max(0) as u64))
    }

    fn set_log_offset(&self, target_id: &str, offset: u64) -> AppResult<()> {
        self.db().execute(
            "INSERT INTO log_offsets(target_id, offset, updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(target_id) DO UPDATE SET offset=excluded.offset, updated_at=excluded.updated_at",
            rusqlite::params![target_id, offset as i64, db::now()],
        )?;
        Ok(())
    }

    /// 마지막으로 읽은 위치부터 끝까지(한 번에 최대 16MB) 새 줄만 읽어 검사한다.
    fn handle_log(&self, t: &WatchTarget) -> AppResult<usize> {
        use std::io::{Read, Seek, SeekFrom};
        let cfg: logwatch::LogWatchConfig = self.module_config(modules::LOGWATCH);
        let path = Path::new(&t.path);
        let len = std::fs::metadata(path)?.len();
        let mut offset = self.log_offset(&t.id)?.unwrap_or(len);
        if len < offset {
            // 파일이 줄었다 = 로테이션/초기화 → 처음부터
            offset = 0;
            db::add_activity(&self.db(), modules::LOGWATCH, "log.rotate", "completed", Some(&fsutil::short_path(path)),
                "로그 파일이 새로 시작되어 처음부터 읽습니다", None, None)?;
        }
        if len == offset {
            return Ok(0);
        }
        let max = 16 * 1024 * 1024u64;
        // 너무 많이 쌓였으면(16MB 초과) 끝부분만 본다.
        let start = if len - offset > max { len - max } else { offset };
        let want = (len - start) as usize;
        let mut f = std::fs::File::open(path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; want];
        f.read_exact(&mut buf)?;
        let complete = logwatch::complete_len(&buf);
        if complete == 0 {
            return Ok(0); // 아직 줄이 끝나지 않음
        }
        let text = logwatch::decode(&buf[..complete]);
        let found = logwatch::scan(&text, &[], &cfg);
        let mut new_alerts = 0;
        for item in &found {
            if self.record_log_issue(t, item, &cfg)? {
                new_alerts += 1;
            }
        }
        self.set_log_offset(&t.id, start + complete as u64)?;
        Ok(new_alerts)
    }

    /// 같은 지문의 오류는 횟수만 늘린다. 처음이거나 재알림 간격이 지났으면 알림 + 이력에 남긴다.
    fn record_log_issue(&self, t: &WatchTarget, item: &logwatch::Found, cfg: &logwatch::LogWatchConfig) -> AppResult<bool> {
        use rusqlite::OptionalExtension;
        let now = chrono::Utc::now();
        let existing: Option<(i64, Option<String>)> = self
            .db()
            .query_row(
                "SELECT count, last_notified FROM log_issues WHERE fingerprint=?1 AND target_id=?2",
                rusqlite::params![item.fingerprint, t.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let should_alert = match &existing {
            None => true,
            Some((_, last)) => last
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|l| now.signed_duration_since(l) >= chrono::Duration::minutes(cfg.renotify_minutes))
                .unwrap_or(true),
        };
        let alert = should_alert && (item.level == logwatch::Level::Error || cfg.notify_warn);
        let ts = now.to_rfc3339();
        if existing.is_none() {
            self.db().execute(
                "INSERT INTO log_issues(fingerprint, target_id, level, sample, context, count, first_seen, last_seen, last_notified)
                 VALUES(?1,?2,?3,?4,?5,1,?6,?6,?7)",
                rusqlite::params![item.fingerprint, t.id, item.level.as_str(), item.line, item.context, ts,
                    if alert { Some(ts.clone()) } else { None }],
            )?;
        } else {
            self.db().execute(
                // 문맥은 더 많은 정보(스택 트레이스 등)를 담은 쪽을 유지한다.
                "UPDATE log_issues SET count=count+1, last_seen=?3, sample=?4,
                   context = CASE WHEN length(?5) >= length(context) THEN ?5 ELSE context END,
                   last_notified = CASE WHEN ?6 THEN ?3 ELSE last_notified END
                 WHERE fingerprint=?1 AND target_id=?2",
                rusqlite::params![item.fingerprint, t.id, ts, item.line, item.context, alert],
            )?;
        }
        if !alert {
            return Ok(false);
        }
        let file = fsutil::short_path(Path::new(&t.path));
        let count = existing.map(|e| e.0 + 1).unwrap_or(1);
        let label = if count == 1 { "새 오류".to_string() } else { format!("반복 오류({count}회)") };
        let preview: String = item.line.chars().take(120).collect();
        db::add_activity(&self.db(), modules::LOGWATCH, "log.issue", "completed", Some(&file),
            &format!("{} {label}: {preview}", item.level.as_str()), None, None)?;
        self.notify(&format!("LogWatcher · {label}"), &format!("{file}\n{preview}"));
        Ok(true)
    }

    pub fn list_log_issues(&self, target_id: Option<&str>) -> AppResult<Vec<serde_json::Value>> {
        let conn = self.db();
        let mut stmt = conn.prepare(
            "SELECT i.fingerprint, i.target_id, w.path, i.level, i.sample, i.context, i.count, i.first_seen, i.last_seen
             FROM log_issues i LEFT JOIN watch_targets w ON w.id = i.target_id
             WHERE (?1 IS NULL OR i.target_id=?1) ORDER BY i.last_seen DESC LIMIT 300",
        )?;
        let rows = stmt.query_map([target_id], |r| {
            Ok(serde_json::json!({
                "fingerprint": r.get::<_, String>(0)?, "targetId": r.get::<_, String>(1)?, "path": r.get::<_, Option<String>>(2)?,
                "level": r.get::<_, String>(3)?, "sample": r.get::<_, String>(4)?, "context": r.get::<_, String>(5)?,
                "count": r.get::<_, i64>(6)?, "firstSeen": r.get::<_, String>(7)?, "lastSeen": r.get::<_, String>(8)?,
            }))
        })?;
        let out: Vec<serde_json::Value> = rows.collect::<Result<_, _>>()?;
        Ok(out)
    }

    pub fn clear_log_issues(self: &Arc<Self>, target_id: Option<&str>) -> AppResult<usize> {
        let n = self.db().execute("DELETE FROM log_issues WHERE (?1 IS NULL OR target_id=?1)", [target_id])?;
        self.notify_changed();
        Ok(n)
    }

    fn enqueue(self: &Arc<Self>, module_id: &str, job_type: &str, path: &Path, target: Option<&WatchTarget>) {
        let meta = std::fs::metadata(path).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta.and_then(|m| m.modified().ok()).map(|t| format!("{t:?}")).unwrap_or_default();
        let key = format!("{module_id}|{}|{size}|{mtime}", path.to_string_lossy());
        let payload = serde_json::json!({ "path": path.to_string_lossy(), "targetId": target.map(|t| t.id.clone()) }).to_string();
        let id = match db::insert_job(&self.db(), module_id, job_type, "QUEUED", &fsutil::sha256_str(&key), &payload) {
            Ok(Some(id)) => id,
            _ => return, // 같은 변경에 대한 중복 작업
        };
        self.run_job(&id);
    }

    /// 앱 시작/재개 시 미완료 작업(QUEUED, RUNNING)을 다시 처리한다.
    fn recover_jobs(self: &Arc<Self>) {
        let jobs = db::jobs_with_status(&self.db(), &["QUEUED", "RUNNING"], None).unwrap_or_default();
        for j in jobs {
            if self.runtime().shutting_down || !self.runtime().master {
                break;
            }
            if !self.is_module_running(&j.module_id) {
                let _ = db::update_job_status(&self.db(), &j.id, "PAUSED_MODULE", None);
                continue;
            }
            if j.attempts >= 3 {
                let err = AppError::new("RETRY_EXHAUSTED", "재시도 횟수를 초과했습니다");
                let _ = db::update_job_status(&self.db(), &j.id, "FAILED", Some(&err));
                continue;
            }
            self.run_job(&j.id);
        }
    }

    fn run_job(self: &Arc<Self>, job_id: &str) {
        let job = match db::get_job(&self.db(), job_id) {
            Ok(j) => j,
            Err(_) => return,
        };
        let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
        let path = PathBuf::from(payload["path"].as_str().unwrap_or_default());
        let _ = db::update_job_status(&self.db(), job_id, "RUNNING", None);
        let result = match job.module_id.as_str() {
            modules::BACKUP => self.handle_backup(&path, payload["targetId"].as_str().unwrap_or_default()),
            modules::DOWNLOADS => self.handle_download(job_id, &path),
            _ => Err(AppError::invalid("이 모듈은 파일 작업을 처리하지 않습니다")),
        };
        match result {
            Ok(status) => {
                let _ = db::update_job_status(&self.db(), job_id, status, None);
                if status == "COMPLETED" || status == "WAITING_APPROVAL" {
                    self.record_run(&job.module_id, None);
                }
            }
            Err(e) => {
                let _ = db::update_job_status(&self.db(), job_id, "FAILED", Some(&e));
                let _ = db::add_activity(&self.db(), &job.module_id, &job.job_type, "failed",
                    Some(&fsutil::short_path(&path)), &e.message, Some(e.code), None);
                self.log("WARN", &job.module_id, &format!("{} 실패: {}", job.job_type, e.code));
                self.record_run(&job.module_id, Some(&e));
                if job.module_id == modules::BACKUP {
                    self.notify("백업 실패", &format!("{} — {}", fsutil::short_path(&path), e.message));
                }
            }
        }
        self.notify_changed();
    }

    // ---------------- AutoBackup ----------------

    fn handle_backup(&self, path: &Path, target_id: &str) -> AppResult<&'static str> {
        let cfg: backup::BackupConfig = self.module_config(modules::BACKUP);
        if !path.exists() {
            return Ok("SKIPPED");
        }
        if !backup::is_candidate(path, &cfg) {
            return Ok("SKIPPED");
        }
        let target = db::list_watch_targets(&self.db(), Some(modules::BACKUP))?
            .into_iter()
            .find(|t| t.id == target_id)
            .ok_or_else(|| AppError::not_found("감시 폴더 등록이 삭제되었습니다"))?;
        let root = PathBuf::from(&target.path);
        // symlink를 통한 감시 폴더 탈출 차단
        if !fsutil::is_within(&root, path) || fsutil::is_within(&self.paths.data, path) {
            return Ok("SKIPPED");
        }
        if !fsutil::is_stable(path, Duration::from_millis(800)) {
            return Ok("SKIPPED"); // 아직 쓰는 중: 다음 이벤트에서 다시 처리된다
        }
        let outcome = {
            let conn = self.db();
            backup::backup_file(&conn, &self.paths.backups, &target.id, &root, path, &cfg)?
        };
        let short = fsutil::short_path(path);
        match outcome {
            backup::BackupOutcome::Created { size, .. } => {
                db::add_activity(&self.db(), modules::BACKUP, "backup.snapshot", "completed", Some(&short),
                    &format!("버전 저장 ({})", human_size(size)), None, None)?;
                Ok("COMPLETED")
            }
            backup::BackupOutcome::Unchanged => Ok("SKIPPED"),
            backup::BackupOutcome::TooLarge(size) => {
                db::add_activity(&self.db(), modules::BACKUP, "backup.snapshot", "skipped", Some(&short),
                    &format!("크기 제한 초과로 건너뜀 ({})", human_size(size)), None, None)?;
                Ok("SKIPPED")
            }
        }
    }

    pub fn restore_version(self: &Arc<Self>, version_id: &str, mode: &str) -> AppResult<String> {
        let v = backup::get_version(&self.db(), version_id)?;
        if !v.snapshot_path.is_file() {
            return Err(AppError::not_found("스냅샷 파일이 없습니다(정리되었거나 이동됨)"));
        }
        if fsutil::sha256_file(&v.snapshot_path)? != v.sha256 {
            return Err(AppError::new("SNAPSHOT_CORRUPT", "스냅샷 파일이 손상되어 복원하지 않았습니다"));
        }
        let parent = v.original_path.parent().ok_or_else(|| AppError::invalid("원래 경로가 올바르지 않습니다"))?;
        if !parent.is_dir() {
            return Err(AppError::new("ORIGINAL_DIR_MISSING", "원래 폴더가 없습니다. 폴더를 만든 뒤 다시 시도하세요"));
        }
        let dest = match mode {
            "copy" => {
                let stem = v.original_path.file_stem().and_then(|s| s.to_str()).unwrap_or("restored");
                let ext = v.original_path.extension().and_then(|s| s.to_str());
                let name = match ext {
                    Some(e) => format!("{stem} (복원 {}).{e}", chrono::Local::now().format("%m%d-%H%M")),
                    None => format!("{stem} (복원 {})", chrono::Local::now().format("%m%d-%H%M")),
                };
                fsutil::unique_path(parent, &name)
            }
            "overwrite" => {
                // 덮어쓰기 전에 현재 파일을 먼저 백업한다.
                if v.original_path.is_file() {
                    let cfg: backup::BackupConfig = self.module_config(modules::BACKUP);
                    let root = db::list_watch_targets(&self.db(), Some(modules::BACKUP))?
                        .into_iter()
                        .find(|t| t.id == v.watch_target_id)
                        .map(|t| PathBuf::from(t.path))
                        .unwrap_or_else(|| parent.to_path_buf());
                    let conn = self.db();
                    let big = backup::BackupConfig { max_file_size_mb: u64::MAX / (1024 * 1024 * 2), ..cfg };
                    backup::backup_file(&conn, &self.paths.backups, &v.watch_target_id, &root, &v.original_path, &big)?;
                }
                v.original_path.clone()
            }
            _ => return Err(AppError::invalid("복원 방식이 올바르지 않습니다")),
        };
        fsutil::atomic_copy(&v.snapshot_path, &dest)?;
        db::add_activity(&self.db(), modules::BACKUP, "backup.restore", "completed", Some(&fsutil::short_path(&dest)),
            if mode == "copy" { "백업 버전을 사본으로 복원" } else { "백업 버전으로 복원(이전 상태는 백업됨)" }, None, None)?;
        self.notify_changed();
        Ok(dest.to_string_lossy().to_string())
    }

    // ---------------- Download Butler ----------------

    fn handle_download(&self, job_id: &str, path: &Path) -> AppResult<&'static str> {
        let cfg: downloads::DownloadsConfig = self.module_config(modules::DOWNLOADS);
        if !path.is_file() || downloads::should_ignore(path, &cfg) {
            return Ok("SKIPPED");
        }
        // 다운로드 완료 확인: 크기/수정시각이 안정될 때까지
        if !fsutil::is_stable(path, Duration::from_millis(1500)) {
            return Ok("SKIPPED");
        }
        // 사용자가 '되돌리기'한 파일(같은 내용)은 다시 옮기지 않는다.
        let has_undone: bool = self.db().query_row("SELECT EXISTS(SELECT 1 FROM file_moves WHERE undone_at IS NOT NULL)", [], |r| r.get(0))?;
        if has_undone {
            let hash = fsutil::sha256_file(path)?;
            let undone: bool = self.db().query_row(
                "SELECT EXISTS(SELECT 1 FROM file_moves WHERE undone_at IS NOT NULL AND sha256=?1)", [&hash], |r| r.get(0))?;
            if undone {
                return Ok("SKIPPED");
            }
        }
        let (category, confident) = downloads::classify(path);
        let dir = path.parent().ok_or_else(|| AppError::invalid("경로 오류"))?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let short = fsutil::short_path(path);
        let category_label = category.unwrap_or("Unknown");
        if cfg.mode == "auto" && confident {
            if let Some(cat) = category {
                let dest = fsutil::unique_path(&downloads::category_dir(dir, cat), &name);
                self.do_move(path, &dest, "download.move", "자동 분류")?;
                return Ok("COMPLETED");
            }
        }
        // 미리보기 모드이거나 분류가 불확실하면 승인 대기
        let proposed = downloads::category_dir(dir, category_label).join(&name);
        let payload = serde_json::json!({
            "path": path.to_string_lossy(),
            "proposed": proposed.to_string_lossy(),
            "category": category_label,
            "confident": confident,
        });
        self.db().execute("UPDATE jobs SET payload_json=?2 WHERE id=?1", rusqlite::params![job_id, payload.to_string()])?;
        db::add_activity(&self.db(), modules::DOWNLOADS, "download.preview", "waiting", Some(&short),
            &format!("{category_label} 폴더로 이동 제안{}", if confident { "" } else { " (분류 불확실)" }), None, None)?;
        Ok("WAITING_APPROVAL")
    }

    fn do_move(&self, src: &Path, dest: &Path, action: &str, label: &str) -> AppResult<String> {
        self.mark_self_write(dest);
        let hash = fsutil::safe_move(src, dest)?;
        let move_id = db::new_id();
        self.db().execute(
            "INSERT INTO file_moves(id, module_id, original_path, new_path, sha256, undo_available, created_at) VALUES(?1,?2,?3,?4,?5,1,?6)",
            rusqlite::params![move_id, modules::DOWNLOADS, src.to_string_lossy(), dest.to_string_lossy(), hash, db::now()],
        )?;
        let rel = dest.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        db::add_activity(&self.db(), modules::DOWNLOADS, action, "completed", Some(&fsutil::short_path(dest)),
            &format!("{label}: {rel} 폴더로 이동"), None, Some(&move_id))?;
        Ok(move_id)
    }

    /// 승인 센터에서 이동을 승인한다. 파일이 그 사이 바뀌었으면 안전하게 거절한다.
    pub fn approve_job(self: &Arc<Self>, job_id: &str, category: Option<String>) -> AppResult<()> {
        let job = db::get_job(&self.db(), job_id)?;
        if job.status != "WAITING_APPROVAL" {
            return Err(AppError::invalid("이미 처리된 항목입니다"));
        }
        let p: serde_json::Value = serde_json::from_str(&job.payload_json)?;
        let src = PathBuf::from(p["path"].as_str().unwrap_or_default());
        if !src.is_file() {
            db::update_job_status(&self.db(), job_id, "CANCELLED", Some(&AppError::not_found("파일이 이미 없습니다")))?;
            self.notify_changed();
            return Err(AppError::not_found("파일이 이미 옮겨졌거나 삭제되었습니다"));
        }
        let dir = src.parent().ok_or_else(|| AppError::invalid("경로 오류"))?;
        if !self.download_dirs().iter().any(|d| d == dir) {
            return Err(AppError::invalid("감시 중인 다운로드 폴더의 파일이 아닙니다"));
        }
        let cat = category.unwrap_or_else(|| p["category"].as_str().unwrap_or("Unknown").to_string());
        let allowed = downloads::CATEGORY_FOLDERS.contains(&cat.as_str()) || cat == "Unknown";
        if !allowed {
            return Err(AppError::invalid("허용되지 않은 분류 폴더입니다"));
        }
        let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let dest = fsutil::unique_path(&downloads::category_dir(dir, &cat), &name);
        self.do_move(&src, &dest, "download.move", "승인 후 이동")?;
        db::update_job_status(&self.db(), job_id, "COMPLETED", None)?;
        self.notify_changed();
        Ok(())
    }

    pub fn reject_job(self: &Arc<Self>, job_id: &str) -> AppResult<()> {
        let job = db::get_job(&self.db(), job_id)?;
        if job.status != "WAITING_APPROVAL" {
            return Err(AppError::invalid("이미 처리된 항목입니다"));
        }
        db::update_job_status(&self.db(), job_id, "CANCELLED", None)?;
        let p: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap_or_default();
        db::add_activity(&self.db(), &job.module_id, "download.preview", "cancelled",
            Some(&fsutil::short_path(Path::new(p["path"].as_str().unwrap_or_default()))), "이동 제안을 거절했습니다(파일 그대로 둠)", None, None)?;
        self.notify_changed();
        Ok(())
    }

    pub fn undo_move(self: &Arc<Self>, move_id: &str) -> AppResult<String> {
        let (orig, new_path, available): (String, String, i64) = self.db().query_row(
            "SELECT original_path, new_path, undo_available FROM file_moves WHERE id=?1",
            [move_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).map_err(|_| AppError::not_found("이동 기록을 찾을 수 없습니다"))?;
        if available == 0 {
            return Err(AppError::invalid("이미 되돌린 이동입니다"));
        }
        let (orig, new_path) = (PathBuf::from(orig), PathBuf::from(new_path));
        if !new_path.is_file() {
            return Err(AppError::not_found("이동된 파일이 더 이상 그 위치에 없습니다"));
        }
        let dir = orig.parent().ok_or_else(|| AppError::invalid("경로 오류"))?;
        let name = orig.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let dest = fsutil::unique_path(dir, &name);
        self.mark_self_write(&dest);
        fsutil::safe_move(&new_path, &dest)?;
        self.db().execute("UPDATE file_moves SET undo_available=0, undone_at=?2 WHERE id=?1", rusqlite::params![move_id, db::now()])?;
        db::add_activity(&self.db(), modules::DOWNLOADS, "download.undo", "completed", Some(&fsutil::short_path(&dest)),
            "이동을 되돌려 원래 위치로 복원", None, None)?;
        self.notify_changed();
        Ok(dest.to_string_lossy().to_string())
    }

    /// 다운로드 폴더에 이미 있는 파일을 일괄 '미리보기'로 올린다(자동 이동하지 않음).
    pub fn scan_downloads(self: &Arc<Self>) -> AppResult<usize> {
        if !self.is_module_running(modules::DOWNLOADS) {
            return Err(AppError::invalid("Download Butler가 실행 중일 때만 검사할 수 있습니다"));
        }
        let cfg: downloads::DownloadsConfig = self.module_config(modules::DOWNLOADS);
        let mut n = 0;
        for dir in self.download_dirs() {
            for e in std::fs::read_dir(&dir)?.flatten() {
                let p = e.path();
                if !p.is_file() || downloads::should_ignore(&p, &cfg) {
                    continue;
                }
                let (cat, confident) = downloads::classify(&p);
                let label = cat.unwrap_or("Unknown");
                let name = e.file_name().to_string_lossy().to_string();
                let payload = serde_json::json!({
                    "path": p.to_string_lossy(),
                    "proposed": downloads::category_dir(&dir, label).join(&name).to_string_lossy(),
                    "category": label,
                    "confident": confident,
                });
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                let key = fsutil::sha256_str(&format!("scan|{}|{size}", p.to_string_lossy()));
                if let Ok(Some(_)) = db::insert_job(&self.db(), modules::DOWNLOADS, "download.classify", "WAITING_APPROVAL", &key, &payload.to_string()) {
                    n += 1;
                }
            }
        }
        db::add_activity(&self.db(), modules::DOWNLOADS, "download.scan", "completed", None,
            &format!("기존 파일 {n}개에 대한 이동 제안을 만들었습니다"), None, None)?;
        self.notify_changed();
        Ok(n)
    }

    // ---------------- watch targets ----------------

    pub fn add_watch_target(self: &Arc<Self>, module_id: &str, path: &str, recursive: bool) -> AppResult<String> {
        if module_id == modules::LOGWATCH {
            let canon = fsutil::validate_log_file(Path::new(path), &self.paths.data)?;
            let id = db::add_watch_target(&self.db(), module_id, &canon.to_string_lossy(), false)?;
            // 등록 시점 이후에 쌓이는 줄만 검사한다.
            let len = std::fs::metadata(&canon).map(|m| m.len()).unwrap_or(0);
            self.set_log_offset(&id, len)?;
            self.apply();
            return Ok(id);
        }
        if module_id != modules::BACKUP && module_id != modules::DOWNLOADS {
            return Err(AppError::invalid("폴더를 등록할 수 없는 모듈입니다"));
        }
        let canon = fsutil::validate_watch_root(Path::new(path), &self.paths.data)?;
        let recursive = recursive && module_id == modules::BACKUP; // 다운로드는 최상위만
        let id = db::add_watch_target(&self.db(), module_id, &canon.to_string_lossy(), recursive)?;
        self.apply();
        Ok(id)
    }

    pub fn remove_watch_target(self: &Arc<Self>, id: &str) -> AppResult<()> {
        db::remove_watch_target(&self.db(), id)?;
        self.db().execute("DELETE FROM log_offsets WHERE target_id=?1", [id])?;
        self.db().execute("DELETE FROM log_issues WHERE target_id=?1", [id])?;
        self.apply();
        Ok(())
    }

    // ---------------- overview ----------------

    pub fn overview(&self, launch_at_login: (bool, Option<String>)) -> AppResult<Overview> {
        let rt = self.runtime();
        let mut mods = Vec::new();
        for m in modules::REGISTRY {
            let row = db::get_module(&self.db(), m.id)?;
            let (status, reason) = self.module_status(m.id);
            mods.push(ModuleView {
                id: m.id,
                name: m.name,
                description: m.description,
                enabled: row.enabled,
                status,
                status_reason: reason.or_else(|| if m.id == modules::AUTOPASTE { rt.ai_shortcut_error.clone() } else { None }),
                last_run_at: row.last_run_at,
                last_error_code: row.last_error_code,
                today_count: db::count_activity_today(&self.db(), "completed", Some(m.id))?,
                config: serde_json::from_str(&row.config_json).unwrap_or_default(),
            });
        }
        let conn = self.db();
        let pending: i64 = conn.query_row("SELECT COUNT(*) FROM jobs WHERE status='WAITING_APPROVAL'", [], |r| r.get(0))?;
        let today_completed = conn.query_row(
            "SELECT COUNT(*) FROM activity WHERE created_day=?1 AND result='completed' AND module_id<>'core'",
            [db::today_local()], |r| r.get(0))?;
        let today_failed = db::count_activity_today(&conn, "failed", None)?;
        let recent = db::list_activity(&conn, None, None, 8)?;
        drop(conn);
        Ok(Overview {
            status: self.app_status(),
            master_enabled: rt.master,
            background_mode: rt.background,
            launch_at_login: launch_at_login.0,
            launch_at_login_error: launch_at_login.1,
            modules: mods,
            today_completed,
            today_failed,
            pending_approvals: pending,
            recent,
            data_dir: self.paths.data.to_string_lossy().to_string(),
            downloads_dir: self.app.path().download_dir().ok().map(|p| p.to_string_lossy().to_string()),
            platform: std::env::consts::OS,
            version: self.app.package_info().version.to_string(),
        })
    }

    // ---------------- notify / log ----------------

    pub fn notify(&self, title: &str, body: &str) {
        if !db::get_setting_or(&self.db(), "notifications_enabled", true) {
            return;
        }
        use tauri_plugin_notification::NotificationExt;
        let _ = self.app.notification().builder().title(title).body(body).show();
    }

    /// 구조화 로그(파일). 개인 데이터/클립보드 내용은 절대 넣지 않는다. 1MB 넘으면 회전.
    pub fn log(&self, level: &str, module: &str, msg: &str) {
        use std::io::Write;
        let file = self.paths.logs.join("autodesk.log");
        if std::fs::metadata(&file).map(|m| m.len() > 1024 * 1024).unwrap_or(false) {
            let _ = std::fs::rename(&file, self.paths.logs.join("autodesk.1.log"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
            let line = serde_json::json!({ "ts": db::now(), "level": level, "module": module, "msg": msg });
            let _ = writeln!(f, "{line}");
        }
    }
}

fn cleanup_temp_snapshots(dir: &Path) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                cleanup_temp_snapshots(&p);
            } else if p.extension().map(|x| x == "tmp-snapshot").unwrap_or(false) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[i]) }
}
