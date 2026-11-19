import { Activity, CheckCircle2, Inbox, TriangleAlert } from 'lucide-react';
import { api, type Overview } from '../lib/api';
import { APP_STATUS_LABEL, MODULE_STATUS_LABEL, MODULE_NAME, RESULT_LABEL, formatTime, moduleTone } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Card, Empty, Notice, Pill, Row, Toggle } from '../components/ui';

export function Dashboard({ data, go }: { data: Overview; go: (p: string) => void }) {
  const { message, run } = useAction();
  const active = data.modules.filter((m) => m.status === 'RUNNING').length;
  const paused = data.status === 'GLOBAL_PAUSED';

  return (
    <div className="page">
      <header className="page-head">
        <div>
          <h1>대시보드</h1>
          <p className="muted">
            상태: <strong>{APP_STATUS_LABEL[data.status]}</strong> · 자동화 모듈 {active}/{data.modules.length}개 작동
          </p>
        </div>
      </header>

      <Card>
        <Row
          label="전체 자동화"
          hint={paused ? '모든 감시·단축키가 멈춰 있습니다. 대기 작업은 보존됩니다.' : '켜진 모듈이 파일 변경·다운로드·단축키를 처리합니다.'}
        >
          <Toggle
            label="전체 자동화"
            checked={data.masterEnabled}
            onChange={(v) => run(() => api.setMaster(v))}
          />
        </Row>
        <Notice message={message} />
      </Card>

      <div className="stats">
        <button className="stat" onClick={() => go('activity')}>
          <CheckCircle2 size={18} aria-hidden />
          <span className="stat-num">{data.todayCompleted}</span>
          <span className="stat-label">오늘 처리</span>
        </button>
        <button className="stat" onClick={() => go('approvals')}>
          <Inbox size={18} aria-hidden />
          <span className="stat-num">{data.pendingApprovals}</span>
          <span className="stat-label">확인 대기</span>
        </button>
        <button className="stat" onClick={() => go('activity')}>
          <TriangleAlert size={18} aria-hidden />
          <span className="stat-num">{data.todayFailed}</span>
          <span className="stat-label">오늘 오류</span>
        </button>
        <div className="stat static">
          <Activity size={18} aria-hidden />
          <span className="stat-num small">{data.backgroundMode ? '켜짐' : '꺼짐'}</span>
          <span className="stat-label">
            백그라운드 · 로그인 자동 실행 {data.launchAtLogin ? '켜짐' : '꺼짐'}
          </span>
        </div>
      </div>

      <div className="grid2">
        <Card title="모듈 상태" actions={<button className="link" onClick={() => go('automations')}>관리</button>}>
          <ul className="list">
            {data.modules.map((m) => (
              <li key={m.id} className="list-row">
                <div>
                  <div className="strong">{m.name}</div>
                  <div className="muted small">
                    {m.statusReason ?? `오늘 ${m.todayCount}건 · 마지막 ${formatTime(m.lastRunAt)}`}
                  </div>
                </div>
                <Pill tone={moduleTone(m.status)}>{MODULE_STATUS_LABEL[m.status]}</Pill>
              </li>
            ))}
          </ul>
        </Card>
        <Card title="최근 처리" actions={<button className="link" onClick={() => go('activity')}>전체 보기</button>}>
          {data.recent.length === 0 ? (
            <Empty>아직 처리한 작업이 없습니다.</Empty>
          ) : (
            <ul className="list">
              {data.recent.map((a) => (
                <li key={a.id} className="list-row">
                  <div className="ellipsis">
                    <div className="ellipsis">{a.message}</div>
                    <div className="muted small ellipsis">
                      {MODULE_NAME[a.moduleId] ?? a.moduleId} · {a.target ?? ''} · {formatTime(a.createdAt)}
                    </div>
                  </div>
                  <span className={`result ${a.result}`}>{RESULT_LABEL[a.result]}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </div>
  );
}
