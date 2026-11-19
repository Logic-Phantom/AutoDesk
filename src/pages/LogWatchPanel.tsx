import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { FilePlus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { api, type LogIssue, type ModuleView, type WatchTarget } from '../lib/api';
import { formatTime } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Empty, Notice, Row, Toggle } from '../components/ui';

interface LogWatchConfig {
  errorPatterns: string[];
  warnPatterns: string[];
  caseSensitive: boolean;
  notifyWarn: boolean;
  renotifyMinutes: number;
  contextLines: number;
}

/** 자주 쓰는 WAS 로그 패턴 묶음 */
const PRESETS: { label: string; error: string[]; warn: string[] }[] = [
  { label: '일반', error: ['ERROR', 'FATAL', 'SEVERE', 'Exception', '[ERR]'], warn: ['WARN', 'WARNING'] },
  { label: 'Tomcat', error: ['SEVERE', 'ERROR', 'Exception', 'OutOfMemoryError'], warn: ['WARNING', 'WARN'] },
  { label: 'JEUS/WebtoB', error: ['[ERROR]', '[FATAL]', 'Exception', 'ERROR'], warn: ['[WARNING]', '[WARN]', 'WARN'] },
];

const split = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);

export function LogWatchPanel({ m, tick }: { m: ModuleView; tick: unknown }) {
  const cfg = m.config as unknown as LogWatchConfig;
  const [targets, setTargets] = useState<WatchTarget[]>([]);
  const [issues, setIssues] = useState<LogIssue[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [errors, setErrors] = useState(cfg.errorPatterns.join(', '));
  const [warns, setWarns] = useState(cfg.warnPatterns.join(', '));
  const { message, run, busy } = useAction();

  const load = useCallback(async () => {
    setTargets(await api.listWatchTargets('logwatch'));
    setIssues(await api.listLogIssues());
  }, []);
  useEffect(() => {
    load();
  }, [load, tick, message]);

  const save = (patch: Partial<LogWatchConfig>, ok = '저장했습니다') =>
    run(() => api.setModuleConfig('logwatch', { ...cfg, ...patch }), ok);

  async function addFile() {
    const picked = await openDialog({
      multiple: false,
      directory: false,
      title: '감시할 로그 파일 선택',
      filters: [{ name: '로그', extensions: ['log', 'txt', 'out', 'err'] }, { name: '모든 파일', extensions: ['*'] }],
    });
    if (typeof picked !== 'string') return;
    await run(() => api.addWatchTarget('logwatch', picked, false), '등록했습니다. 지금부터 새로 쌓이는 줄만 검사합니다.');
    load();
  }

  return (
    <>
      <h3>감시할 로그 파일</h3>
      {targets.length === 0 ? (
        <Empty>아직 등록한 로그 파일이 없습니다.</Empty>
      ) : (
        <ul className="list">
          {targets.map((t) => (
            <li key={t.id} className="list-row">
              <span className="mono ellipsis">{t.path}</span>
              <button className="icon-btn" aria-label="로그 파일 제거" onClick={() => run(() => api.removeWatchTarget(t.id)).then(load)}>
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="inline">
        <button className="btn" onClick={addFile} disabled={busy}>
          <FilePlus size={16} aria-hidden /> 로그 파일 추가
        </button>
      </div>
      <p className="muted small">
        등록한 뒤에 추가되는 줄만 읽습니다. 파일이 새로 시작되면(로테이션) 처음부터 다시 읽고, 스택 트레이스(at …, Caused by)는 앞 오류에 묶어 보여줍니다.
        UTF-8과 EUC-KR 로그를 모두 읽습니다.
      </p>

      <h3>감지 규칙</h3>
      <div className="inline">
        <span className="small muted">패턴 묶음:</span>
        {PRESETS.map((p) => (
          <button
            key={p.label}
            className="btn small"
            onClick={() => {
              setErrors(p.error.join(', '));
              setWarns(p.warn.join(', '));
              save({ errorPatterns: p.error, warnPatterns: p.warn }, `${p.label} 패턴을 적용했습니다`);
            }}
          >
            {p.label}
          </button>
        ))}
      </div>
      <label className="field">
        <span>오류로 볼 문구(쉼표 구분)</span>
        <input className="input mono" value={errors} onChange={(e) => setErrors(e.target.value)} />
      </label>
      <label className="field">
        <span>경고로 볼 문구(쉼표 구분)</span>
        <input className="input mono" value={warns} onChange={(e) => setWarns(e.target.value)} />
      </label>
      <button className="btn primary" onClick={() => save({ errorPatterns: split(errors), warnPatterns: split(warns) })}>
        패턴 저장
      </button>
      <Row label="대소문자 구분" hint="끄면 error/Error/ERROR를 모두 잡습니다(오탐이 늘 수 있음)">
        <Toggle label="대소문자 구분" checked={cfg.caseSensitive} onChange={(v) => save({ caseSensitive: v })} />
      </Row>
      <Row label="경고(WARN)도 알림" hint="끄면 경고는 목록에만 기록합니다">
        <Toggle label="경고 알림" checked={cfg.notifyWarn} onChange={(v) => save({ notifyWarn: v })} />
      </Row>
      <Row label="같은 오류 다시 알림 간격(분)" hint="그 사이 반복된 횟수는 계속 집계합니다">
        <input className="input num" type="number" min={1} max={1440} value={cfg.renotifyMinutes}
          onChange={(e) => save({ renotifyMinutes: Number(e.target.value) })} />
      </Row>
      <Row label="함께 보관할 앞줄 수">
        <input className="input num" type="number" min={0} max={20} value={cfg.contextLines}
          onChange={(e) => save({ contextLines: Number(e.target.value) })} />
      </Row>

      <h3>감지된 오류 ({issues.length})</h3>
      {issues.length === 0 ? (
        <Empty>아직 감지된 오류가 없습니다.</Empty>
      ) : (
        <>
          <ul className="list">
            {issues.map((i) => {
              const key = i.fingerprint + i.targetId;
              return (
                <li key={key}>
                  <button className="list-row selectable" onClick={() => setOpen(open === key ? null : key)} aria-expanded={open === key}>
                    <div className="ellipsis grow">
                      <div className="ellipsis">
                        <span className={`pill ${i.level === 'ERROR' ? 'err' : 'warn'}`}>{i.level}</span> <span className="mono small">{i.sample}</span>
                      </div>
                      <div className="muted small">
                        {i.count}회 · 처음 {formatTime(i.firstSeen)} · 최근 {formatTime(i.lastSeen)} · {i.path?.split(/[\\/]/).pop() ?? '삭제된 파일'}
                      </div>
                    </div>
                  </button>
                  {open === key && (
                    <div className="diff">
                      <pre>{i.context}</pre>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
          <button className="btn" onClick={() => run(() => api.clearLogIssues(), (n) => `${n}건을 정리했습니다`)}>
            확인함 — 목록 비우기
          </button>
        </>
      )}
      <Notice message={message} />
    </>
  );
}
