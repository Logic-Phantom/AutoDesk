import { formatBytes, formatTime, moduleTone, prettyShortcut } from './format';

describe('format helpers', () => {
  it('formats bytes', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2.0 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
  });

  it('formats time relative to today', () => {
    const now = new Date('2026-11-19T10:00:00');
    expect(formatTime(null, now)).toBe('—');
    expect(formatTime('2026-11-19T09:30:00', now)).toMatch(/^오늘/);
    expect(formatTime('2026-11-17T09:30:00', now)).toMatch(/^11\/17/);
  });

  it('maps module status to tone', () => {
    expect(moduleTone('RUNNING')).toBe('ok');
    expect(moduleTone('ERROR_PAUSED')).toBe('err');
    expect(moduleTone('DISABLED')).toBe('off');
    expect(moduleTone('PAUSED_GLOBAL')).toBe('warn');
  });

  it('pretty prints shortcuts per OS', () => {
    expect(prettyShortcut('CommandOrControl+Alt+Shift+KeyV', 'windows')).toBe('Ctrl + Alt + Shift + V');
    expect(prettyShortcut('CommandOrControl+Alt+Shift+KeyV', 'macos')).toBe('⌘⌥⇧V');
  });
});
