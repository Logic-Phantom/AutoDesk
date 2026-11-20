import { save } from '@tauri-apps/plugin-dialog';
import { ClipboardPaste, Copy, Download, Eraser } from 'lucide-react';
import { useMemo, useState } from 'react';
import { convert, OUTPUT_EXT, OUTPUT_LABEL, parse, type InputFormat, type OutputFormat } from '../../lib/table';
import { tools } from '../../lib/toolsApi';
import { useAction } from '../../lib/useCore';
import { Card, Notice } from '../../components/ui';

const INPUT_LABEL: Record<InputFormat, string> = {
  auto: '자동 감지',
  tsv: '엑셀 복사(TSV)',
  csv: 'CSV',
  json: 'JSON',
  html: 'HTML 표',
  markdown: 'Markdown 표',
};

export function TableConverter() {
  const [text, setText] = useState('');
  const [html, setHtml] = useState<string | undefined>();
  const [inFmt, setInFmt] = useState<InputFormat>('auto');
  const [outFmt, setOutFmt] = useState<OutputFormat>('markdown');
  const [hasHeader, setHasHeader] = useState(true);
  const [colOff, setColOff] = useState<Record<number, boolean>>({});
  const [rowOff, setRowOff] = useState<Record<number, boolean>>({});
  const { message, run, setMessage } = useAction();

  const parsed = useMemo(() => {
    if (!text.trim() && !html) return { grid: [] as string[][], format: 'csv' as const, error: null as string | null };
    try {
      return { ...parse(text, inFmt, html), error: null };
    } catch (e) {
      return { grid: [], format: 'csv' as const, error: `입력을 해석할 수 없습니다: ${(e as Error).message}` };
    }
  }, [text, html, inFmt]);

  const selected = useMemo(
    () => parsed.grid.filter((_, ri) => !rowOff[ri]).map((r) => r.filter((_, ci) => !colOff[ci])),
    [parsed.grid, colOff, rowOff],
  );
  const output = useMemo(() => (selected.length ? convert(selected, outFmt, hasHeader) : ''), [selected, outFmt, hasHeader]);
  const width = parsed.grid[0]?.length ?? 0;

  function onPaste(e: React.ClipboardEvent<HTMLTextAreaElement>) {
    const h = e.clipboardData.getData('text/html');
    const t = e.clipboardData.getData('text/plain');
    // 엑셀/웹에서 복사하면 HTML 표가 함께 들어온다 → 셀 병합·줄바꿈을 더 정확히 해석
    if (h && /<table[\s>]/i.test(h)) {
      e.preventDefault();
      setHtml(h);
      setText(t);
    } else {
      setHtml(undefined);
    }
    setColOff({});
    setRowOff({});
  }

  async function saveFile() {
    const path = await save({
      title: '변환 결과 저장',
      defaultPath: `표.${OUTPUT_EXT[outFmt]}`,
      filters: [{ name: OUTPUT_LABEL[outFmt], extensions: [OUTPUT_EXT[outFmt]] }],
    });
    if (!path) return;
    await run(() => tools.saveText(path, output, outFmt === 'csv-excel'), `저장했습니다: ${path}`);
  }

  return (
    <div className="page wide">
      <header className="page-head">
        <div>
          <h1>표 변환</h1>
          <p className="muted">엑셀·웹·CSV·JSON 표를 붙여넣으면 형식을 자동으로 알아내고, 원하는 형식으로 바꿔 줍니다. 모든 처리는 PC 안에서만 합니다.</p>
        </div>
      </header>
      <div className="split">
        <Card
          title="입력"
          actions={
            <div className="inline tight">
              <select className="input" value={inFmt} onChange={(e) => setInFmt(e.target.value as InputFormat)} aria-label="입력 형식">
                {Object.entries(INPUT_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))}
              </select>
              <button className="icon-btn" aria-label="입력 지우기" title="지우기" onClick={() => { setText(''); setHtml(undefined); setMessage(null); }}>
                <Eraser size={16} />
              </button>
            </div>
          }
        >
          <textarea
            className="input code-area"
            placeholder={'여기에 표를 붙여넣으세요 (Ctrl+V)\n예) 엑셀에서 셀 범위 복사, 웹페이지 표 복사, CSV/JSON 텍스트'}
            value={text}
            onPaste={onPaste}
            onChange={(e) => { setText(e.target.value); setHtml(undefined); }}
            spellCheck={false}
          />
          <p className="muted small">
            {parsed.error ?? (parsed.grid.length ? <><ClipboardPaste size={12} aria-hidden /> {INPUT_LABEL[parsed.format]} · {parsed.grid.length}행 × {width}열 {html ? '· 서식 있는 표로 읽음' : ''}</> : '아직 입력이 없습니다')}
          </p>
        </Card>
        <Card
          title="결과"
          actions={
            <div className="inline tight">
              <select className="input" value={outFmt} onChange={(e) => setOutFmt(e.target.value as OutputFormat)} aria-label="출력 형식">
                {Object.entries(OUTPUT_LABEL).map(([k, v]) => (
                  <option key={k} value={k}>{v}</option>
                ))}
              </select>
            </div>
          }
        >
          <textarea className="input code-area" readOnly value={output} spellCheck={false} aria-label="변환 결과" />
          <div className="inline">
            <button className="btn primary" disabled={!output} onClick={() => run(() => tools.textCopy(output), '복사했습니다')}>
              <Copy size={16} aria-hidden /> 복사
            </button>
            <button className="btn" disabled={!output} onClick={saveFile}>
              <Download size={16} aria-hidden /> 파일로 저장
            </button>
            <label className="check">
              <input type="checkbox" checked={hasHeader} onChange={(e) => setHasHeader(e.target.checked)} /> 첫 행은 제목
            </label>
          </div>
          <Notice message={message} />
        </Card>
      </div>

      {parsed.grid.length > 0 && (
        <Card title="미리보기 · 열/행 선택" actions={<span className="muted small">체크를 해제한 열·행은 결과에서 빠집니다</span>}>
          <div className="table-scroll">
            <table className="table grid">
              <thead>
                <tr>
                  <th />
                  {Array.from({ length: width }, (_, ci) => (
                    <th key={ci}>
                      <label className="check">
                        <input type="checkbox" checked={!colOff[ci]} onChange={(e) => setColOff({ ...colOff, [ci]: !e.target.checked })} />
                        {hasHeader ? parsed.grid[0][ci] || `열${ci + 1}` : `열${ci + 1}`}
                      </label>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {parsed.grid.slice(hasHeader ? 1 : 0, 200).map((r, i) => {
                  const ri = i + (hasHeader ? 1 : 0);
                  return (
                    <tr key={ri} className={rowOff[ri] ? 'off' : ''}>
                      <td>
                        <input type="checkbox" aria-label={`${ri}행 포함`} checked={!rowOff[ri]} onChange={(e) => setRowOff({ ...rowOff, [ri]: !e.target.checked })} />
                      </td>
                      {r.map((c, ci) => (
                        <td key={ci} className={colOff[ci] ? 'off' : ''}>{c}</td>
                      ))}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {parsed.grid.length > 201 && <p className="muted small">미리보기는 200행까지만 표시합니다(변환은 전체).</p>}
        </Card>
      )}
    </div>
  );
}
