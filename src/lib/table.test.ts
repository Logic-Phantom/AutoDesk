import { convert, detectFormat, parse, parseDelimited, typed } from './table';

describe('table converter', () => {
  it('parses quoted CSV with commas, quotes and newlines', () => {
    const g = parseDelimited('이름,메모\n"김, 철수","따옴표 ""안"" 줄\n바꿈"\n', ',');
    expect(g).toEqual([['이름', '메모'], ['김, 철수', '따옴표 "안" 줄\n바꿈']]);
  });

  it('detects formats', () => {
    expect(detectFormat('a\tb\n1\t2')).toBe('tsv');
    expect(detectFormat('a,b\n1,2')).toBe('csv');
    expect(detectFormat('[{"a":1}]')).toBe('json');
    expect(detectFormat('| a | b |\n|---|---|\n| 1 | 2 |')).toBe('markdown');
    expect(detectFormat('x', '<table><tr><td>1</td></tr></table>')).toBe('html');
  });

  it('excel paste (TSV) to markdown', () => {
    const { grid } = parse('품목\t수량\t단가\n사과\t3\t1,000\n배|특\t1\t2,500', 'auto');
    expect(convert(grid, 'markdown', true)).toBe(
      '| 품목 | 수량 | 단가 |\n| --- | --- | --- |\n| 사과 | 3 | 1,000 |\n| 배\\|특 | 1 | 2,500 |',
    );
  });

  it('csv to json with types and leading zeros kept', () => {
    const { grid } = parse('코드,이름,금액,사용\n007,가,1200,true', 'csv');
    expect(JSON.parse(convert(grid, 'json', true))).toEqual([{ 코드: '007', 이름: '가', 금액: 1200, 사용: true }]);
  });

  it('json objects to table with union of keys', () => {
    const { grid } = parse('[{"a":1},{"b":"x","a":2}]', 'json');
    expect(grid).toEqual([['a', 'b'], ['1', ''], ['2', 'x']]);
  });

  it('html table with colspan to csv', () => {
    const html = '<table><tr><th>이름</th><th colspan="2">연락처</th></tr><tr><td>A</td><td>010</td><td>a@b.c</td></tr></table>';
    const { grid } = parse('', 'html', html);
    expect(convert(grid, 'csv', true)).toBe('이름,연락처,\r\nA,010,a@b.c');
  });

  it('tsv <-> csv round trip and html escaping', () => {
    const { grid } = parse('a,b\n"1,2",<x>', 'csv');
    expect(convert(grid, 'tsv', true)).toBe('a\tb\n1,2\t<x>');
    expect(convert(grid, 'html', true)).toContain('<td>&lt;x&gt;</td>');
  });

  it('markdown back to grid', () => {
    const { grid } = parse('| a | b |\n|:--|--:|\n| 1 | x\\|y |', 'markdown');
    expect(grid).toEqual([['a', 'b'], ['1', 'x|y']]);
  });

  it('typed values', () => {
    expect(typed('12.5')).toBe(12.5);
    expect(typed('0012')).toBe('0012');
    expect(typed('abc')).toBe('abc');
  });

  it('no header generates column names', () => {
    const { grid } = parse('1,2', 'csv');
    expect(convert(grid, 'markdown', false).split('\n')[0]).toBe('| 열1 | 열2 |');
  });
});
