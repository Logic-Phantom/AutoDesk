// Capture Studio 편집기: 화살표·사각형·형광펜·번호·텍스트·모자이크·자르기·크기 변경·OCR·복사·저장.
// 원본 캡처는 메모리에만 있고, 사용자가 저장/복사할 때만 밖으로 나간다.
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { save } from '@tauri-apps/plugin-dialog';
import {
  ArrowUpRight, Copy, Crop, Download, Grid3x3, Hash, Highlighter, Redo2, ScanText, Scissors, Square, Type, Undo2, X, Circle,
} from 'lucide-react';
import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import { errorMessage } from '../lib/api';
import { tools } from '../lib/toolsApi';

type Tool = 'arrow' | 'rect' | 'ellipse' | 'highlight' | 'number' | 'text' | 'mosaic' | 'crop';
interface Box { x: number; y: number; w: number; h: number }
type Shape =
  | { t: 'arrow'; x1: number; y1: number; x2: number; y2: number; color: string; size: number }
  | { t: 'rect' | 'ellipse' | 'highlight' | 'mosaic'; box: Box; color: string; size: number }
  | { t: 'number'; x: number; y: number; n: number; color: string; size: number }
  | { t: 'text'; x: number; y: number; text: string; color: string; size: number };
interface Doc { shapes: Shape[]; crop: Box | null }
interface History { doc: Doc; past: Doc[]; future: Doc[] }
type HistAction = { type: 'commit'; fn: (d: Doc) => Doc } | { type: 'undo' } | { type: 'redo' } | { type: 'reset' };

/** 그리기 기록(실행 취소/다시 실행 포함)을 한 곳에서 원자적으로 갱신한다 */
function historyReducer(h: History, a: HistAction): History {
  switch (a.type) {
    case 'commit': {
      const next = a.fn(h.doc);
      return next === h.doc ? h : { doc: next, past: [...h.past.slice(-50), h.doc], future: [] };
    }
    case 'undo':
      return h.past.length ? { doc: h.past[h.past.length - 1], past: h.past.slice(0, -1), future: [h.doc, ...h.future] } : h;
    case 'redo':
      return h.future.length ? { doc: h.future[0], past: [...h.past, h.doc], future: h.future.slice(1) } : h;
    case 'reset':
      return { doc: { shapes: [], crop: null }, past: [], future: [] };
  }
}

const COLORS = ['#e5484d', '#f59e0b', '#22c55e', '#2563eb', '#a855f7', '#111827', '#ffffff'];
const TOOLS: { id: Tool; label: string; icon: typeof Square; key: string }[] = [
  { id: 'arrow', label: '화살표', icon: ArrowUpRight, key: 'A' },
  { id: 'rect', label: '사각형', icon: Square, key: 'R' },
  { id: 'ellipse', label: '원', icon: Circle, key: 'E' },
  { id: 'highlight', label: '형광펜', icon: Highlighter, key: 'H' },
  { id: 'number', label: '번호', icon: Hash, key: 'N' },
  { id: 'text', label: '텍스트', icon: Type, key: 'T' },
  { id: 'mosaic', label: '모자이크(가리기)', icon: Grid3x3, key: 'M' },
  { id: 'crop', label: '자르기', icon: Crop, key: 'C' },
];

const normBox = (x1: number, y1: number, x2: number, y2: number): Box => ({ x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) });

function drawShape(ctx: CanvasRenderingContext2D, s: Shape, base: HTMLImageElement) {
  ctx.save();
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  switch (s.t) {
    case 'mosaic': {
      const { x, y, w, h } = s.box;
      if (w < 2 || h < 2) break;
      // 작게 줄였다 크게 늘리는 방식으로 픽셀화(원본 정보가 복원되지 않음)
      const cell = Math.max(8, Math.round(Math.min(w, h) / 12));
      const tw = Math.max(1, Math.round(w / cell));
      const th = Math.max(1, Math.round(h / cell));
      const tmp = document.createElement('canvas');
      tmp.width = tw;
      tmp.height = th;
      const tctx = tmp.getContext('2d')!;
      tctx.drawImage(base, x, y, w, h, 0, 0, tw, th);
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(tmp, 0, 0, tw, th, x, y, w, h);
      break;
    }
    case 'highlight':
      ctx.globalAlpha = 0.35;
      ctx.fillStyle = s.color === '#ffffff' ? '#facc15' : s.color;
      ctx.fillRect(s.box.x, s.box.y, s.box.w, s.box.h);
      break;
    case 'rect':
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.size;
      ctx.strokeRect(s.box.x, s.box.y, s.box.w, s.box.h);
      break;
    case 'ellipse':
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.size;
      ctx.beginPath();
      ctx.ellipse(s.box.x + s.box.w / 2, s.box.y + s.box.h / 2, s.box.w / 2, s.box.h / 2, 0, 0, Math.PI * 2);
      ctx.stroke();
      break;
    case 'arrow': {
      const ang = Math.atan2(s.y2 - s.y1, s.x2 - s.x1);
      const head = Math.max(12, s.size * 4);
      ctx.strokeStyle = s.color;
      ctx.fillStyle = s.color;
      ctx.lineWidth = s.size;
      ctx.beginPath();
      ctx.moveTo(s.x1, s.y1);
      ctx.lineTo(s.x2 - Math.cos(ang) * head * 0.6, s.y2 - Math.sin(ang) * head * 0.6);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(s.x2, s.y2);
      ctx.lineTo(s.x2 - head * Math.cos(ang - Math.PI / 7), s.y2 - head * Math.sin(ang - Math.PI / 7));
      ctx.lineTo(s.x2 - head * Math.cos(ang + Math.PI / 7), s.y2 - head * Math.sin(ang + Math.PI / 7));
      ctx.closePath();
      ctx.fill();
      break;
    }
    case 'number': {
      const r = Math.max(13, s.size * 4);
      ctx.fillStyle = s.color;
      ctx.beginPath();
      ctx.arc(s.x, s.y, r, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = s.color === '#ffffff' ? '#111827' : '#ffffff';
      ctx.font = `700 ${Math.round(r * 1.15)}px 'Segoe UI', 'Malgun Gothic', sans-serif`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(String(s.n), s.x, s.y + 1);
      break;
    }
    case 'text': {
      const fs = Math.max(14, s.size * 6);
      ctx.font = `600 ${fs}px 'Malgun Gothic', 'Segoe UI', sans-serif`;
      ctx.textBaseline = 'top';
      // 어떤 배경에서도 읽히도록 외곽선
      ctx.lineWidth = Math.max(3, fs / 6);
      ctx.strokeStyle = s.color === '#ffffff' ? '#111827' : '#ffffff';
      s.text.split('\n').forEach((line, i) => {
        ctx.strokeText(line, s.x, s.y + i * fs * 1.25);
        ctx.fillStyle = s.color;
        ctx.fillText(line, s.x, s.y + i * fs * 1.25);
      });
      break;
    }
  }
  ctx.restore();
}

export function Editor() {
  const [img, setImg] = useState<HTMLImageElement | null>(null);
  const [hist, dispatch] = useReducer(historyReducer, { doc: { shapes: [], crop: null }, past: [], future: [] });
  const { doc, past, future } = hist;
  const [tool, setTool] = useState<Tool>('arrow');
  const [color, setColor] = useState(COLORS[0]);
  const [size, setSize] = useState(4);
  const [draft, setDraftState] = useState<Shape | null>(null);
  const [cropDraft, setCropState] = useState<Box | null>(null);
  // 마우스를 놓는 순간에도 최신 값을 읽도록 ref에 함께 보관(빠른 드래그에서 그림이 빠지지 않게)
  const draftRef = useRef<Shape | null>(null);
  const cropRef = useRef<Box | null>(null);
  const setDraft = (s: Shape | null) => { draftRef.current = s; setDraftState(s); };
  const setCropDraft = (b: Box | null) => { cropRef.current = b; setCropState(b); };
  const [textInput, setTextInput] = useState<{ x: number; y: number; cx: number; cy: number; value: string } | null>(null);
  const [scale, setScale] = useState(100);
  const [ocr, setOcr] = useState<{ busy: boolean; text: string | null; error: string | null } | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const dragRef = useRef<{ x: number; y: number } | null>(null);

  const genRef = useRef(0);
  useEffect(() => {
    let url: string | null = null;
    const load = async () => {
      try {
        const gen = await tools.captureGen();
        const buf = await tools.captureGet();
        genRef.current = gen;
        if (url) URL.revokeObjectURL(url);
        url = URL.createObjectURL(new Blob([buf], { type: 'image/png' }));
        const im = new Image();
        im.onload = () => {
          setImg(im);
          dispatch({ type: 'reset' });
          setOcr(null);
          setScale(100);
        };
        im.src = url;
      } catch (e) {
        setToast(errorMessage(e));
      }
    };
    load();
    // 편집기가 열린 채로 새 캡처를 하면 같은 창에서 다시 불러온다
    const un1 = listen('capture://updated', () => { load(); });
    // 편집 창을 닫으면 '이 창이 보던' 캡처만 메모리에서 지운다
    const un2 = getCurrentWindow().onCloseRequested(() => { tools.captureClear(genRef.current); });
    return () => {
      if (url) URL.revokeObjectURL(url);
      un1.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  const commit = useCallback((fn: (d: Doc) => Doc) => dispatch({ type: 'commit', fn }), []);
  const undo = useCallback(() => dispatch({ type: 'undo' }), []);
  const redo = useCallback(() => dispatch({ type: 'redo' }), []);

  // 화면 그리기(원본 해상도 캔버스, CSS로 축소 표시)
  useEffect(() => {
    const c = canvasRef.current;
    if (!c || !img) return;
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const ctx = c.getContext('2d')!;
    ctx.drawImage(img, 0, 0);
    [...doc.shapes, ...(draft ? [draft] : [])].forEach((s) => drawShape(ctx, s, img));
    const crop = cropDraft ?? doc.crop;
    if (crop && crop.w > 2) {
      ctx.save();
      ctx.fillStyle = 'rgba(0,0,0,0.5)';
      ctx.beginPath();
      ctx.rect(0, 0, c.width, c.height);
      ctx.rect(crop.x, crop.y, crop.w, crop.h);
      ctx.fill('evenodd');
      ctx.strokeStyle = '#fff';
      ctx.setLineDash([8, 6]);
      ctx.lineWidth = 2;
      ctx.strokeRect(crop.x, crop.y, crop.w, crop.h);
      ctx.restore();
    }
  }, [img, doc, draft, cropDraft]);

  const toImg = (e: React.MouseEvent) => {
    const c = canvasRef.current!;
    const r = c.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * c.width, y: ((e.clientY - r.top) / r.height) * c.height, cx: e.clientX - r.left, cy: e.clientY - r.top };
  };

  function onDown(e: React.MouseEvent) {
    if (!img || e.button !== 0) return;
    const p = toImg(e);
    if (tool === 'text') {
      setTextInput({ x: p.x, y: p.y, cx: p.cx, cy: p.cy, value: '' });
      return;
    }
    if (tool === 'number') {
      commit((d) => ({ ...d, shapes: [...d.shapes, { t: 'number', x: p.x, y: p.y, n: d.shapes.filter((s) => s.t === 'number').length + 1, color, size }] }));
      return;
    }
    dragRef.current = { x: p.x, y: p.y };
  }

  function onMove(e: React.MouseEvent) {
    const s = dragRef.current;
    if (!s) return;
    const p = toImg(e);
    if (tool === 'crop') setCropDraft(normBox(s.x, s.y, p.x, p.y));
    else if (tool === 'arrow') setDraft({ t: 'arrow', x1: s.x, y1: s.y, x2: p.x, y2: p.y, color, size });
    else setDraft({ t: tool as 'rect', box: normBox(s.x, s.y, p.x, p.y), color, size });
  }

  function onUp(e?: React.MouseEvent) {
    if (!dragRef.current) return;
    if (e) onMove(e); // 놓은 위치까지 반영
    dragRef.current = null;
    const crop = cropRef.current;
    if (tool === 'crop') {
      if (crop && crop.w > 8 && crop.h > 8) commit((d) => ({ ...d, crop }));
      setCropDraft(null);
      return;
    }
    const shape = draftRef.current;
    if (shape) {
      const tiny = shape.t === 'arrow' ? Math.hypot(shape.x2 - shape.x1, shape.y2 - shape.y1) < 6 : 'box' in shape && (shape.box.w < 4 || shape.box.h < 4);
      if (!tiny) commit((d) => ({ ...d, shapes: [...d.shapes, shape] }));
      setDraft(null);
    }
  }

  function commitText() {
    if (textInput && textInput.value.trim()) {
      const t = textInput;
      commit((d) => ({ ...d, shapes: [...d.shapes, { t: 'text', x: t.x, y: t.y, text: t.value, color, size }] }));
    }
    setTextInput(null);
  }

  /** 자르기·크기 변경을 반영한 최종 이미지 */
  const exportData = useCallback((type: 'image/png' | 'image/jpeg' = 'image/png', scaleOverride?: number) => {
    if (!img) return '';
    const full = document.createElement('canvas');
    full.width = img.naturalWidth;
    full.height = img.naturalHeight;
    const fctx = full.getContext('2d')!;
    fctx.drawImage(img, 0, 0);
    doc.shapes.forEach((s) => drawShape(fctx, s, img));
    const crop = doc.crop ?? { x: 0, y: 0, w: img.naturalWidth, h: img.naturalHeight };
    const k = (scaleOverride ?? scale) / 100;
    const out = document.createElement('canvas');
    out.width = Math.max(1, Math.round(crop.w * k));
    out.height = Math.max(1, Math.round(crop.h * k));
    const octx = out.getContext('2d')!;
    if (type === 'image/jpeg') {
      octx.fillStyle = '#fff';
      octx.fillRect(0, 0, out.width, out.height);
    }
    octx.imageSmoothingQuality = 'high';
    octx.drawImage(full, crop.x, crop.y, crop.w, crop.h, 0, 0, out.width, out.height);
    return out.toDataURL(type, 0.92);
  }, [img, doc, scale]);

  const flash = (m: string) => {
    setToast(m);
    window.setTimeout(() => setToast(null), 2200);
  };

  async function copy() {
    try {
      await tools.imageCopy(exportData());
      flash('클립보드에 복사했습니다 — 메신저·메일에 바로 붙여넣으세요');
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function saveAs() {
    const stamp = new Date().toISOString().slice(0, 19).replace(/[-:T]/g, '').replace(/(\d{8})(\d{6})/, '$1_$2');
    const path = await save({ title: '이미지 저장', defaultPath: `캡처_${stamp}.png`, filters: [{ name: 'PNG', extensions: ['png'] }, { name: 'JPG', extensions: ['jpg', 'jpeg'] }] });
    if (!path) return;
    try {
      await tools.imageSave(path, exportData(/\.jpe?g$/i.test(path) ? 'image/jpeg' : 'image/png'));
      flash(`저장했습니다: ${path}`);
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function runOcr() {
    setOcr({ busy: true, text: null, error: null });
    try {
      // OCR은 원본 해상도에서 정확도가 높다 → 저장 크기 설정은 무시하고 자른 영역만 보낸다
      const text = await tools.ocr(exportData('image/png', 100));
      setOcr({ busy: false, text: text || '(인식된 글자가 없습니다)', error: null });
    } catch (e) {
      setOcr({ busy: false, text: null, error: errorMessage(e) });
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (textInput) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') { e.preventDefault(); if (e.shiftKey) redo(); else undo(); return; }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'y') { e.preventDefault(); redo(); return; }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'c') { e.preventDefault(); copy(); return; }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); saveAs(); return; }
      const t = TOOLS.find((x) => x.key === e.key.toUpperCase());
      if (t && !e.ctrlKey && !e.metaKey && !e.altKey) setTool(t.id);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  const outW = img ? Math.round((doc.crop?.w ?? img.naturalWidth) * scale / 100) : 0;
  const outH = img ? Math.round((doc.crop?.h ?? img.naturalHeight) * scale / 100) : 0;

  return (
    <div className="editor">
      <header className="editor-bar">
        <div className="tool-group" role="toolbar" aria-label="그리기 도구">
          {TOOLS.map(({ id, label, icon: Icon, key }) => (
            <button key={id} className={`tool ${tool === id ? 'on' : ''}`} title={`${label} (${key})`} aria-label={label} aria-pressed={tool === id} onClick={() => setTool(id)}>
              <Icon size={18} />
            </button>
          ))}
        </div>
        <div className="tool-group" aria-label="색">
          {COLORS.map((c) => (
            <button key={c} className={`swatch ${color === c ? 'on' : ''}`} style={{ background: c }} aria-label={`색 ${c}`} onClick={() => setColor(c)} />
          ))}
        </div>
        <label className="tool-group size" title="굵기">
          <input type="range" min={2} max={12} value={size} onChange={(e) => setSize(Number(e.target.value))} aria-label="굵기" />
        </label>
        <div className="tool-group">
          <button className="tool" title="실행 취소 (Ctrl+Z)" aria-label="실행 취소" disabled={!past.length} onClick={undo}><Undo2 size={18} /></button>
          <button className="tool" title="다시 실행 (Ctrl+Y)" aria-label="다시 실행" disabled={!future.length} onClick={redo}><Redo2 size={18} /></button>
          {doc.crop && <button className="tool" title="자르기 해제" aria-label="자르기 해제" onClick={() => commit((d) => ({ ...d, crop: null }))}><Scissors size={18} /></button>}
        </div>
        <span className="spacer" />
        <label className="tool-group scale" title="저장 크기">
          <select value={scale} onChange={(e) => setScale(Number(e.target.value))} aria-label="저장 크기">
            {[25, 50, 75, 100, 150, 200].map((p) => <option key={p} value={p}>{p}%</option>)}
          </select>
          <span className="muted small">{outW}×{outH}</span>
        </label>
        <button className="btn" onClick={runOcr} title="이미지 속 글자 추출(Windows 내장 OCR, 로컬 처리)"><ScanText size={16} /> 글자 추출</button>
        <button className="btn" onClick={saveAs}><Download size={16} /> 저장</button>
        <button className="btn primary" onClick={copy}><Copy size={16} /> 복사</button>
      </header>

      <div className="editor-body">
        <div className="canvas-wrap">
          {!img && <p className="muted">{toast ?? '캡처를 불러오는 중…'}</p>}
          <div className="canvas-stage">
            <canvas
              ref={canvasRef}
              className={`tool-${tool}`}
              onMouseDown={onDown}
              onMouseMove={onMove}
              onMouseUp={(e) => onUp(e)}
              onMouseLeave={() => onUp()}
            />
            {textInput && (
              <textarea
                autoFocus
                className="text-input"
                style={{ left: textInput.cx, top: textInput.cy, color }}
                value={textInput.value}
                placeholder="텍스트 입력 후 Enter (줄바꿈: Shift+Enter)"
                onChange={(e) => setTextInput({ ...textInput, value: e.target.value })}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); commitText(); }
                  if (e.key === 'Escape') setTextInput(null);
                }}
                onBlur={commitText}
              />
            )}
          </div>
        </div>
        {ocr && (
          <aside className="ocr-panel">
            <header>
              <strong>추출한 글자</strong>
              <button className="icon-btn" aria-label="닫기" onClick={() => setOcr(null)}><X size={16} /></button>
            </header>
            {ocr.busy && <p className="muted">인식 중…</p>}
            {ocr.error && <p className="notice err">{ocr.error}</p>}
            {ocr.text !== null && (
              <>
                <textarea className="input" value={ocr.text} onChange={(e) => setOcr({ ...ocr, text: e.target.value })} spellCheck={false} />
                <button className="btn primary" onClick={() => navigator.clipboard.writeText(ocr.text ?? '').then(() => flash('텍스트를 복사했습니다'))}><Copy size={14} /> 텍스트 복사</button>
                <p className="muted small">Windows 내장 OCR로 PC 안에서 인식했습니다. 자르기로 영역을 좁히면 더 정확합니다.</p>
              </>
            )}
          </aside>
        )}
      </div>
      <footer className="editor-foot muted small">
        도구 단축키: A 화살표 · R 사각형 · E 원 · H 형광펜 · N 번호 · T 텍스트 · M 모자이크 · C 자르기 · Ctrl+Z/Y · Ctrl+C 복사 · Ctrl+S 저장 — 이 창을 닫으면 캡처는 메모리에서 지워집니다(저장한 파일만 남음).
      </footer>
      {toast && img && <div className="toast" role="status">{toast}</div>}
    </div>
  );
}
