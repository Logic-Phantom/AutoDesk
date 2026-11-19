import { Check, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api, type JobRow } from '../lib/api';
import { formatTime } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Card, Empty, Notice } from '../components/ui';

const CATEGORIES = ['Documents/PDF', 'Documents/Office', 'Documents/Text', 'Images', 'Videos', 'Audio', 'Archives', 'Installers', 'Unknown'];

interface Payload {
  path: string;
  proposed?: string;
  category?: string;
  confident?: boolean;
}

function fileName(p: string) {
  return p.split(/[\\/]/).pop() ?? p;
}

export function Approvals({ tick }: { tick: unknown }) {
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [choice, setChoice] = useState<Record<string, string>>({});
  const { message, run, busy } = useAction();

  useEffect(() => {
    api.listApprovals().then(setJobs).catch(() => setJobs([]));
  }, [tick, message]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>승인 센터</h1>
          <p className="muted">확인이 필요한 작업만 모입니다. 승인하기 전에는 파일을 옮기지 않습니다.</p>
        </div>
      </header>
      <Notice message={message} />
      <Card>
        {jobs.length === 0 ? (
          <Empty>확인할 작업이 없습니다.</Empty>
        ) : (
          <ul className="list">
            {jobs.map((j) => {
              const p = JSON.parse(j.payloadJson) as Payload;
              const cat = choice[j.id] ?? p.category ?? 'Unknown';
              return (
                <li key={j.id} className="list-row wrap">
                  <div className="ellipsis grow">
                    <div className="strong ellipsis">{fileName(p.path)}</div>
                    <div className="muted small">
                      Download Butler · {formatTime(j.createdAt)} {p.confident === false && '· 분류 불확실 — 확인 필요'}
                    </div>
                  </div>
                  <select className="input" value={cat} aria-label="이동할 폴더" onChange={(e) => setChoice({ ...choice, [j.id]: e.target.value })}>
                    {CATEGORIES.map((c) => (
                      <option key={c} value={c}>{c}</option>
                    ))}
                  </select>
                  <button className="btn primary" disabled={busy} onClick={() => run(() => api.approveJob(j.id, cat), '이동했습니다(이력에서 되돌릴 수 있음)')}>
                    <Check size={14} aria-hidden /> 이동
                  </button>
                  <button className="btn" disabled={busy} onClick={() => run(() => api.rejectJob(j.id), '그대로 두었습니다')}>
                    <X size={14} aria-hidden /> 그대로 두기
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </Card>
    </div>
  );
}
