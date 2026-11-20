import { AppWindow, Crop, Monitor, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { prettyShortcut } from '../../lib/format';
import { tools, type CaptureWindow, type ToolSettings } from '../../lib/toolsApi';
import { useAction } from '../../lib/useCore';
import { Card, Empty, Notice, Row, Toggle } from '../../components/ui';

export function CaptureStudio({ platform }: { platform: string }) {
  const [wins, setWins] = useState<CaptureWindow[]>([]);
  const [settings, setSettings] = useState<ToolSettings | null>(null);
  const [shortcut, setShortcut] = useState('');
  const { message, run, busy } = useAction();

  const loadWins = () => tools.captureWindows().then(setWins).catch(() => setWins([]));
  useEffect(() => {
    loadWins();
    tools.settingsGet().then((s) => { setSettings(s); setShortcut(s.captureShortcut); });
  }, []);

  const saveSettings = (s: ToolSettings, ok: string) => run(() => tools.settingsSet(s).then(() => setSettings(s)), ok);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>화면 캡처</h1>
          <p className="muted">화면을 캡처해서 화살표·도형·번호·텍스트·모자이크를 넣고, 잘라서 복사하거나 저장합니다. 캡처는 저장하기 전까지 메모리에만 있습니다.</p>
        </div>
      </header>
      <div className="capture-actions">
        <button className="capture-card" disabled={busy} onClick={() => run(() => tools.captureStart('region'))}>
          <Crop size={28} aria-hidden />
          <b>영역 캡처</b>
          <span className="muted small">드래그해서 원하는 부분만{settings?.captureShortcutEnabled ? ` · ${prettyShortcut(settings.captureShortcut, platform)}` : ''}</span>
        </button>
        <button className="capture-card" disabled={busy} onClick={() => run(() => tools.captureStart('full'))}>
          <Monitor size={28} aria-hidden />
          <b>화면 전체</b>
          <span className="muted small">마우스가 있는 모니터 전체</span>
        </button>
      </div>
      <Notice message={message} />

      <Card title="창 캡처" actions={<button className="icon-btn" aria-label="창 목록 새로고침" onClick={loadWins}><RefreshCw size={16} /></button>}>
        {wins.length === 0 ? <Empty>캡처할 수 있는 창이 없습니다.</Empty> : (
          <ul className="list scroll tall">
            {wins.map((w) => (
              <li key={w.id}>
                <button className="list-row selectable" onClick={() => run(() => tools.captureWindow(w.id))}>
                  <AppWindow size={16} className="muted" aria-hidden />
                  <span className="ellipsis grow">{w.title}</span>
                  <span className="muted small nowrap">{w.app} · {w.width}×{w.height}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      {settings && (
        <Card title="단축키">
          <Row label="영역 캡처 단축키 사용" hint="전체 자동화 스위치와 상관없이 동작합니다">
            <Toggle label="캡처 단축키" checked={settings.captureShortcutEnabled} onChange={(v) => saveSettings({ ...settings, captureShortcutEnabled: v }, v ? '단축키를 켰습니다' : '단축키를 껐습니다')} />
          </Row>
          <label className="field">
            <span>단축키 (예: CommandOrControl+Alt+Shift+KeyS)</span>
            <div className="inline">
              <input className="input mono" value={shortcut} onChange={(e) => setShortcut(e.target.value)} />
              <button className="btn" onClick={() => saveSettings({ ...settings, captureShortcut: shortcut }, '단축키를 다시 등록했습니다')}>적용</button>
            </div>
          </label>
        </Card>
      )}
      <p className="muted small">편집기 안에서 &lsquo;글자 추출&rsquo;을 누르면 OS 내장 OCR(Windows OCR / macOS Vision)로 이미지 속 글자를 PC 안에서 읽어냅니다.</p>
    </div>
  );
}
