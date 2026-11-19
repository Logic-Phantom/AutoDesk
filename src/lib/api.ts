// Rust Core 명령의 타입 계약. 실행 로직은 Rust에 있고 UI는 명령만 보낸다.
import { invoke } from '@tauri-apps/api/core';

export type AppStatus =
  | 'APP_STARTING'
  | 'FOREGROUND_ACTIVE'
  | 'BACKGROUND_ACTIVE'
  | 'GLOBAL_PAUSED'
  | 'SHUTTING_DOWN'
  | 'UNINSTALLING';

export type ModuleStatus =
  | 'DISABLED'
  | 'RUNNING'
  | 'PAUSED_GLOBAL'
  | 'WAITING_PERMISSION'
  | 'ERROR_PAUSED'
  | 'STOPPING';

export interface ActivityRow {
  id: string;
  moduleId: string;
  action: string;
  result: 'completed' | 'failed' | 'skipped' | 'waiting' | 'cancelled';
  target: string | null;
  message: string;
  errorCode: string | null;
  undoRef: string | null;
  createdAt: string;
}

export interface ModuleView {
  id: 'backup' | 'downloads' | 'autopaste' | 'logwatch';
  name: string;
  description: string;
  enabled: boolean;
  status: ModuleStatus;
  statusReason: string | null;
  lastRunAt: string | null;
  lastErrorCode: string | null;
  todayCount: number;
  config: Record<string, unknown>;
}

export interface Overview {
  status: AppStatus;
  masterEnabled: boolean;
  backgroundMode: boolean;
  launchAtLogin: boolean;
  launchAtLoginError: string | null;
  modules: ModuleView[];
  todayCompleted: number;
  todayFailed: number;
  pendingApprovals: number;
  recent: ActivityRow[];
  dataDir: string;
  downloadsDir: string | null;
  platform: string;
  version: string;
}

export interface WatchTarget {
  id: string;
  moduleId: string;
  path: string;
  recursive: boolean;
  enabled: boolean;
}

export interface JobRow {
  id: string;
  moduleId: string;
  jobType: string;
  status: string;
  payloadJson: string;
  attempts: number;
  createdAt: string;
}

export interface BackedFile {
  originalPath: string;
  relativePath: string;
  versions: number;
  lastBackupAt: string;
  exists: boolean;
}

export interface VersionRow {
  id: string;
  originalPath: string;
  relativePath: string;
  sha256: string;
  fileSize: number;
  createdAt: string;
}

export type DiffResult =
  | { kind: 'Text'; diff: string; identical: boolean }
  | { kind: 'Binary'; snapshotSize: number; snapshotHash: string; currentSize: number | null; currentHash: string | null };

export interface LogIssue {
  fingerprint: string;
  targetId: string;
  path: string | null;
  level: 'ERROR' | 'WARN';
  sample: string;
  context: string;
  count: number;
  firstSeen: string;
  lastSeen: string;
}

export interface ClaudeStatus {
  available: boolean;
  path: string | null;
  version: string | null;
}

export interface UninstallPreview {
  items: { label: string; path: string | null; size: string | null; note: string | null }[];
  backupsSize: string;
  totalSize: string;
  devBuild: boolean;
  preserved: string[];
}

export interface AppError {
  code: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === 'object' && 'message' in e) return String((e as AppError).message);
  return String(e);
}

export const api = {
  overview: () => invoke<Overview>('get_overview'),
  setMaster: (enabled: boolean) => invoke<void>('set_master', { enabled }),
  setBackgroundMode: (enabled: boolean) => invoke<void>('set_background_mode', { enabled }),
  setLaunchAtLogin: (enabled: boolean, alsoEnableBackground = false) =>
    invoke<boolean>('set_launch_at_login', { enabled, alsoEnableBackground }),
  setModuleEnabled: (id: string, enabled: boolean) => invoke<void>('set_module_enabled', { id, enabled }),
  setModuleConfig: (id: string, config: Record<string, unknown>) => invoke<void>('set_module_config', { id, config }),
  listWatchTargets: (moduleId?: string) => invoke<WatchTarget[]>('list_watch_targets', { moduleId }),
  addWatchTarget: (moduleId: string, path: string, recursive: boolean) =>
    invoke<string>('add_watch_target', { moduleId, path, recursive }),
  removeWatchTarget: (id: string) => invoke<void>('remove_watch_target', { id }),
  listActivity: (result?: string, moduleId?: string, limit = 300) =>
    invoke<ActivityRow[]>('list_activity', { result, moduleId, limit }),
  listApprovals: () => invoke<JobRow[]>('list_approvals'),
  approveJob: (id: string, category?: string) => invoke<void>('approve_job', { id, category }),
  rejectJob: (id: string) => invoke<void>('reject_job', { id }),
  scanDownloads: () => invoke<number>('scan_downloads'),
  undoMove: (moveId: string) => invoke<string>('undo_move', { moveId }),
  listBackedFiles: (query?: string) => invoke<BackedFile[]>('list_backed_files', { query }),
  listVersions: (originalPath: string) => invoke<VersionRow[]>('list_versions', { originalPath }),
  diffVersion: (id: string) => invoke<DiffResult>('diff_version', { id }),
  restoreVersion: (id: string, mode: 'copy' | 'overwrite') => invoke<string>('restore_version', { id, mode }),
  getSettings: () => invoke<{ notificationsEnabled: boolean; firstRunCompleted: boolean }>('get_settings'),
  setSetting: (key: string, value: unknown) => invoke<void>('set_setting', { key, value }),
  listLogIssues: (targetId?: string) => invoke<LogIssue[]>('list_log_issues', { targetId }),
  clearLogIssues: (targetId?: string) => invoke<number>('clear_log_issues', { targetId }),
  aiStatus: () => invoke<ClaudeStatus>('ai_status'),
  uninstallPreview: () => invoke<UninstallPreview>('uninstall_preview'),
  uninstallExecute: (keepBackups: boolean, confirm: string) =>
    invoke<string>('uninstall_execute', { keepBackups, confirm }),
  quit: () => invoke<void>('quit_app'),
};
