import { open } from '@tauri-apps/plugin-dialog';
import { AppWindow as AppIcon, ArrowDown, ArrowUp, Coffee, FileText, Globe, Moon, Play, Plus, Save, Sun, Terminal, Trash2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { fileName, tools, type AppWindow, type ModeResult, type WorkspaceConfig } from '../lib/toolsApi';
import { useAction } from '../lib/useCore';
import { Card, Empty, Notice, Row, Toggle } from '../components/ui';

function move<T>(arr: T[], i: number, d: -1 | 1): T[] {
  const j = i + d;
  if (j < 0 || j >= arr.length) return arr;
  const c = arr.slice();
  [c[i], c[j]] = [c[j], c[i]];
  return c;
}

export function Workspace() {
  const [cfg, setCfg] = useState<WorkspaceConfig | null>(null);
  const [dirty, setDirty] = useState(false);
  const [url, setUrl] = useState('');
  const [running, setRunning] = useState<AppWindow[]>([]);
  const [proc, setProc] = useState('');
  const [result, setResult] = useState<ModeResult | null>(null);
  const { message, run, busy } = useAction();

  useEffect(() => {
    tools.workspaceGet().then(setCfg);
    tools.runningApps().then(setRunning).catch(() => setRunning([]));
  }, []);

  if (!cfg) return <div className="page"><p className="muted">불러오는 중…</p></div>;
  const set = (patch: Partial<WorkspaceConfig>) => {
    setCfg({ ...cfg, ...patch });
    setDirty(true);
  };

  async function addApp() {
    const p = await open({ multiple: false, directory: false, title: '실행할 프로그램', filters: [{ name: '프로그램', extensions: ['exe', 'lnk', 'bat', 'cmd', 'app'] }, { name: '모든 파일', extensions: ['*'] }] });
    if (typeof p === 'string') set({ startItems: [...cfg!.startItems, { kind: 'app', target: p, args: '', label: fileName(p).replace(/\.(exe|lnk)$/i, '') }] });
  }
  async function addFile(directory: boolean) {
    const p = await open({ multiple: false, directory, title: directory ? '열 폴더' : '열 문서' });
    if (typeof p === 'string') set({ startItems: [...cfg!.startItems, { kind: 'file', target: p, args: '', label: fileName(p) }] });
  }
  function addUrl() {
    const u = url.trim();
    if (!u) return;
    const full = /^https?:\/\//i.test(u) ? u : `https://${u}`;
    set({ startItems: [...cfg!.startItems, { kind: 'url', target: full, args: '', label: full.replace(/^https?:\/\//, '').split('/')[0] }] });
    setUrl('');
  }
  async function addCommand() {
    const p = await open({ multiple: false, directory: false, title: '퇴근 때 실행할 스크립트(예: Tomcat shutdown.bat)', filters: [{ name: '스크립트·프로그램', extensions: ['bat', 'cmd', 'ps1', 'exe', 'sh'] }] });
    if (typeof p === 'string') set({ endCommands: [...cfg!.endCommands, { label: fileName(p), path: p, args: '', cwd: '' }] });
  }
  function addClose(process: string) {
    const p = process.trim();
    if (!p || cfg!.endClose.some((c) => c.process.toLowerCase() === p.toLowerCase())) return;
    set({ endClose: [...cfg!.endClose, { process: p, label: '' }] });
    setProc('');
  }

  async function saveCfg() {
    const ok = await run(() => tools.workspaceSet(cfg!), '저장했습니다');
    if (ok !== undefined) setDirty(false);
  }

  async function test(mode: 'start' | 'away' | 'end') {
    if (dirty) await tools.workspaceSet(cfg!);
    setDirty(false);
    const r = await run(() => tools.workspaceRun(mode));
    if (r) setResult(r);
  }

  const uniqueRunning = Array.from(new Map(running.map((r) => [r.process.toLowerCase(), r])).values());
  const kindIcon = { app: AppIcon, url: Globe, file: FileText };

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>업무 모드</h1>
          <p className="muted">화면 오른쪽 아래 플로팅 버튼으로 업무 시작 · 자리 비움 · 퇴근을 한 번에 실행합니다. 여기서 등록한 것만 실행·종료합니다.</p>
        </div>
        <button className="btn primary" disabled={!dirty || busy} onClick={saveCfg}><Save size={16} aria-hidden /> 저장</button>
      </header>
      <Notice message={message} />

      <Card>
        <Row label="플로팅 버튼 표시" hint="화면 오른쪽 아래에 항상 위에 떠 있습니다. 끄면 트레이 메뉴와 이 화면에서만 실행할 수 있습니다.">
          <Toggle label="플로팅 버튼" checked={cfg.dockEnabled} onChange={(v) => set({ dockEnabled: v })} />
        </Row>
      </Card>

      <Card title={<span className="inline tight"><Sun size={18} className="c-start" /> 업무 시작</span>} actions={<button className="btn small" onClick={() => test('start')}><Play size={14} /> 지금 실행</button>}>
        <p className="muted small">순서대로 엽니다. 프로그램·문서·폴더·웹사이트(기본 브라우저)를 등록하세요.</p>
        {cfg.startItems.length === 0 ? <Empty>등록한 항목이 없습니다.</Empty> : (
          <ul className="list">
            {cfg.startItems.map((it, i) => {
              const Icon = kindIcon[it.kind];
              return (
                <li key={i} className="list-row">
                  <Icon size={16} className="muted" aria-hidden />
                  <input className="input grow-input" value={it.label} onChange={(e) => set({ startItems: cfg.startItems.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)) })} aria-label="이름" />
                  <span className="mono small muted ellipsis grow">{it.target}</span>
                  {it.kind === 'app' && (
                    <input className="input" style={{ width: 150 }} placeholder="실행 인자(선택)" value={it.args} onChange={(e) => set({ startItems: cfg.startItems.map((x, j) => (j === i ? { ...x, args: e.target.value } : x)) })} aria-label="실행 인자" />
                  )}
                  <button className="icon-btn" aria-label="위로" onClick={() => set({ startItems: move(cfg.startItems, i, -1) })}><ArrowUp size={14} /></button>
                  <button className="icon-btn" aria-label="아래로" onClick={() => set({ startItems: move(cfg.startItems, i, 1) })}><ArrowDown size={14} /></button>
                  <button className="icon-btn" aria-label="삭제" onClick={() => set({ startItems: cfg.startItems.filter((_, j) => j !== i) })}><Trash2 size={14} /></button>
                </li>
              );
            })}
          </ul>
        )}
        <div className="inline">
          <button className="btn small" onClick={addApp}><Plus size={14} /> 프로그램</button>
          <button className="btn small" onClick={() => addFile(false)}><Plus size={14} /> 문서</button>
          <button className="btn small" onClick={() => addFile(true)}><Plus size={14} /> 폴더</button>
          <input className="input" placeholder="웹 주소 (예: mail.company.com)" value={url} onChange={(e) => setUrl(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && addUrl()} />
          <button className="btn small" onClick={addUrl} disabled={!url.trim()}><Plus size={14} /> 사이트</button>
        </div>
        <Row label="꺼져 있던 자동화 다시 켜기"><Toggle label="자동화 켜기" checked={cfg.startResumeAutomation} onChange={(v) => set({ startResumeAutomation: v })} /></Row>
      </Card>

      <Card title={<span className="inline tight"><Coffee size={18} className="c-away" /> 자리 비움</span>} actions={<button className="btn small" onClick={() => test('away')}><Play size={14} /> 지금 실행</button>}>
        <Row label="동작">
          <select className="input" value={cfg.awayAction} onChange={(e) => set({ awayAction: e.target.value as WorkspaceConfig['awayAction'] })}>
            <option value="lock">화면 잠금(Win+L과 같음)</option>
            <option value="display_off">모니터만 끄기(잠그지 않음)</option>
            <option value="none">아무것도 안 함</option>
          </select>
        </Row>
        <Row label="자동화 일시정지" hint="돌아와서 업무 시작을 누르면 다시 켜집니다"><Toggle label="자리 비움 시 자동화 정지" checked={cfg.awayPauseAutomation} onChange={(v) => set({ awayPauseAutomation: v })} /></Row>
      </Card>

      <Card title={<span className="inline tight"><Moon size={18} className="c-end" /> 퇴근</span>} actions={<button className="btn small" onClick={() => test('end')}><Play size={14} /> 지금 실행</button>}>
        <h3>1. 먼저 실행할 정리 명령</h3>
        <p className="muted small">예: Tomcat <code>bin\shutdown.bat</code>, 빌드 서버 종료 스크립트. 등록한 순서대로 실행하고 끝날 때까지(최대 90초) 기다립니다.</p>
        {cfg.endCommands.length === 0 ? <Empty>등록한 명령이 없습니다.</Empty> : (
          <ul className="list">
            {cfg.endCommands.map((c, i) => (
              <li key={i} className="list-row">
                <Terminal size={16} className="muted" aria-hidden />
                <input className="input grow-input" value={c.label} onChange={(e) => set({ endCommands: cfg.endCommands.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)) })} aria-label="이름" />
                <span className="mono small muted ellipsis grow">{c.path}</span>
                <input className="input" style={{ width: 130 }} placeholder="인자(선택)" value={c.args} onChange={(e) => set({ endCommands: cfg.endCommands.map((x, j) => (j === i ? { ...x, args: e.target.value } : x)) })} aria-label="인자" />
                <button className="icon-btn" aria-label="삭제" onClick={() => set({ endCommands: cfg.endCommands.filter((_, j) => j !== i) })}><Trash2 size={14} /></button>
              </li>
            ))}
          </ul>
        )}
        <button className="btn small" onClick={addCommand}><Plus size={14} /> 스크립트 추가</button>

        <h3>2. 닫을 프로그램</h3>
        <p className="muted small">각 프로그램에 '정상 종료'를 요청합니다. 저장하지 않은 문서가 있으면 그 프로그램이 저장 여부를 직접 묻습니다. 탐색기·시스템 프로세스·AutoDesk는 닫지 않습니다.</p>
        <div className="chips">
          {cfg.endClose.map((c, i) => (
            <span key={i} className="chip">
              {c.process}
              <button aria-label={`${c.process} 빼기`} onClick={() => set({ endClose: cfg.endClose.filter((_, j) => j !== i) })}><X size={12} /></button>
            </span>
          ))}
          {cfg.endClose.length === 0 && <span className="muted small">아직 없음</span>}
        </div>
        <div className="inline">
          <select className="input" value="" onChange={(e) => addClose(e.target.value)} aria-label="실행 중인 프로그램에서 고르기">
            <option value="">지금 실행 중인 프로그램에서 고르기…</option>
            {uniqueRunning.map((r) => <option key={r.process} value={r.process}>{r.process} — {r.title.slice(0, 40)}</option>)}
          </select>
          <input className="input" placeholder="직접 입력 (예: EXCEL.EXE)" value={proc} onChange={(e) => setProc(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && addClose(proc)} />
          <button className="btn small" onClick={() => addClose(proc)} disabled={!proc.trim()}><Plus size={14} /> 추가</button>
          <button className="btn small" onClick={() => tools.runningApps().then(setRunning)}>목록 새로고침</button>
        </div>
        <Row label="닫힐 때까지 기다리는 시간(초)"><input className="input num" type="number" min={5} max={120} value={cfg.endCloseTimeoutSec} onChange={(e) => set({ endCloseTimeoutSec: Number(e.target.value) })} /></Row>
        <Row label="응답 없으면 강제 종료" hint={<span className="warn-text">저장하지 않은 내용이 사라질 수 있습니다. 꼭 필요한 경우에만 켜세요.</span>}>
          <Toggle label="강제 종료" checked={cfg.endForceKill} onChange={(v) => set({ endForceKill: v })} />
        </Row>
        <h3>3. 마무리</h3>
        <Row label="모두 끝난 뒤">
          <select className="input" value={cfg.endAfter} onChange={(e) => set({ endAfter: e.target.value as WorkspaceConfig['endAfter'] })}>
            <option value="none">그대로 두기</option>
            <option value="lock">화면 잠금</option>
            <option value="shutdown">PC 종료(60초 뒤, 닫히지 않은 창이 있으면 건너뜀)</option>
          </select>
        </Row>
        <Row label="자동화 일시정지"><Toggle label="퇴근 시 자동화 정지" checked={cfg.endPauseAutomation} onChange={(v) => set({ endPauseAutomation: v })} /></Row>
      </Card>

      {result && (
        <Card title="마지막 실행 결과">
          <ul className="list">
            {result.steps.map((s, i) => (
              <li key={i} className="list-row">
                <span className={s.ok ? 'ok-text' : 'err-text'}>{s.ok ? '✓' : '!'}</span>
                <span className="grow">{s.label}</span>
                <span className="muted small">{s.detail}</span>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
