// 영역 캡처: 방금 찍은 화면을 전체 화면으로 깔고, 드래그한 영역만 잘라 편집기로 넘긴다.
import { useEffect, useRef, useState } from 'react';
import { imageSizeOf, rectFromPoints as norm, toImageRect, type Rect } from '../lib/geometry';
import { tools } from '../lib/toolsApi';

export function Overlay() {
  const [src, setSrc] = useState<string | null>(null);
  const [start, setStart] = useState<{ x: number; y: number } | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const [natural, setNatural] = useState({ w: 0, h: 0 });
  const imgRef = useRef<HTMLImageElement>(null);
  // 다음 렌더 전에 들어오는 빠른 이벤트에서도 시작점을 읽을 수 있게 ref에 보관
  const startRef = useRef<{ x: number; y: number } | null>(null);
  const doneRef = useRef(false);

  useEffect(() => {
    let url: string | null = null;
    tools.captureGet().then((buf) => {
      url = URL.createObjectURL(new Blob([buf], { type: 'image/png' }));
      setSrc(url);
    }).catch(() => tools.captureCancel());
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') tools.captureCancel();
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      if (url) URL.revokeObjectURL(url);
    };
  }, []);

  const viewport = () => ({ w: window.innerWidth, h: window.innerHeight });

  function finish(r: Rect) {
    const img = imgRef.current;
    if (!img) return;
    // 화면(CSS px) → 이미지(물리 px). 고배율(125%·150%·200%) 모니터 대응 — lib/geometry 테스트 참고
    const image = { w: img.naturalWidth, h: img.naturalHeight };
    const ir = toImageRect(r, image, viewport());
    if (!ir) {
      tools.captureCrop(0, 0, image.w, image.h); // 클릭만 하면 화면 전체
      return;
    }
    tools.captureCrop(ir.x, ir.y, ir.w, ir.h);
  }

  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.button !== 0) return;
        startRef.current = { x: e.clientX, y: e.clientY };
        setStart({ x: e.clientX, y: e.clientY });
        setRect({ x: e.clientX, y: e.clientY, w: 0, h: 0 });
      }}
      onMouseMove={(e) => {
        setPos({ x: e.clientX, y: e.clientY });
        if (startRef.current) setRect(norm(startRef.current, { x: e.clientX, y: e.clientY }));
      }}
      onMouseUp={(e) => {
        const s0 = startRef.current;
        if (!s0 || doneRef.current) return;
        const r = norm(s0, { x: e.clientX, y: e.clientY });
        startRef.current = null;
        doneRef.current = true; // 한 번만 확정
        setStart(null);
        finish(r);
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        tools.captureCancel();
      }}
    >
      {src && <img ref={imgRef} src={src} alt="" draggable={false} className="overlay-img" onLoad={(e) => setNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })} />}
      <div className="overlay-dim" style={rect && rect.w > 0 ? { clipPath: `polygon(0 0, 100% 0, 100% 100%, 0 100%, 0 ${rect.y}px, ${rect.x}px ${rect.y}px, ${rect.x}px ${rect.y + rect.h}px, ${rect.x + rect.w}px ${rect.y + rect.h}px, ${rect.x + rect.w}px ${rect.y}px, 0 ${rect.y}px)` } : undefined} />
      {rect && rect.w > 0 && (
        <div className="overlay-sel" style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}>
          <span className="overlay-size">{(() => { const z = imageSizeOf(rect, natural, viewport()); return `${z.w} × ${z.h}`; })()}</span>
        </div>
      )}
      {!start && (
        <>
          <div className="overlay-cross-h" style={{ top: pos.y }} />
          <div className="overlay-cross-v" style={{ left: pos.x }} />
          <div className="overlay-hint">드래그해서 영역 선택 · 클릭만 하면 화면 전체 · Esc/우클릭 취소</div>
        </>
      )}
    </div>
  );
}
