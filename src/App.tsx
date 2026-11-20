import { listen } from '@tauri-apps/api/event';
import { Bot, Camera, CalendarCheck, Columns2, History, Images, Inbox, LayoutDashboard, Settings as SettingsIcon, Sunrise, Table2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { APP_STATUS_LABEL } from './lib/format';
import { useOverview } from './lib/useCore';
import { Dashboard } from './pages/Dashboard';
import { Automations } from './pages/Automations';
import { Activity } from './pages/Activity';
import { Approvals } from './pages/Approvals';
import { Settings, type Theme } from './pages/Settings';
import { Workspace } from './pages/Workspace';
import { Digest } from './pages/Digest';
import { CaptureStudio } from './pages/tools/CaptureStudio';
import { TableConverter } from './pages/tools/TableConverter';
import { CompareDesk } from './pages/tools/CompareDesk';
import { ImageStudio } from './pages/tools/ImageStudio';

const NAV = [
  { id: 'dashboard', label: '대시보드', icon: LayoutDashboard, group: '' },
  { id: 'automations', label: '자동화', icon: Bot, group: '' },
  { id: 'activity', label: '이력', icon: History, group: '' },
  { id: 'approvals', label: '승인 센터', icon: Inbox, group: '' },
  { id: 'digest', label: '일일 요약', icon: CalendarCheck, group: '' },
  { id: 'workspace', label: '업무 모드', icon: Sunrise, group: '' },
  { id: 'capture', label: 'Capture Studio', icon: Camera, group: '도구' },
  { id: 'table', label: 'Table Converter', icon: Table2, group: '도구' },
  { id: 'compare', label: 'Compare Desk', icon: Columns2, group: '도구' },
  { id: 'images', label: 'Image Studio', icon: Images, group: '도구' },
  { id: 'settings', label: '설정', icon: SettingsIcon, group: '-' },
] as const;
type Page = (typeof NAV)[number]['id'];

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem('theme');
    if (t === 'light' || t === 'dark' || t === 'system') return t;
  } catch {
    /* 저장소를 못 쓰면 시스템 테마 */
  }
  return 'system';
}

function initialPage(): Page {
  // 트레이에서 특정 화면으로 열 때 창을 새로 만들며 ?page= 로 전달된다
  const p = new URLSearchParams(window.location.search).get('page');
  return NAV.some((n) => n.id === p) ? (p as Page) : 'dashboard';
}

export default function App() {
  const { data, error } = useOverview();
  const [page, setPage] = useState<Page>(initialPage);
  const [theme, setThemeState] = useState<Theme>(loadTheme);

  useEffect(() => {
    if (theme === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    const un = listen<string>('core://navigate', (e) => {
      if (NAV.some((n) => n.id === e.payload)) setPage(e.payload as Page);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const setTheme = (t: Theme) => {
    setThemeState(t);
    try {
      localStorage.setItem('theme', t);
    } catch {
      /* 무시 */
    }
  };

  if (!data) {
    return <div className="boot">{error ? `엔진 연결 실패: ${error}` : 'AutoDesk 시작 중…'}</div>;
  }

  const go = (p: string) => setPage(p as Page);

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="주 메뉴">
        <div className="brand">
          <span className="brand-mark" aria-hidden>A</span> AutoDesk
        </div>
        {NAV.map(({ id, label, icon: Icon, group }, i) => (
          <div key={id} className="nav-item">
          {group && group !== NAV[i - 1]?.group && (group === '-' ? <div className="nav-sep" /> : <div className="nav-group">{group}</div>)}
          <button className={`nav ${page === id ? 'active' : ''}`} onClick={() => setPage(id)} aria-current={page === id}>
            <Icon size={18} aria-hidden />
            <span>{label}</span>
            {id === 'approvals' && data.pendingApprovals > 0 && <span className="badge">{data.pendingApprovals}</span>}
          </button>
          </div>
        ))}
        <div className={`sidebar-status ${data.masterEnabled ? 'on' : 'off'}`}>
          <span className="dot" aria-hidden /> {APP_STATUS_LABEL[data.status]}
        </div>
      </nav>
      <main className="main">
        {page === 'dashboard' && <Dashboard data={data} go={go} />}
        {page === 'automations' && <Automations data={data} />}
        {page === 'activity' && <Activity tick={data} />}
        {page === 'approvals' && <Approvals tick={data} />}
        {page === 'workspace' && <Workspace />}
        {page === 'digest' && <Digest tick={data} />}
        {page === 'capture' && <CaptureStudio platform={data.platform} />}
        {/* 도구 화면은 다른 메뉴로 가도 입력이 사라지지 않도록 숨김 처리만 한다 */}
        <div hidden={page !== 'table'}><TableConverter /></div>
        <div hidden={page !== 'compare'}><CompareDesk /></div>
        <div hidden={page !== 'images'}><ImageStudio /></div>
        {page === 'settings' && <Settings data={data} theme={theme} setTheme={setTheme} />}
      </main>
    </div>
  );
}
