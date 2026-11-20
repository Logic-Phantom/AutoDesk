// Compare Desk 핵심 로직. 줄 단위 diff로 좌우 정렬 행을 만들고, 수정된 줄은 단어/글자 단위로 강조한다.
import { diffArrays, diffChars, diffWordsWithSpace } from 'diff';
import { parseDelimited } from './table';

export type RowKind = 'same' | 'added' | 'removed' | 'modified';
export interface Seg {
  text: string;
  kind: 'same' | 'add' | 'del';
}
export interface Row {
  kind: RowKind;
  leftNo: number | null;
  rightNo: number | null;
  left: Seg[];
  right: Seg[];
}
export interface Stats {
  added: number;
  removed: number;
  modified: number;
  same: number;
}

export interface Options {
  ignoreWhitespace: boolean;
  ignoreCase: boolean;
  /** word: 단어 단위 강조, char: 글자 단위(한국어 문장 수정 확인에 유리) */
  granularity: 'word' | 'char';
}

const norm = (s: string, o: Options) => {
  let x = s;
  if (o.ignoreWhitespace) x = x.replace(/\s+/g, ' ').trim();
  if (o.ignoreCase) x = x.toLowerCase();
  return x;
};

/** 수정된 줄의 좌/우 강조 조각 */
export function inlineDiff(a: string, b: string, o: Options): { left: Seg[]; right: Seg[] } {
  const parts = o.granularity === 'char' ? diffChars(a, b, { ignoreCase: o.ignoreCase }) : diffWordsWithSpace(a, b, { ignoreCase: o.ignoreCase });
  const left: Seg[] = [];
  const right: Seg[] = [];
  for (const p of parts) {
    if (p.added) right.push({ text: p.value, kind: 'add' });
    else if (p.removed) left.push({ text: p.value, kind: 'del' });
    else {
      left.push({ text: p.value, kind: 'same' });
      right.push({ text: p.value, kind: 'same' });
    }
  }
  return { left, right };
}

/** 두 줄이 '수정'으로 묶일 만큼 비슷한지(공통 글자 비율) */
/** 글자 2개씩 묶은 조각(bigram) 집합 — 줄 유사도를 빠르게(O(길이)) 계산하는 데 쓴다 */
function bigrams(s: string): Map<string, number> {
  const m = new Map<string, number>();
  const t = s.replace(/\s+/g, ' ').trim();
  if (t.length === 1) m.set(t, 1);
  for (let i = 0; i < t.length - 1; i++) {
    const g = t.slice(i, i + 2);
    m.set(g, (m.get(g) ?? 0) + 1);
  }
  return m;
}

function dice(x: Map<string, number>, y: Map<string, number>): number {
  let sx = 0;
  let sy = 0;
  let common = 0;
  for (const v of x.values()) sx += v;
  for (const v of y.values()) sy += v;
  if (sx === 0 && sy === 0) return 1;
  for (const [k, v] of x) {
    const w = y.get(k);
    if (w) common += Math.min(v, w);
  }
  return (2 * common) / (sx + sy || 1);
}

/** 두 줄이 '수정'으로 묶일 만큼 비슷한지(0~1). 큰 문서에서도 빠르도록 bigram Dice 계수 사용 */
export function similarity(a: string, b: string): number {
  if (a === b) return 1;
  return dice(bigrams(a), bigrams(b));
}

const MATCH_MIN = 0.35;

/**
 * 삭제된 줄 목록과 추가된 줄 목록을 순서를 유지하며 짝짓는다(유사도 합 최대, 동적 계획법).
 * 반환: [왼쪽 인덱스|null, 오른쪽 인덱스|null] 순서열. 블록이 크면(>200줄) 순서대로 짝짓는다.
 */
export function alignBlock(a: string[], b: string[]): [number | null, number | null][] {
  const out: [number | null, number | null][] = [];
  if (a.length * b.length > 250000) {
    const m = Math.max(a.length, b.length);
    for (let k = 0; k < m; k++) {
      if (k < a.length && k < b.length && similarity(a[k], b[k]) >= MATCH_MIN) out.push([k, k]);
      else {
        if (k < a.length) out.push([k, null]);
        if (k < b.length) out.push([null, k]);
      }
    }
    return out;
  }
  const ga = a.map(bigrams);
  const gb = b.map(bigrams);
  const sim = ga.map((x) => gb.map((y) => dice(x, y)));
  const dp = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const s = sim[i - 1][j - 1];
      dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1], s >= MATCH_MIN ? dp[i - 1][j - 1] + s : -1);
    }
  }
  let i = a.length;
  let j = b.length;
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && sim[i - 1][j - 1] >= MATCH_MIN && dp[i][j] === dp[i - 1][j - 1] + sim[i - 1][j - 1]) {
      out.push([i - 1, j - 1]);
      i--;
      j--;
    } else if (i > 0 && (j === 0 || dp[i][j] === dp[i - 1][j])) {
      out.push([i - 1, null]);
      i--;
    } else {
      out.push([null, j - 1]);
      j--;
    }
  }
  return out.reverse();
}

export function compareLines(leftText: string, rightText: string, o: Options): { rows: Row[]; stats: Stats } {
  const L = leftText.replace(/\r\n?/g, '\n').split('\n');
  const R = rightText.replace(/\r\n?/g, '\n').split('\n');
  if (L[L.length - 1] === '' && R[R.length - 1] === '') {
    L.pop();
    R.pop();
  }
  const parts = diffArrays(L.map((l) => norm(l, o)), R.map((r) => norm(r, o)));
  const rows: Row[] = [];
  let li = 0;
  let ri = 0;
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    const n = p.value.length;
    if (!p.added && !p.removed) {
      for (let k = 0; k < n; k++) {
        rows.push({ kind: 'same', leftNo: li + 1, rightNo: ri + 1, left: [{ text: L[li], kind: 'same' }], right: [{ text: R[ri], kind: 'same' }] });
        li++;
        ri++;
      }
    } else if (p.removed && parts[i + 1]?.added) {
      // 삭제 블록 + 추가 블록: 비슷한 줄끼리 순서를 지키며 최적으로 짝지어 '수정'으로, 나머지는 삭제/추가로
      const add = parts[i + 1];
      const delLines = L.slice(li, li + n);
      const addLines = R.slice(ri, ri + add.value.length);
      for (const [a, b] of alignBlock(delLines, addLines)) {
        if (a !== null && b !== null) {
          const { left, right } = inlineDiff(L[li], R[ri], o);
          rows.push({ kind: 'modified', leftNo: li + 1, rightNo: ri + 1, left, right });
          li++;
          ri++;
        } else if (a !== null) {
          rows.push({ kind: 'removed', leftNo: li + 1, rightNo: null, left: [{ text: L[li], kind: 'del' }], right: [] });
          li++;
        } else {
          rows.push({ kind: 'added', leftNo: null, rightNo: ri + 1, left: [], right: [{ text: R[ri], kind: 'add' }] });
          ri++;
        }
      }
      i++;
    } else if (p.removed) {
      for (let k = 0; k < n; k++) {
        rows.push({ kind: 'removed', leftNo: li + 1, rightNo: null, left: [{ text: L[li], kind: 'del' }], right: [] });
        li++;
      }
    } else {
      for (let k = 0; k < n; k++) {
        rows.push({ kind: 'added', leftNo: null, rightNo: ri + 1, left: [], right: [{ text: R[ri], kind: 'add' }] });
        ri++;
      }
    }
  }
  return { rows, stats: countStats(rows) };
}

export function countStats(rows: Row[]): Stats {
  const s: Stats = { added: 0, removed: 0, modified: 0, same: 0 };
  for (const r of rows) s[r.kind === 'same' ? 'same' : r.kind]++;
  return s;
}

/** CSV 행 비교: 키 열 기준으로 같은 행을 찾아 셀 단위로 비교. 키가 없으면 순서대로 비교. */
export interface CsvRow {
  kind: RowKind;
  key: string;
  left: string[] | null;
  right: string[] | null;
  changedCols: number[];
}

export function compareCsv(leftText: string, rightText: string, keyCol: number, o: Options): { header: string[]; rows: CsvRow[]; stats: Stats } {
  const delim = (t: string) => ((t.split('\n')[0] ?? '').includes('\t') ? '\t' : ',');
  const L = parseDelimited(leftText, delim(leftText));
  const R = parseDelimited(rightText, delim(rightText));
  const header = (L[0] ?? R[0] ?? []).slice();
  const lBody = L.slice(1);
  const rBody = R.slice(1);
  const keyOf = (r: string[], i: number) => (keyCol >= 0 ? norm(r[keyCol] ?? '', o) : String(i));
  const rMap = new Map<string, string[]>();
  rBody.forEach((r, i) => rMap.set(keyOf(r, i), r));
  const seen = new Set<string>();
  const rows: CsvRow[] = [];
  lBody.forEach((l, i) => {
    const k = keyOf(l, i);
    const r = rMap.get(k);
    if (!r) {
      rows.push({ kind: 'removed', key: k, left: l, right: null, changedCols: [] });
      return;
    }
    seen.add(k);
    const width = Math.max(l.length, r.length);
    const changed: number[] = [];
    for (let c = 0; c < width; c++) if (norm(l[c] ?? '', o) !== norm(r[c] ?? '', o)) changed.push(c);
    rows.push({ kind: changed.length ? 'modified' : 'same', key: k, left: l, right: r, changedCols: changed });
  });
  rBody.forEach((r, i) => {
    const k = keyOf(r, i);
    if (!seen.has(k)) rows.push({ kind: 'added', key: k, left: null, right: r, changedCols: [] });
  });
  const stats: Stats = { added: 0, removed: 0, modified: 0, same: 0 };
  for (const r of rows) stats[r.kind === 'same' ? 'same' : r.kind]++;
  return { header, rows, stats };
}

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const segHtml = (segs: Seg[]) => segs.map((s) => (s.kind === 'same' ? esc(s.text) : `<mark class="${s.kind}">${esc(s.text)}</mark>`)).join('');

/** 비교 결과를 단독 HTML 리포트로 */
export function reportHtml(title: string, leftName: string, rightName: string, rows: Row[], stats: Stats, changesOnly: boolean): string {
  const body = rows
    .filter((r) => !changesOnly || r.kind !== 'same')
    .map(
      (r) =>
        `<tr class="${r.kind}"><td class="n">${r.leftNo ?? ''}</td><td>${segHtml(r.left)}</td><td class="n">${r.rightNo ?? ''}</td><td>${segHtml(r.right)}</td></tr>`,
    )
    .join('\n');
  return `<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>${esc(title)}</title><style>
body{font-family:'Malgun Gothic',system-ui,sans-serif;margin:24px;color:#1f2328}
table{border-collapse:collapse;width:100%;table-layout:fixed;font-size:13px}
td{border-bottom:1px solid #eee;padding:3px 6px;vertical-align:top;white-space:pre-wrap;word-break:break-all}
td.n{width:44px;color:#999;text-align:right;font-family:Consolas,monospace}
tr.added td{background:#eefbf1}tr.removed td{background:#fdeeee}tr.modified td{background:#fff9e6}
mark.add{background:#b7ebc6}mark.del{background:#f6c3c3;text-decoration:line-through}
.sum span{margin-right:14px}</style></head><body>
<h1>${esc(title)}</h1><p>왼쪽: ${esc(leftName)}<br>오른쪽: ${esc(rightName)}</p>
<p class="sum"><span>수정 ${stats.modified}</span><span>추가 ${stats.added}</span><span>삭제 ${stats.removed}</span><span>같음 ${stats.same}</span></p>
<table>${body}</table><p style="color:#999">AutoDesk Compare Desk · ${new Date().toLocaleString('ko-KR')}</p></body></html>`;
}

/** 변경점만 텍스트로(메일/메신저에 붙여넣기용) */
export function changesText(rows: Row[]): string {
  const t = (segs: Seg[]) => segs.map((s) => s.text).join('');
  return rows
    .filter((r) => r.kind !== 'same')
    .map((r) => {
      if (r.kind === 'added') return `+ [${r.rightNo}] ${t(r.right)}`;
      if (r.kind === 'removed') return `- [${r.leftNo}] ${t(r.left)}`;
      return `~ [${r.leftNo}→${r.rightNo}] ${t(r.left)}\n  → ${t(r.right)}`;
    })
    .join('\n');
}
