// 표 변환 핵심 로직(순수 함수). 엑셀/웹에서 복사한 표를 여러 형식으로 바꾼다.

export type Grid = string[][];
export type InputFormat = 'auto' | 'tsv' | 'csv' | 'json' | 'html' | 'markdown';
export type OutputFormat = 'markdown' | 'html' | 'csv' | 'csv-excel' | 'tsv' | 'json' | 'json-array';

/** RFC 4180 CSV 파서(따옴표 안의 쉼표·줄바꿈·"" 이스케이프 지원). delimiter로 TSV에도 사용. */
export function parseDelimited(text: string, delimiter: string): Grid {
  const rows: Grid = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  const s = text.replace(/\r\n?/g, '\n');
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quoted) {
      if (c === '"') {
        if (s[i + 1] === '"') {
          cell += '"';
          i++;
        } else quoted = false;
      } else cell += c;
    } else if (c === '"' && cell === '') {
      quoted = true;
    } else if (c === delimiter) {
      row.push(cell);
      cell = '';
    } else if (c === '\n') {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
    } else cell += c;
  }
  if (cell !== '' || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows.filter((r) => !(r.length === 1 && r[0] === ''));
}

export function parseHtmlTable(html: string): Grid {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const table = doc.querySelector('table');
  if (!table) return [];
  return Array.from(table.querySelectorAll('tr')).map((tr) =>
    Array.from(tr.querySelectorAll('th,td')).flatMap((td) => {
      const text = (td.textContent ?? '').replace(/\s+/g, ' ').trim();
      const span = Math.min(Number((td as HTMLTableCellElement).colSpan) || 1, 50);
      return [text, ...Array(span - 1).fill('')];
    }),
  );
}

export function parseMarkdown(text: string): Grid {
  const lines = text.split(/\r?\n/).map((l) => l.trim()).filter((l) => l.startsWith('|'));
  return lines
    .filter((l) => !/^\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?$/.test(l))
    .map((l) =>
      l
        .replace(/^\|/, '')
        .replace(/\|$/, '')
        .split(/(?<!\\)\|/)
        .map((c) => c.trim().replace(/\\\|/g, '|')),
    );
}

function cellToString(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'object') return JSON.stringify(v);
  return String(v);
}

export function parseJson(text: string): Grid {
  const data: unknown = JSON.parse(text);
  const arr = Array.isArray(data) ? data : [data];
  if (arr.length === 0) return [];
  if (arr.every((r) => Array.isArray(r))) return (arr as unknown[][]).map((r) => r.map(cellToString));
  if (arr.every((r) => r && typeof r === 'object')) {
    const keys: string[] = [];
    for (const r of arr as Record<string, unknown>[]) for (const k of Object.keys(r)) if (!keys.includes(k)) keys.push(k);
    return [keys, ...(arr as Record<string, unknown>[]).map((r) => keys.map((k) => cellToString(r[k])))];
  }
  return arr.map((v) => [cellToString(v)]);
}

/** 입력 형식 추측: JSON → HTML → Markdown → 탭 → 쉼표/세미콜론 */
export function detectFormat(text: string, html?: string): Exclude<InputFormat, 'auto'> {
  const t = text.trim();
  if (html && /<table[\s>]/i.test(html)) return 'html';
  if ((t.startsWith('[') || t.startsWith('{')) && (() => { try { JSON.parse(t); return true; } catch { return false; } })()) return 'json';
  if (/<table[\s>]/i.test(t)) return 'html';
  if (t.split('\n').filter((l) => l.trim().startsWith('|')).length >= 2) return 'markdown';
  const first = t.split('\n').slice(0, 5).join('\n');
  if (first.includes('\t')) return 'tsv';
  return 'csv';
}

export function parse(text: string, format: InputFormat, html?: string): { grid: Grid; format: Exclude<InputFormat, 'auto'> } {
  const f = format === 'auto' ? detectFormat(text, html) : format;
  let grid: Grid;
  switch (f) {
    case 'html':
      grid = parseHtmlTable(html && /<table/i.test(html) ? html : text);
      break;
    case 'json':
      grid = parseJson(text);
      break;
    case 'markdown':
      grid = parseMarkdown(text);
      break;
    case 'tsv':
      grid = parseDelimited(text, '\t');
      break;
    default: {
      const firstLine = text.split('\n')[0] ?? '';
      const semi = (firstLine.match(/;/g) ?? []).length > (firstLine.match(/,/g) ?? []).length;
      grid = parseDelimited(text, semi ? ';' : ',');
    }
  }
  return { grid: normalize(grid), format: f };
}

/** 모든 행의 열 수를 맞추고 앞뒤 공백 정리 */
export function normalize(grid: Grid): Grid {
  const width = Math.max(0, ...grid.map((r) => r.length));
  return grid.map((r) => Array.from({ length: width }, (_, i) => (r[i] ?? '').trim()));
}

export function select(grid: Grid, cols: boolean[], rows: boolean[]): Grid {
  return grid.filter((_, ri) => rows[ri] !== false).map((r) => r.filter((_, ci) => cols[ci] !== false));
}

const csvCell = (v: string, d: string) => (/["\n\r]/.test(v) || v.includes(d) || /^\s|\s$/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
const escHtml = (v: string) => v.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const mdCell = (v: string) => v.replace(/\|/g, '\\|').replace(/\r?\n/g, '<br>');

export function convert(grid: Grid, out: OutputFormat, hasHeader: boolean): string {
  if (grid.length === 0) return '';
  const [head, ...body] = hasHeader ? grid : [grid[0].map((_, i) => `열${i + 1}`), ...grid];
  switch (out) {
    case 'markdown': {
      const line = (r: string[]) => `| ${r.map(mdCell).join(' | ')} |`;
      return [line(head), `| ${head.map(() => '---').join(' | ')} |`, ...body.map(line)].join('\n');
    }
    case 'html':
      return [
        '<table>',
        '  <thead>',
        `    <tr>${head.map((h) => `<th>${escHtml(h)}</th>`).join('')}</tr>`,
        '  </thead>',
        '  <tbody>',
        ...body.map((r) => `    <tr>${r.map((c) => `<td>${escHtml(c)}</td>`).join('')}</tr>`),
        '  </tbody>',
        '</table>',
      ].join('\n');
    case 'csv':
    case 'csv-excel':
      return [head, ...body].map((r) => r.map((c) => csvCell(c, ',')).join(',')).join('\r\n');
    case 'tsv':
      return [head, ...body].map((r) => r.map((c) => c.replace(/[\t\r\n]+/g, ' ')).join('\t')).join('\n');
    case 'json':
      return JSON.stringify(body.map((r) => Object.fromEntries(head.map((h, i) => [h || `열${i + 1}`, typed(r[i] ?? '')]))), null, 2);
    case 'json-array':
      return JSON.stringify([head, ...body], null, 2);
  }
}

/** JSON 출력 시 숫자/불리언은 실제 타입으로(앞자리 0이 있는 코드값은 문자열 유지) */
export function typed(v: string): string | number | boolean | null {
  if (v === '') return '';
  if (v === 'true') return true;
  if (v === 'false') return false;
  if (/^-?(0|[1-9]\d{0,14})(\.\d+)?$/.test(v)) return Number(v);
  return v;
}

export const OUTPUT_LABEL: Record<OutputFormat, string> = {
  markdown: 'Markdown 표',
  html: 'HTML 표',
  csv: 'CSV',
  'csv-excel': 'CSV (엑셀용, UTF-8 BOM)',
  tsv: 'TSV (엑셀 붙여넣기용)',
  json: 'JSON (객체 배열)',
  'json-array': 'JSON (2차원 배열)',
};

export const OUTPUT_EXT: Record<OutputFormat, string> = {
  markdown: 'md',
  html: 'html',
  csv: 'csv',
  'csv-excel': 'csv',
  tsv: 'tsv',
  json: 'json',
  'json-array': 'json',
};
