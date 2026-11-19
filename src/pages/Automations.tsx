import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import { api, type ModuleView, type Overview } from '../lib/api';
import { MODULE_STATUS_LABEL, formatTime, moduleTone } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Notice, Pill, Toggle } from '../components/ui';
import { BackupPanel } from './BackupPanel';
import { DownloadsPanel } from './DownloadsPanel';
import { AutoPastePanel } from './AutoPastePanel';
import { LogWatchPanel } from './LogWatchPanel';

function ModuleCard({ m, data }: { m: ModuleView; data: Overview }) {
  const [open, setOpen] = useState(m.status === 'WAITING_PERMISSION');
  const { message, run } = useAction();
  return (
    <section className="card module">
      <header className="module-head">
        <button className="module-title" onClick={() => setOpen(!open)} aria-expanded={open}>
          {open ? <ChevronDown size={16} aria-hidden /> : <ChevronRight size={16} aria-hidden />}
          <span className="strong">{m.name}</span>
          <Pill tone={moduleTone(m.status)}>{MODULE_STATUS_LABEL[m.status]}</Pill>
        </button>
        <Toggle label={`${m.name} 켜기/끄기`} checked={m.enabled} onChange={(v) => run(() => api.setModuleEnabled(m.id, v))} />
      </header>
      <p className="muted">{m.description}</p>
      <p className="muted small">
        오늘 {m.todayCount}건 · 마지막 실행 {formatTime(m.lastRunAt)}
        {m.lastErrorCode && ` · 최근 오류 ${m.lastErrorCode}`}
      </p>
      {m.statusReason && <p className={`notice ${m.status === 'ERROR_PAUSED' ? 'err' : 'warn'}`}>{m.statusReason}</p>}
      <Notice message={message} />
      {open && (
        <div className="module-body">
          {m.id === 'backup' && <BackupPanel m={m} />}
          {m.id === 'downloads' && <DownloadsPanel m={m} data={data} />}
          {m.id === 'autopaste' && <AutoPastePanel m={m} data={data} />}
          {m.id === 'logwatch' && <LogWatchPanel m={m} tick={data} />}
        </div>
      )}
    </section>
  );
}

export function Automations({ data }: { data: Overview }) {
  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>자동화</h1>
          <p className="muted">
            모듈마다 독립적으로 켜고 끌 수 있습니다. 끄면 해당 모듈의 감시·단축키가 즉시 해제되고 다른 모듈은 계속 동작합니다.
          </p>
        </div>
      </header>
      {!data.masterEnabled && <p className="notice warn">전체 자동화가 꺼져 있어 모든 모듈이 일시정지 상태입니다.</p>}
      {data.modules.map((m) => (
        <ModuleCard key={m.id} m={m} data={data} />
      ))}
      <p className="muted small">
        향후 모듈(Document Guard, ProjectGuard, Daily Digest)은 아직 구현되지 않아 여기에 표시하지 않습니다. 로드맵은 README를 참고하세요.
      </p>
    </div>
  );
}
