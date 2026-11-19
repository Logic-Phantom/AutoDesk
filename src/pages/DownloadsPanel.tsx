import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { FolderPlus, ScanSearch, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { api, type ModuleView, type Overview, type WatchTarget } from '../lib/api';
import { useAction } from '../lib/useCore';
import { Notice, Row, Toggle } from '../components/ui';

interface DownloadsConfig {
  mode: 'preview' | 'auto';
  watchDefaultDir: boolean;
  excludeExtensions: string[];
}

export function DownloadsPanel({ m, data }: { m: ModuleView; data: Overview }) {
  const cfg = m.config as unknown as DownloadsConfig;
  const [targets, setTargets] = useState<WatchTarget[]>([]);
  const [excludes, setExcludes] = useState(cfg.excludeExtensions.join(', '));
  const { message, run, busy } = useAction();
  const load = useCallback(async () => setTargets(await api.listWatchTargets('downloads')), []);
  useEffect(() => {
    load();
  }, [load]);

  const save = (patch: Partial<DownloadsConfig>, ok = '저장했습니다') =>
    run(() => api.setModuleConfig('downloads', { ...cfg, ...patch }), ok);

  async function addFolder() {
    const picked = await openDialog({ directory: true, multiple: false, title: '분류할 다운로드 폴더 선택' });
    if (typeof picked !== 'string') return;
    await run(() => api.addWatchTarget('downloads', picked, false), '폴더를 추가했습니다');
    load();
  }

  return (
    <>
      <Row
        label="자동 이동(AUTO)"
        hint={
          cfg.mode === 'auto'
            ? '확실히 분류되는 파일은 바로 이동합니다. 불확실한 파일은 승인 대기로 남습니다.'
            : '미리보기 모드: 이동 제안만 만들고, 승인 센터에서 승인해야 이동합니다.'
        }
      >
        <Toggle
          label="자동 이동"
          checked={cfg.mode === 'auto'}
          onChange={(v) => {
            if (v && !window.confirm('새 다운로드를 자동으로 하위 폴더(Documents/PDF, Images 등)로 옮깁니다. 삭제·덮어쓰기는 하지 않고 언제든 되돌릴 수 있습니다. 켤까요?')) return;
            save({ mode: v ? 'auto' : 'preview' });
          }}
        />
      </Row>
      <Row label="기본 다운로드 폴더 감시" hint={<span className="mono">{data.downloadsDir ?? '찾을 수 없음'}</span>}>
        <Toggle label="기본 다운로드 폴더 감시" checked={cfg.watchDefaultDir} onChange={(v) => save({ watchDefaultDir: v })} />
      </Row>
      <h3>추가 폴더</h3>
      <ul className="list">
        {targets.map((t) => (
          <li key={t.id} className="list-row">
            <span className="mono ellipsis">{t.path}</span>
            <button className="icon-btn" aria-label="폴더 제거" onClick={() => run(() => api.removeWatchTarget(t.id)).then(load)}>
              <Trash2 size={16} />
            </button>
          </li>
        ))}
      </ul>
      <div className="inline">
        <button className="btn" onClick={addFolder} disabled={busy}>
          <FolderPlus size={16} aria-hidden /> 폴더 추가
        </button>
        <button
          className="btn"
          disabled={busy || m.status !== 'RUNNING'}
          onClick={() => run(() => api.scanDownloads(), (n) => `기존 파일 ${n}개에 대한 이동 제안을 승인 센터에 올렸습니다`)}
        >
          <ScanSearch size={16} aria-hidden /> 지금 있는 파일 정리 제안 받기
        </button>
      </div>
      <label className="field">
        <span>제외할 확장자(쉼표 구분)</span>
        <input className="input" value={excludes} onChange={(e) => setExcludes(e.target.value)} placeholder="예: torrent, iso" />
      </label>
      <button
        className="btn primary"
        onClick={() => save({ excludeExtensions: excludes.split(',').map((x) => x.trim()).filter(Boolean) })}
      >
        저장
      </button>
      <p className="muted small">
        분류 규칙: PDF → Documents/PDF · 오피스/한글 → Documents/Office · 텍스트 → Documents/Text · 이미지/영상/음악/압축 → 각 폴더 ·
        설치 파일 → Installers. 파일은 실행하지 않으며, 확장자와 내용이 다르면(예: .pdf인데 실행 파일) 승인을 요청합니다. 이동 내역은
        &lsquo;이력&rsquo;에서 되돌릴 수 있습니다.
      </p>
      <Notice message={message} />
    </>
  );
}
