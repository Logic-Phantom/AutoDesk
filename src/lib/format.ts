import type { ActivityRow, AppStatus, ModuleStatus } from './api';

export const APP_STATUS_LABEL: Record<AppStatus, string> = {
  APP_STARTING: '시작 중',
  FOREGROUND_ACTIVE: '실행 중',
  BACKGROUND_ACTIVE: '백그라운드 실행 중',
  GLOBAL_PAUSED: '일시정지',
  SHUTTING_DOWN: '종료 중',
  UNINSTALLING: '제거 중',
};

export const MODULE_STATUS_LABEL: Record<ModuleStatus, string> = {
  DISABLED: '꺼짐',
  RUNNING: '작동 중',
  PAUSED_GLOBAL: '전체 일시정지',
  WAITING_PERMISSION: '설정 필요',
  ERROR_PAUSED: '오류로 정지',
  STOPPING: '정지 중',
};

export function moduleTone(s: ModuleStatus): 'ok' | 'warn' | 'err' | 'off' {
  if (s === 'RUNNING') return 'ok';
  if (s === 'ERROR_PAUSED') return 'err';
  if (s === 'DISABLED' || s === 'STOPPING') return 'off';
  return 'warn';
}

export const RESULT_LABEL: Record<ActivityRow['result'], string> = {
  completed: '완료',
  failed: '실패',
  skipped: '건너뜀',
  waiting: '승인 대기',
  cancelled: '취소',
};

export const MODULE_NAME: Record<string, string> = {
  backup: '자동 백업',
  downloads: '다운로드 정리',
  autopaste: '클립보드 정리',
  core: '시스템',
};

export function formatTime(iso: string | null | undefined, now = new Date()): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  const sameDay = d.toDateString() === now.toDateString();
  const hm = d.toLocaleTimeString('ko-KR', { hour: '2-digit', minute: '2-digit' });
  if (sameDay) return `오늘 ${hm}`;
  return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

/** 단축키 문자열을 사람이 읽는 형태로 바꾼다. */
export function prettyShortcut(s: string, platform: string): string {
  const mac = platform === 'macos';
  return s
    .split('+')
    .map((k) => {
      const t = k.trim();
      if (/^(CommandOrControl|CmdOrCtrl)$/i.test(t)) return mac ? '⌘' : 'Ctrl';
      if (/^alt|option$/i.test(t)) return mac ? '⌥' : 'Alt';
      if (/^shift$/i.test(t)) return mac ? '⇧' : 'Shift';
      if (/^Key[A-Z]$/.test(t)) return t.slice(3);
      if (/^Digit\d$/.test(t)) return t.slice(5);
      return t;
    })
    .join(mac ? '' : ' + ');
}
