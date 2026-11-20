// 영역 캡처 좌표 계산.
// 선택 창(overlay)은 모니터 전체를 덮고, 그 안의 마우스 좌표는 CSS 픽셀이다.
// 캡처 이미지는 물리 픽셀(배율 125%면 CSS의 1.25배)이므로 비율로 바꿔야 한다.
// 모니터 위치(음수 좌표 등)는 선택 창 자체가 그 모니터에 맞춰 놓이므로 여기서는 신경 쓰지 않는다.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Size {
  w: number;
  h: number;
}

/** 두 점으로 만든 사각형(어느 방향으로 드래그해도 양수 폭·높이) */
export function rectFromPoints(a: { x: number; y: number }, b: { x: number; y: number }): Rect {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y) };
}

/**
 * CSS 픽셀 사각형 → 이미지 픽셀 사각형. 이미지 경계를 넘지 않게 자른다.
 * 너무 작으면(클릭만 한 경우) null → 화면 전체로 처리.
 */
export function toImageRect(r: Rect, image: Size, viewport: Size, minCss = 4): Rect | null {
  if (r.w < minCss || r.h < minCss) return null;
  const sx = image.w / viewport.w;
  const sy = image.h / viewport.h;
  const x = Math.max(0, Math.min(image.w - 1, Math.round(r.x * sx)));
  const y = Math.max(0, Math.min(image.h - 1, Math.round(r.y * sy)));
  const w = Math.max(1, Math.min(image.w - x, Math.round(r.w * sx)));
  const h = Math.max(1, Math.min(image.h - y, Math.round(r.h * sy)));
  return { x, y, w, h };
}

/** 선택 중 표시할 실제 픽셀 크기 */
export function imageSizeOf(r: Rect, image: Size, viewport: Size): Size {
  if (!image.w || !viewport.w) return { w: Math.round(r.w), h: Math.round(r.h) };
  return { w: Math.round((r.w * image.w) / viewport.w), h: Math.round((r.h * image.h) / viewport.h) };
}
