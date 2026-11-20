import { changesText, compareCsv, compareLines, reportHtml } from './compare';

const opt = { ignoreWhitespace: false, ignoreCase: false, granularity: 'word' as const };

describe('compare desk', () => {
  it('classifies same / modified / added / removed lines', () => {
    const left = '제1조 목적\n계약 기간은 1년으로 한다.\n삭제될 줄\n끝';
    const right = '제1조 목적\n계약 기간은 2년으로 한다.\n끝\n새 조항';
    const { rows, stats } = compareLines(left, right, opt);
    expect(stats).toEqual({ same: 2, modified: 1, removed: 1, added: 1 });
    const mod = rows.find((r) => r.kind === 'modified')!;
    const del = mod.left.filter((s) => s.kind === 'del').map((s) => s.text).join('');
    const add = mod.right.filter((s) => s.kind === 'add').map((s) => s.text).join('');
    expect(del).toContain('1');
    expect(add).toContain('2');
    expect(del).not.toContain('계약');
  });

  it('pairs the right lines when deletions and edits are mixed', () => {
    const left = ['제1조 목적', '제2조 기간은 12월 31일까지', '제4조 대금은 매월 말일에 지급한다.', '제5조 정보를 누설하지 않는다.'].join('\n');
    const right = ['제1조 목적', '제2조 기간은 6월 30일까지', '제5조 정보를 계약 종료 후에도 누설하지 않는다.', '제6조 분쟁은 서울중앙지방법원 관할'].join('\n');
    const { rows } = compareLines(left, right, opt);
    const kinds = rows.map((r) => r.kind);
    expect(kinds).toEqual(['same', 'modified', 'removed', 'modified', 'added']);
    const mod5 = rows[3];
    expect(mod5.leftNo).toBe(4);
    expect(mod5.rightNo).toBe(3);
  });

  it('stays fast on large changed blocks', () => {
    const NL = String.fromCharCode(10);
    const left = Array.from({ length: 400 }, (_, i) => `${i}번째 줄: 계약 조건 항목 ${i} 에 대한 설명 문장입니다. 금액은 ${i * 1000}원.`).join(NL);
    const right = Array.from({ length: 400 }, (_, i) => `${i}번째 줄: 계약 조건 항목 ${i} 에 대한 수정된 설명 문장입니다. 금액은 ${i * 1100}원.`).join(NL);
    const t0 = performance.now();
    const { stats } = compareLines(left, right, opt);
    const ms = performance.now() - t0;
    expect(stats.modified).toBe(400);
    expect(ms).toBeLessThan(3000);
  });

  it('ignore whitespace and case options', () => {
    expect(compareLines('Hello  World', 'hello world', { ...opt, ignoreWhitespace: true, ignoreCase: true }).stats.same).toBe(1);
    expect(compareLines('Hello  World', 'hello world', opt).stats.modified).toBe(1);
  });

  it('very different lines become remove+add, not modified', () => {
    const { stats } = compareLines('완전히 다른 문장입니다', 'xyz 12345 qwerty', opt);
    expect(stats.modified).toBe(0);
    expect(stats.removed).toBe(1);
    expect(stats.added).toBe(1);
  });

  it('char granularity for Korean edits', () => {
    const { rows } = compareLines('회의는 3시에 시작합니다', '회의는 4시에 시작합니다', { ...opt, granularity: 'char' });
    const r = rows[0];
    expect(r.right.filter((s) => s.kind === 'add').map((s) => s.text).join('')).toBe('4');
  });

  it('csv compare by key column', () => {
    const a = 'id,name,price\n1,사과,1000\n2,배,2000\n3,감,500';
    const b = 'id,name,price\n2,배,2500\n1,사과,1000\n4,귤,300';
    const { stats, rows } = compareCsv(a, b, 0, opt);
    expect(stats).toEqual({ same: 1, modified: 1, removed: 1, added: 1 });
    expect(rows.find((r) => r.key === '2')!.changedCols).toEqual([2]);
  });

  it('report and changes text', () => {
    const { rows, stats } = compareLines('a\nb', 'a\nc <x>', opt);
    expect(reportHtml('비교', 'L', 'R', rows, stats, true)).toContain('&lt;x&gt;');
    expect(changesText(rows)).toMatch(/^[-~+]/);
  });
});
