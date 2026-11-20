import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { FolderOpen, ImagePlus, Play, Trash2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { formatBytes } from '../../lib/format';
import { fileName, tools, type ImageJob, type ImageResult } from '../../lib/toolsApi';
import { useAction } from '../../lib/useCore';
import { Card, Empty, Notice, Row } from '../../components/ui';

const IMG_EXT = ['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp'];
const isImage = (p: string) => IMG_EXT.includes(p.split('.').pop()?.toLowerCase() ?? '');

const PRESETS: { label: string; patch: Partial<ImageJob> }[] = [
  { label: '웹 업로드(1920px, JPG 80)', patch: { format: 'jpg', resize: 'fit', width: 1920, height: 1920, quality: 80, maxKb: 0 } },
  { label: '제출용 500KB 이하', patch: { format: 'jpg', resize: 'fit', width: 2560, height: 2560, quality: 90, maxKb: 500 } },
  { label: '썸네일 400×400', patch: { format: 'jpg', resize: 'cover', width: 400, height: 400, quality: 85, maxKb: 0 } },
  { label: 'PNG로 변환', patch: { format: 'png', resize: 'none', maxKb: 0 } },
];

export function ImageStudio() {
  const [files, setFiles] = useState<string[]>([]);
  const [job, setJob] = useState<Omit<ImageJob, 'files'>>({
    format: 'jpg', resize: 'fit', width: 1920, height: 1920, percent: 50, quality: 85, maxKb: 0, outputDir: '', suffix: '',
  });
  const [results, setResults] = useState<ImageResult[]>([]);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const { message, run, busy } = useAction();

  const add = (paths: string[]) => setFiles((cur) => Array.from(new Set([...cur, ...paths.filter(isImage)])));

  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === 'over' || e.payload.type === 'enter') setDragging(true);
      else if (e.payload.type === 'drop') {
        setDragging(false);
        add(e.payload.paths);
      } else setDragging(false);
    });
    const un2 = listen<{ done: number; total: number; result: ImageResult }>('tools://image-progress', (e) => {
      setProgress({ done: e.payload.done, total: e.payload.total });
      setResults((r) => [...r, e.payload.result]);
    });
    return () => {
      un.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  async function pick() {
    const p = await open({ multiple: true, directory: false, title: '이미지 선택', filters: [{ name: '이미지', extensions: IMG_EXT }] });
    if (Array.isArray(p)) add(p);
    else if (typeof p === 'string') add([p]);
  }

  async function pickOut() {
    const p = await open({ directory: true, multiple: false, title: '저장할 폴더' });
    if (typeof p === 'string') setJob({ ...job, outputDir: p });
  }

  async function start() {
    setResults([]);
    setProgress({ done: 0, total: files.length });
    const r = await run(() => tools.imageBatch({ ...job, files }), (rs) => {
      const ok = rs.filter((x) => !x.error).length;
      return `${ok}/${rs.length}개 변환 완료 — 원본은 그대로 두고 새 파일로 저장했습니다`;
    });
    if (r) setResults(r);
    setProgress(null);
  }

  const before = results.reduce((n, r) => n + (r.error ? 0 : r.beforeBytes), 0);
  const after = results.reduce((n, r) => n + (r.error ? 0 : r.afterBytes), 0);
  const outDir = results.find((r) => r.output)?.output?.replace(/[\\/][^\\/]+$/, '');

  return (
    <div className="page wide">
      <header className="page-head">
        <div>
          <h1>이미지 변환</h1>
          <p className="muted">여러 이미지를 한 번에 형식 변환·크기 조절·용량 압축합니다. 원본은 건드리지 않고 별도 폴더에 저장합니다. 휴대폰 사진의 회전 정보도 반영합니다.</p>
        </div>
      </header>
      <div className="split">
        <Card
          title={`이미지 ${files.length}개`}
          actions={
            <div className="inline tight">
              <button className="btn small" onClick={pick}><ImagePlus size={14} aria-hidden /> 추가</button>
              {files.length > 0 && <button className="icon-btn" aria-label="모두 비우기" title="모두 비우기" onClick={() => { setFiles([]); setResults([]); }}><Trash2 size={16} /></button>}
            </div>
          }
        >
          <div className={`dropzone ${dragging ? 'over' : ''}`} onClick={() => files.length === 0 && pick()}>
            {files.length === 0 ? (
              <p>여기로 이미지를 끌어다 놓거나 눌러서 선택하세요<br /><span className="muted small">JPG · PNG · WEBP · GIF · BMP, 한 번에 500개까지</span></p>
            ) : (
              <ul className="list file-list">
                {files.map((f) => (
                  <li key={f} className="list-row">
                    <span className="ellipsis">{fileName(f)}</span>
                    <button className="icon-btn" aria-label="목록에서 빼기" onClick={(e) => { e.stopPropagation(); setFiles(files.filter((x) => x !== f)); }}><X size={14} /></button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </Card>
        <Card title="변환 설정">
          <div className="inline">
            {PRESETS.map((p) => (
              <button key={p.label} className="btn small" onClick={() => setJob({ ...job, ...p.patch })}>{p.label}</button>
            ))}
          </div>
          <Row label="저장 형식">
            <select className="input" value={job.format} onChange={(e) => setJob({ ...job, format: e.target.value as ImageJob['format'] })}>
              <option value="keep">원래 형식 유지</option>
              <option value="jpg">JPG</option>
              <option value="png">PNG</option>
              <option value="webp">WEBP (무손실)</option>
            </select>
          </Row>
          <Row label="크기 조절" hint={{ none: '원본 크기 그대로', fit: '비율 유지, 작은 이미지는 키우지 않음', cover: '비율 유지, 넘치는 부분은 가운데 기준으로 자름', exact: '비율 무시하고 정확히 맞춤', percent: '가로·세로를 같은 비율로' }[job.resize]}>
            <select className="input" value={job.resize} onChange={(e) => setJob({ ...job, resize: e.target.value as ImageJob['resize'] })}>
              <option value="none">그대로</option>
              <option value="fit">최대 크기에 맞추기</option>
              <option value="cover">정확한 크기로 자르기</option>
              <option value="exact">정확한 크기로 늘리기</option>
              <option value="percent">비율(%)</option>
            </select>
          </Row>
          {job.resize === 'percent' ? (
            <Row label="비율(%)"><input className="input num" type="number" min={1} max={400} value={job.percent} onChange={(e) => setJob({ ...job, percent: Number(e.target.value) })} /></Row>
          ) : job.resize !== 'none' ? (
            <Row label="가로 × 세로(px)">
              <span className="inline tight">
                <input className="input num" type="number" min={1} value={job.width} onChange={(e) => setJob({ ...job, width: Number(e.target.value) })} />
                ×
                <input className="input num" type="number" min={1} value={job.height} onChange={(e) => setJob({ ...job, height: Number(e.target.value) })} />
              </span>
            </Row>
          ) : null}
          {(job.format === 'jpg' || job.format === 'keep') && (
            <>
              <Row label={`JPG 품질 ${job.quality}`}>
                <input type="range" min={30} max={100} value={job.quality} onChange={(e) => setJob({ ...job, quality: Number(e.target.value) })} />
              </Row>
              <Row label="파일 용량 제한(KB)" hint="0이면 제한 없음. 넘으면 품질을 자동으로 낮춰 맞춥니다(JPG)">
                <input className="input num" type="number" min={0} value={job.maxKb} onChange={(e) => setJob({ ...job, maxKb: Number(e.target.value) })} />
              </Row>
            </>
          )}
          <Row label="저장 위치" hint={job.outputDir || '첫 번째 이미지 폴더 안의 "AutoDesk 변환" 폴더'}>
            <span className="inline tight">
              <button className="btn small" onClick={pickOut}><FolderOpen size={14} aria-hidden /> 선택</button>
              {job.outputDir && <button className="btn small" onClick={() => setJob({ ...job, outputDir: '' })}>기본값</button>}
            </span>
          </Row>
          <Row label="파일 이름 뒤에 붙일 글자" hint="예: _web → 사진_web.jpg (같은 이름이 있으면 (2)를 붙임)">
            <input className="input" style={{ width: 120 }} value={job.suffix} onChange={(e) => setJob({ ...job, suffix: e.target.value.replace(/[\\/:*?"<>|]/g, '') })} />
          </Row>
          <button className="btn primary" disabled={busy || files.length === 0} onClick={start}>
            <Play size={16} aria-hidden /> {busy && progress ? `변환 중 ${progress.done}/${progress.total}` : `${files.length}개 변환`}
          </button>
          {progress && <progress className="bar" value={progress.done} max={progress.total} />}
          <Notice message={message} />
        </Card>
      </div>

      {results.length > 0 && (
        <Card
          title="결과"
          actions={
            <div className="inline tight">
              {before > 0 && <span className={`chip ${after <= before ? 'add' : 'mod'}`}>{formatBytes(before)} → {formatBytes(after)} ({Math.abs(Math.round((1 - after / before) * 100))}% {after <= before ? '감소' : '증가'})</span>}
              {outDir && <button className="btn small" onClick={() => tools.openPath(outDir)}><FolderOpen size={14} aria-hidden /> 폴더 열기</button>}
            </div>
          }
        >
          <table className="table">
            <thead><tr><th>파일</th><th>변환 전</th><th>변환 후</th><th>크기(px)</th><th>메모</th></tr></thead>
            <tbody>
              {results.map((r) => (
                <tr key={r.source + (r.output ?? '')}>
                  <td className="ellipsis">{fileName(r.output ?? r.source)}</td>
                  <td className="nowrap">{formatBytes(r.beforeBytes)}</td>
                  <td className="nowrap">{r.error ? '—' : formatBytes(r.afterBytes)}</td>
                  <td className="nowrap">{r.error ? '—' : `${r.width}×${r.height}`}</td>
                  <td className={r.error ? 'err-text small' : 'muted small'}>{r.error ?? r.note ?? ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>
      )}
      {results.length === 0 && files.length > 0 && !busy && <Empty>설정을 고르고 변환을 누르세요.</Empty>}
    </div>
  );
}
