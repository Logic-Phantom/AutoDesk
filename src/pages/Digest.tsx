import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { ChevronLeft, ChevronRight, Copy, Download } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import type { ActivityRow } from '../lib/api';
import { MODULE_NAME, formatTime } from '../lib/format';
import { tools } from '../lib/toolsApi';
import { useAction } from '../lib/useCore';
import { Card, Empty, Notice, Row, Toggle } from '../components/ui';

interface ModuleCount {
  moduleId: string;
  label: string;
  completed: number;
  failed: number;
  waiting: number;
  skipped: number;
}
interface DigestData {
  day: string;
  completed: number;
  failed: number;
  pendingApprovals: number;
  modules: ModuleCount[];
  highlights: string[];
  failures: ActivityRow[];
}
interface DigestSettings {
  notifyEnabled: boolean;
  notifyTime: string;
  skipIfEmpty: boolean;
}

const pad = (n: number) => String(n).padStart(2, '0');
const ymd = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const shift = (day: string, delta: number) => {
  const [y, m, d] = day.split('-').map(Number);
  return ymd(new Date(y, m - 1, d + delta));
};

export function Digest({ tick }: { tick: unknown }) {
  const today = ymd(new Date());
  const [day, setDay] = useState(today);
  const [data, setData] = useState<DigestData | null>(null);
  const [settings, setSettings] = useState<DigestSettings | null>(null);
  const { message, run } = useAction();

  const load = useCallback(() => invoke<DigestData>('digest_get', { day }).then(setData).catch(() => setData(null)), [day]);
  useEffect(() => {
    load();
  }, [load, tick]);
  useEffect(() => {
    invoke<DigestSettings>('digest_settings_get').then(setSettings);
  }, []);

  const saveSettings = (s: DigestSettings, ok: string) =>
    run(() => invoke<void>('digest_settings_set', { settings: s }).then(() => setSettings(s)), ok);

  async function saveFile() {
    const path = await save({
      title: '일일 요약 저장',
      defaultPath: `AutoDesk-요약-${day}.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }, { name: 'HTML', extensions: ['html'] }],
    });
    if (!path) return;
    await run(() => invoke<void>('digest_save', { path, day }), `저장했습니다: ${path}`);
  }

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>일일 요약</h1>
          <p className="muted">AutoDesk가 그날 실제로 처리한 일만 모았습니다. PC의 다른 활동이나 파일 내용은 보지 않습니다.</p>
        </div>
        <div className="inline tight">
          <button className="icon-btn" aria-label="전날" onClick={() => setDay(shift(day, -1))}><ChevronLeft size={18} /></button>
          <input className="input" type="date" value={day} max={today} onChange={(e) => e.target.value && setDay(e.target.value)} aria-label="날짜" />
          <button className="icon-btn" aria-label="다음 날" disabled={day >= today} onClick={() => setDay(shift(day, 1))}><ChevronRight size={18} /></button>
        </div>
      </header>
      <Notice message={message} />

      {data && (
        <>
          <div className="stats three">
            <div className="stat static"><span className="stat-num">{data.completed}</span><span className="stat-label">처리 완료</span></div>
            <div className="stat static"><span className="stat-num">{data.failed}</span><span className="stat-label">실패</span></div>
            <div className="stat static"><span className="stat-num">{data.pendingApprovals}</span><span className="stat-label">지금 확인 대기</span></div>
          </div>

          <Card
            title={day === today ? '오늘 한 일' : `${day} 한 일`}
            actions={
              <div className="inline tight">
                <button className="btn small" onClick={() => run(async () => tools.textCopy(await invoke<string>('digest_markdown', { day })), 'Markdown으로 복사했습니다')}>
                  <Copy size={14} aria-hidden /> 복사
                </button>
                <button className="btn small" onClick={saveFile}><Download size={14} aria-hidden /> 저장</button>
              </div>
            }
          >
            {data.highlights.length === 0 ? (
              <Empty>이날 AutoDesk가 처리한 일이 없습니다.</Empty>
            ) : (
              <ul className="bullets digest-list">
                {data.highlights.map((h) => <li key={h}>{h}</li>)}
              </ul>
            )}
          </Card>

          {data.modules.length > 0 && (
            <Card title="기능별">
              <table className="table">
                <thead><tr><th>기능</th><th>완료</th><th>실패</th><th>승인 대기</th><th>건너뜀</th></tr></thead>
                <tbody>
                  {data.modules.map((m) => (
                    <tr key={m.moduleId}>
                      <td>{m.label}</td>
                      <td>{m.completed}</td>
                      <td className={m.failed ? 'err-text' : ''}>{m.failed}</td>
                      <td>{m.waiting}</td>
                      <td className="muted">{m.skipped}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
          )}

          {data.failures.length > 0 && (
            <Card title="실패 목록">
              <ul className="list">
                {data.failures.map((f) => (
                  <li key={f.id} className="list-row">
                    <div className="ellipsis">
                      <div>{f.message} {f.errorCode && <span className="muted small">({f.errorCode})</span>}</div>
                      <div className="muted small">{MODULE_NAME[f.moduleId] ?? f.moduleId} · {f.target ?? ''} · {formatTime(f.createdAt)}</div>
                    </div>
                  </li>
                ))}
              </ul>
            </Card>
          )}
        </>
      )}

      {settings && (
        <Card title="하루 요약 알림">
          <Row label="정해진 시각에 오늘 요약 알림" hint="하루 한 번, 그 시각 이후 처음 확인할 때 OS 알림으로 보냅니다(앱이 켜져 있을 때).">
            <Toggle label="요약 알림" checked={settings.notifyEnabled} onChange={(v) => saveSettings({ ...settings, notifyEnabled: v }, v ? '요약 알림을 켰습니다' : '요약 알림을 껐습니다')} />
          </Row>
          <Row label="알림 시각">
            <input className="input" type="time" value={settings.notifyTime} onChange={(e) => e.target.value && saveSettings({ ...settings, notifyTime: e.target.value }, `${e.target.value}에 알립니다`)} />
          </Row>
          <Row label="처리한 일이 없는 날은 알리지 않기">
            <Toggle label="빈 날 건너뛰기" checked={settings.skipIfEmpty} onChange={(v) => saveSettings({ ...settings, skipIfEmpty: v }, '저장했습니다')} />
          </Row>
        </Card>
      )}
    </div>
  );
}
