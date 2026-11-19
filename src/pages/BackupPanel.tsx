import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { FolderPlus, RotateCcw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { api, type BackedFile, type DiffResult, type ModuleView, type VersionRow, type WatchTarget } from '../lib/api';
import { formatBytes, formatTime } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Empty, Notice, Row } from '../components/ui';

interface BackupConfig {
  maxVersions: number;
  maxFileSizeMb: number;
  maxTotalMb: number;
  extensions: string[];
  excludes: string[];
}

export function BackupPanel({ m }: { m: ModuleView }) {
  const cfg = m.config as unknown as BackupConfig;
  const [targets, setTargets] = useState<WatchTarget[]>([]);
  const [recursive, setRecursive] = useState(true);
  const [draft, setDraft] = useState({ ...cfg, extensions: cfg.extensions.join(', '), excludes: cfg.excludes.join(', ') });
  const { message, run, busy } = useAction();

  const load = useCallback(async () => setTargets(await api.listWatchTargets('backup')), []);
  useEffect(() => {
    load();
  }, [load]);

  async function addFolder() {
    const picked = await openDialog({ directory: true, multiple: false, title: '백업할 폴더 선택' });
    if (typeof picked !== 'string') return;
    await run(() => api.addWatchTarget('backup', picked, recursive), '폴더를 추가했습니다. 이제 이 폴더의 파일이 바뀌면 버전이 저장됩니다.');
    load();
  }

  async function save() {
    const split = (s: string) => s.split(',').map((x) => x.trim()).filter(Boolean);
    await run(
      () =>
        api.setModuleConfig('backup', {
          maxVersions: Number(draft.maxVersions),
          maxFileSizeMb: Number(draft.maxFileSizeMb),
          maxTotalMb: Number(draft.maxTotalMb),
          extensions: split(draft.extensions),
          excludes: split(draft.excludes),
        }),
      '저장했습니다',
    );
  }

  return (
    <>
      <h3>감시 폴더</h3>
      {targets.length === 0 ? (
        <Empty>아직 등록한 폴더가 없습니다.</Empty>
      ) : (
        <ul className="list">
          {targets.map((t) => (
            <li key={t.id} className="list-row">
              <div className="ellipsis">
                <div className="mono ellipsis">{t.path}</div>
                <div className="muted small">{t.recursive ? '하위 폴더 포함' : '이 폴더만'}</div>
              </div>
              <button className="icon-btn" aria-label="폴더 제거" onClick={() => run(() => api.removeWatchTarget(t.id)).then(load)}>
                <Trash2 size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="inline">
        <button className="btn" onClick={addFolder} disabled={busy}>
          <FolderPlus size={16} aria-hidden /> 폴더 추가
        </button>
        <label className="check">
          <input type="checkbox" checked={recursive} onChange={(e) => setRecursive(e.target.checked)} /> 하위 폴더 포함
        </label>
      </div>

      <h3>보관 정책</h3>
      <Row label="파일당 보관 버전 수">
        <input className="input num" type="number" min={1} max={200} value={draft.maxVersions}
          onChange={(e) => setDraft({ ...draft, maxVersions: Number(e.target.value) })} />
      </Row>
      <Row label="백업할 최대 파일 크기(MB)" hint="이보다 큰 파일은 건너뜁니다">
        <input className="input num" type="number" min={1} value={draft.maxFileSizeMb}
          onChange={(e) => setDraft({ ...draft, maxFileSizeMb: Number(e.target.value) })} />
      </Row>
      <Row label="전체 백업 용량 제한(MB)" hint="넘으면 오래된 버전부터 정리(파일별 최신본은 유지)">
        <input className="input num" type="number" min={50} value={draft.maxTotalMb}
          onChange={(e) => setDraft({ ...draft, maxTotalMb: Number(e.target.value) })} />
      </Row>
      <label className="field">
        <span>백업할 확장자(쉼표 구분, 비우면 모든 파일)</span>
        <textarea className="input" rows={2} value={draft.extensions} onChange={(e) => setDraft({ ...draft, extensions: e.target.value })} />
      </label>
      <label className="field">
        <span>제외 패턴(* 와일드카드, 폴더는 name/**)</span>
        <textarea className="input" rows={2} value={draft.excludes} onChange={(e) => setDraft({ ...draft, excludes: e.target.value })} />
      </label>
      <button className="btn primary" onClick={save} disabled={busy}>설정 저장</button>
      <Notice message={message} />

      <h3>버전 기록 · 복원</h3>
      <VersionBrowser />
    </>
  );
}

function VersionBrowser() {
  const [query, setQuery] = useState('');
  const [files, setFiles] = useState<BackedFile[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [versions, setVersions] = useState<VersionRow[]>([]);
  const [diff, setDiff] = useState<{ id: string; result: DiffResult } | null>(null);
  const { message, run } = useAction();

  useEffect(() => {
    const t = window.setTimeout(() => api.listBackedFiles(query || undefined).then(setFiles).catch(() => setFiles([])), 200);
    return () => window.clearTimeout(t);
  }, [query, message]);

  useEffect(() => {
    if (selected) api.listVersions(selected).then(setVersions);
    setDiff(null);
  }, [selected, message]);

  async function restore(v: VersionRow, mode: 'copy' | 'overwrite') {
    if (mode === 'overwrite' && !window.confirm('현재 파일을 이 버전으로 바꿉니다. 현재 상태는 먼저 백업됩니다. 계속할까요?')) return;
    await run(() => api.restoreVersion(v.id, mode), (p) => `복원했습니다: ${p}`);
  }

  return (
    <div className="versions">
      <input className="input" placeholder="파일 이름으로 검색" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="백업 파일 검색" />
      {files.length === 0 ? (
        <Empty>백업된 파일이 없습니다.</Empty>
      ) : (
        <ul className="list scroll">
          {files.map((f) => (
            <li key={f.originalPath}>
              <button className={`list-row selectable ${selected === f.originalPath ? 'selected' : ''}`} onClick={() => setSelected(f.originalPath)}>
                <div className="ellipsis">
                  <div className="ellipsis">{f.relativePath}</div>
                  <div className="muted small">
                    버전 {f.versions}개 · {formatTime(f.lastBackupAt)} {f.exists ? '' : '· 원본 없음'}
                  </div>
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
      {selected && (
        <ul className="list">
          {versions.map((v) => (
            <li key={v.id} className="list-row">
              <div>
                <div>{formatTime(v.createdAt)}</div>
                <div className="muted small mono">{formatBytes(v.fileSize)} · {v.sha256.slice(0, 10)}</div>
              </div>
              <div className="inline">
                <button className="btn" onClick={async () => setDiff({ id: v.id, result: await api.diffVersion(v.id) })}>비교</button>
                <button className="btn" onClick={() => restore(v, 'copy')}>사본으로 복원</button>
                <button className="btn danger" onClick={() => restore(v, 'overwrite')}>
                  <RotateCcw size={14} aria-hidden /> 덮어쓰기 복원
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {diff && (
        <div className="diff">
          {diff.result.kind === 'Text' ? (
            diff.result.identical ? (
              <Empty>현재 파일과 내용이 같습니다.</Empty>
            ) : (
              <pre>{diff.result.diff.split('\n').map((l, i) => (
                <span key={i} className={l.startsWith('+') ? 'add' : l.startsWith('-') ? 'del' : ''}>{l + '\n'}</span>
              ))}</pre>
            )
          ) : (
            <p className="muted">
              바이너리 파일 비교 — 백업: {formatBytes(diff.result.snapshotSize)} / 현재:{' '}
              {diff.result.currentSize == null ? '없음' : formatBytes(diff.result.currentSize)}{' '}
              {diff.result.currentHash === diff.result.snapshotHash ? '(같은 내용)' : '(다른 내용)'}
            </p>
          )}
        </div>
      )}
      <Notice message={message} />
    </div>
  );
}
