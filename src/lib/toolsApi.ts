// 도구(캡처·표 변환·비교·이미지·업무 모드) 명령 계약
import { invoke } from '@tauri-apps/api/core';

export interface LaunchItem {
  kind: 'app' | 'url' | 'file';
  target: string;
  args: string;
  label: string;
}
export interface CommandItem {
  label: string;
  path: string;
  args: string;
  cwd: string;
}
export interface CloseItem {
  process: string;
  label: string;
}
export interface WorkspaceConfig {
  dockEnabled: boolean;
  startItems: LaunchItem[];
  startResumeAutomation: boolean;
  awayAction: 'lock' | 'display_off' | 'none';
  awayPauseAutomation: boolean;
  endCommands: CommandItem[];
  endClose: CloseItem[];
  endCloseTimeoutSec: number;
  endForceKill: boolean;
  endAfter: 'none' | 'lock' | 'shutdown';
  endPauseAutomation: boolean;
}
export interface StepResult {
  label: string;
  ok: boolean;
  detail: string;
}
export interface ModeResult {
  mode: 'start' | 'away' | 'end';
  steps: StepResult[];
}
export interface AppWindow {
  process: string;
  title: string;
  pid: number;
}
export interface ToolSettings {
  captureShortcut: string;
  captureShortcutEnabled: boolean;
}
export interface CaptureWindow {
  id: number;
  title: string;
  app: string;
  width: number;
  height: number;
}
export interface ImageJob {
  files: string[];
  format: 'keep' | 'jpg' | 'png' | 'webp';
  resize: 'none' | 'fit' | 'exact' | 'cover' | 'percent';
  width: number;
  height: number;
  percent: number;
  quality: number;
  maxKb: number;
  outputDir: string;
  suffix: string;
}
export interface ImageResult {
  source: string;
  output: string | null;
  beforeBytes: number;
  afterBytes: number;
  width: number;
  height: number;
  note: string | null;
  error: string | null;
}

export const tools = {
  dockSetSize: (width: number, height: number) => invoke<void>('dock_set_size', { width, height }),
  workspaceGet: () => invoke<WorkspaceConfig>('workspace_get'),
  workspaceSet: (config: WorkspaceConfig) => invoke<void>('workspace_set', { config }),
  workspaceRun: (mode: 'start' | 'away' | 'end') => invoke<ModeResult>('workspace_run', { mode }),
  workspacePreviewEnd: () => invoke<AppWindow[]>('workspace_preview_end'),
  runningApps: () => invoke<AppWindow[]>('workspace_running_apps'),
  cancelShutdown: () => invoke<string>('workspace_cancel_shutdown'),
  openMain: (route?: string) => invoke<void>('open_main', { route }),
  settingsGet: () => invoke<ToolSettings>('tool_settings_get'),
  settingsSet: (settings: ToolSettings) => invoke<void>('tool_settings_set', { settings }),
  captureStart: (mode: 'full' | 'region') => invoke<void>('capture_start', { mode }),
  captureWindows: () => invoke<CaptureWindow[]>('capture_list_windows'),
  captureWindow: (id: number) => invoke<void>('capture_window', { id }),
  captureCrop: (x: number, y: number, w: number, h: number) => invoke<void>('capture_crop', { x, y, w, h }),
  captureCancel: () => invoke<void>('capture_cancel'),
  captureGet: () => invoke<ArrayBuffer>('capture_get'),
  captureClear: (gen?: number) => invoke<void>('capture_clear', { gen }),
  captureGen: () => invoke<number>('capture_gen'),
  imageCopy: (data: string) => invoke<void>('image_copy', { data }),
  imageSave: (path: string, data: string) => invoke<void>('image_save', { path, data }),
  ocr: (data: string) => invoke<string>('ocr_image', { data }),
  readText: (path: string) => invoke<string>('read_text_file', { path }),
  saveText: (path: string, contents: string, bom = false) => invoke<void>('save_text_file', { path, contents, bom }),
  imageBatch: (job: ImageJob) => invoke<ImageResult[]>('image_batch', { job }),
  openPath: (path: string) => invoke<void>('open_path', { path }),
};

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
