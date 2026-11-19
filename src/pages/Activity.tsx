import { Undo2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api, type ActivityRow } from '../lib/api';
import { MODULE_NAME, RESULT_LABEL, formatTime } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Card, Empty, Notice } from '../components/ui';

const RESULTS = ['', 'completed', 'failed', 'waiting', 'cancelled', 'skipped'] as const;
const MODULES = ['', 'backup', 'downloads', 'autopaste', 'logwatch', 'core'] as const;

export function Activity({ tick }: { tick: unknown }) {
  const [result, setResult] = useState<string>('');
  const [moduleId, setModuleId] = useState<string>('');
  const [rows, setRows] = useState<ActivityRow[]>([]);
  const { message, run } = useAction();

  useEffect(() => {
    api.listActivity(result || undefined, moduleId || undefined).then(setRows).catch(() => setRows([]));
  }, [result, moduleId, tick, message]);

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>이력</h1>
          <p className="muted">실제로 처리한 작업만 기록합니다. 클립보드 내용은 기록하지 않습니다.</p>
        </div>
      </header>
      <div className="filters">
        <select className="input" value={result} onChange={(e) => setResult(e.target.value)} aria-label="결과 필터">
          {RESULTS.map((r) => (
            <option key={r} value={r}>{r ? RESULT_LABEL[r] : '전체 결과'}</option>
          ))}
        </select>
        <select className="input" value={moduleId} onChange={(e) => setModuleId(e.target.value)} aria-label="모듈 필터">
          {MODULES.map((m) => (
            <option key={m} value={m}>{m ? MODULE_NAME[m] : '전체 모듈'}</option>
          ))}
        </select>
      </div>
      <Notice message={message} />
      <Card>
        {rows.length === 0 ? (
          <Empty>조건에 맞는 기록이 없습니다.</Empty>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>시간</th>
                <th>모듈</th>
                <th>내용</th>
                <th>대상</th>
                <th>결과</th>
                <th aria-label="작업" />
              </tr>
            </thead>
            <tbody>
              {rows.map((a) => (
                <tr key={a.id}>
                  <td className="nowrap">{formatTime(a.createdAt)}</td>
                  <td className="nowrap">{MODULE_NAME[a.moduleId] ?? a.moduleId}</td>
                  <td>
                    {a.message}
                    {a.errorCode && <span className="muted small"> ({a.errorCode})</span>}
                  </td>
                  <td className="mono small">{a.target ?? ''}</td>
                  <td><span className={`result ${a.result}`}>{RESULT_LABEL[a.result]}</span></td>
                  <td>
                    {a.undoRef && (
                      <button className="btn small" onClick={() => run(() => api.undoMove(a.undoRef!), (p) => `원래 위치로 되돌렸습니다: ${p}`)}>
                        <Undo2 size={14} aria-hidden /> 되돌리기
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>
    </div>
  );
}
