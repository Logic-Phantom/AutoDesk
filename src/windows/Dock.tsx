// 업무 모드 플로팅 독(화면 오른쪽 아래). 접힘 → 펼침 → (퇴근 확인 / 결과) 순서로 창 크기를 내용에 맞춘다.
import { listen } from '@tauri-apps/api/event';
import { Check, ChevronRight, Coffee, Loader2, Moon, Settings2, Sun, TriangleAlert, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { errorMessage } from '../lib/api';
import { tools, type AppWindow, type ModeResult, type WorkspaceConfig } from '../lib/toolsApi';

type View = 'closed' | 'open' | 'confirm' | 'result';
type Mode = 'start' | 'away' | 'end';

const MODES: { id: Mode; label: string; sub: string; icon: typeof Sun }[] = [
  { id: 'start', label: '업무 시작', sub: '프로그램·사이트 열기', icon: Sun },
  { id: 'away', label: '자리 비움', sub: '화면 잠금', icon: Coffee },
  { id: 'end', label: '퇴근', sub: '정리하고 닫기', icon: Moon },
];

export function Dock() {
  const [view, setView] = useState<View>('closed');
  const [busy, setBusy] = useState<Mode | null>(null);
  const [cfg, setCfg] = useState<WorkspaceConfig | null>(null);
  const [preview, setPreview] = useState<AppWindow[]>([]);
  const [result, setResult] = useState<ModeResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    document.documentElement.classList.add('dock-root');
    const load = () => tools.workspaceGet().then(setCfg).catch(() => undefined);
    load();
    const un = listen('workspace://changed', load);
    return () => { un.then((f) => f()); };
  }, []);

  // 내용 크기에 맞춰 OS 창 크기 조절(투명 영역이 다른 창 클릭을 막지 않도록)
  useLayoutEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const fit = () => {
      const r = el.getBoundingClientRect();
      tools.dockSetSize(Math.ceil(r.width) + 28, Math.ceil(r.height) + 28); // 여백 14px×2(그림자 포함)
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [view]);

  const awaySub = cfg?.awayAction === 'display_off' ? '모니터 끄기' : cfg?.awayAction === 'none' ? '자동화만 정리' : '화면 잠금';

  async function run(mode: Mode) {
    setError(null);
    if (mode === 'end') {
      const p = await tools.workspacePreviewEnd().catch(() => []);
      setPreview(p);
      setView('confirm');
      return;
    }
    await execute(mode);
  }

  async function execute(mode: Mode) {
    setBusy(mode);
    try {
      const r = await tools.workspaceRun(mode);
      setResult(r);
      setView('result');
      window.clearTimeout(timer.current);
      const allOk = r.steps.every((s) => s.ok);
      const scheduled = r.steps.some((s) => s.label.includes('PC 종료 예약'));
      if (allOk && !scheduled) timer.current = window.setTimeout(() => setView('closed'), 5000);
    } catch (e) {
      setError(errorMessage(e));
      setView('open');
    } finally {
      setBusy(null);
    }
  }

  const endCount = (cfg?.endCommands.length ?? 0) + (cfg?.endClose.length ?? 0);

  return (
    <div className={`dock-wrap ${view}`}>
      <div ref={boxRef} className={`dock ${view}`}>
        {view === 'closed' && (
          <button className="dock-fab" onClick={() => setView('open')} aria-label="업무 모드 열기" title="업무 모드">
            <span className="dock-logo">A</span>
          </button>
        )}

        {view === 'open' && (
          <div className="dock-bar" role="toolbar" aria-label="업무 모드">
            {MODES.map(({ id, label, sub, icon: Icon }) => (
              <button key={id} className={`dock-btn ${id}`} disabled={!!busy} onClick={() => run(id)} title={label}>
                <span className="dock-ico">{busy === id ? <Loader2 size={18} className="spin" /> : <Icon size={18} />}</span>
                <span className="dock-txt">
                  <b>{label}</b>
                  <small>{id === 'away' ? awaySub : sub}</small>
                </span>
              </button>
            ))}
            <div className="dock-side">
              <button className="dock-mini" onClick={() => tools.openMain('workspace')} aria-label="업무 모드 설정" title="설정"><Settings2 size={15} /></button>
              <button className="dock-mini" onClick={() => setView('closed')} aria-label="접기" title="접기"><ChevronRight size={15} /></button>
            </div>
            {error && <div className="dock-error">{error}</div>}
          </div>
        )}

        {view === 'confirm' && (
          <div className="dock-panel">
            <header><Moon size={16} /> <b>퇴근 모드를 실행할까요?</b></header>
            {endCount === 0 ? (
              <p className="dock-muted">아직 등록한 정리 항목이 없습니다. 설정에서 닫을 프로그램과 종료 스크립트를 등록하세요.</p>
            ) : (
              <ul className="dock-list">
                {cfg?.endCommands.map((c, i) => <li key={`c${i}`}>▶ {c.label || c.path}</li>)}
                {cfg?.endClose.map((c, i) => {
                  const n = preview.filter((p) => p.process.toLowerCase() === c.process.toLowerCase()).length;
                  return <li key={`p${i}`}>✕ {c.label || c.process} <span className="dock-muted">{n ? `창 ${n}개 닫기` : '실행 중 아님'}</span></li>;
                })}
                {cfg?.endAfter === 'lock' && <li>🔒 마지막에 화면 잠금</li>}
                {cfg?.endAfter === 'shutdown' && <li>⏻ 모두 닫히면 60초 뒤 PC 종료(취소 가능)</li>}
              </ul>
            )}
            <p className="dock-muted small">저장하지 않은 문서가 있으면 각 프로그램이 저장할지 물어봅니다.{cfg?.endForceKill ? ' 응답이 없으면 강제 종료합니다.' : ''}</p>
            <div className="dock-actions">
              <button className="dock-ghost" onClick={() => setView('open')}>취소</button>
              <button className="dock-primary" disabled={!!busy || endCount === 0} onClick={() => execute('end')}>
                {busy ? <Loader2 size={14} className="spin" /> : null} 퇴근 실행
              </button>
            </div>
          </div>
        )}

        {view === 'result' && result && (
          <div className="dock-panel">
            <header>
              {result.steps.every((s) => s.ok) ? <Check size={16} className="ok" /> : <TriangleAlert size={16} className="warn" />}
              <b>{{ start: '업무 시작', away: '자리 비움', end: '퇴근' }[result.mode]} 완료</b>
              <span className="spacer" />
              <button className="dock-mini" onClick={() => setView('closed')} aria-label="닫기"><X size={14} /></button>
            </header>
            <ul className="dock-list">
              {result.steps.map((s, i) => (
                <li key={i} className={s.ok ? '' : 'bad'}>{s.ok ? '✓' : '!'} {s.label} <span className="dock-muted">{s.detail}</span></li>
              ))}
            </ul>
            {result.steps.some((s) => s.label.includes('PC 종료 예약') && s.ok) && (
              <div className="dock-actions">
                <button className="dock-primary danger" onClick={() => tools.cancelShutdown().then(() => setView('closed'))}>PC 종료 취소</button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
