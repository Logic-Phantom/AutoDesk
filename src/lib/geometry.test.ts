import { imageSizeOf, rectFromPoints, toImageRect } from './geometry';

describe('capture geometry (DPI scaling)', () => {
  const sel = rectFromPoints({ x: 500, y: 420 }, { x: 100, y: 120 }); // 오른쪽 아래 → 왼쪽 위로 드래그

  it('normalizes drag direction', () => {
    expect(sel).toEqual({ x: 100, y: 120, w: 400, h: 300 });
  });

  it('100% scale maps 1:1', () => {
    expect(toImageRect(sel, { w: 1920, h: 1080 }, { w: 1920, h: 1080 })).toEqual({ x: 100, y: 120, w: 400, h: 300 });
  });

  it('125% / 150% / 200% scale maps to physical pixels', () => {
    // 2400x1350 물리 모니터를 125%로 쓰면 CSS 뷰포트는 1920x1080
    expect(toImageRect(sel, { w: 2400, h: 1350 }, { w: 1920, h: 1080 })).toEqual({ x: 125, y: 150, w: 500, h: 375 });
    // 2880x1620 @150% → CSS 1920x1080
    expect(toImageRect(sel, { w: 2880, h: 1620 }, { w: 1920, h: 1080 })).toEqual({ x: 150, y: 180, w: 600, h: 450 });
    // 4K @200% → CSS 1920x1080
    expect(toImageRect(sel, { w: 3840, h: 2160 }, { w: 1920, h: 1080 })).toEqual({ x: 200, y: 240, w: 800, h: 600 });
  });

  it('non-integer scale (175%) stays inside the image', () => {
    const r = toImageRect({ x: 1000, y: 600, w: 1000, h: 1000 }, { w: 2560, h: 1440 }, { w: 1463, h: 823 })!;
    expect(r.x + r.w).toBeLessThanOrEqual(2560);
    expect(r.y + r.h).toBeLessThanOrEqual(1440);
  });

  it('click without drag returns null (full screen)', () => {
    expect(toImageRect({ x: 10, y: 10, w: 2, h: 1 }, { w: 1920, h: 1080 }, { w: 1920, h: 1080 })).toBeNull();
  });

  it('size label shows physical pixels', () => {
    expect(imageSizeOf(sel, { w: 2880, h: 1620 }, { w: 1920, h: 1080 })).toEqual({ w: 600, h: 450 });
  });
});
