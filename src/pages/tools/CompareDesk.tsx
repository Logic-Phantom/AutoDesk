import { open, save } from '@tauri-apps/plugin-dialog';
import { ArrowDown, ArrowUp, ArrowLeftRight, Copy, FileDown, FolderOpen } from 'lucide-react';
import { useDeferredValue, useMemo, useRef, useState } from 'react';
import { changesText, compareCsv, compareLines, reportHtml, type Options, type Seg } from '../../lib/compare';
import { errorMessage } from '../../lib/api';
import { fileName, tools } from '../../lib/toolsApi';
import { useAction } from '../../lib/useCore';
import { Card, Empty, Notice } from '../../components/ui';

interface Side {
  text: string;
  name: string;
}

const Segs = ({ segs }: { segs: Seg[] }) => (
  <>
    {segs.map((s, i) => (s.kind === 'same' ? <span key={i}>{s.text}</span> : <mark key={i} className={s.kind}>{s.text}</mark>))}
  </>
);

export function CompareDesk() {
  const [left, setLeft] = useState<Side>({ text: '', name: '왼쪽(원본)' });
  const [right, setRight] = useState<Side>({ text: '', name: '오른쪽(수정본)' });
  const [mode, setMode] = useState<'text' | 'csv'>('text');
  const [opt, setOpt] = useState<Options>({ ignoreWhitespace: false, ignoreCase: false, granularity: 'char' });
  const [changesOnly, setChangesOnly] = useState(false);
  const [keyCol, setKeyCol] = useState(0);
  const [cursor, setCursor] = useState(-1);
  const tableRef = useRef<HTMLDivElement>(null);
  const { message, run, setMessage } = useAction();

  // 큰 문서를 입력하는 동안 매 글자마다 비교하지 않도록 지연된 값으로 계산
  const lt = useDeferredValue(left.text);
  const rt = useDeferredValue(right.text);
  const textResult = useMemo(() => (mode === 'text' && (lt || rt) ? compareLines(lt, rt, opt) : null), [mode, lt, rt, opt]);
  const csvResult = useMemo(() => (mode === 'csv' && (lt || rt) ? compareCsv(lt, rt, keyCol, opt) : null), [mode, lt, rt, keyCol, opt]);
  const stats = textResult?.stats ?? csvResult?.stats;
  const changeIdx = useMemo(() => (textResult ? textResult.rows.map((r, i) => (r.kind !== 'same' ? i : -1)).filter((i) => i >= 0) : []), [textResult]);

  async function openFile(setter: (s: Side) => void) {
    const p = await open({ multiple: false, directory: false, title: '비교할 파일', filters: [{ name: '텍스트·문서', extensions: ['txt', 'md', 'csv', 'tsv', 'json', 'html', 'xml', 'log', 'docx', 'java', 'js', 'ts', 'sql', 'yml', 'yaml', 'properties'] }] });
    if (typeof p !== 'string') return;
    try {
      const text = await tools.readText(p);
      setter({ text, name: fileName(p) });
      if (/\.(csv|tsv)$/i.test(p)) setMode('csv');
      setMessage(null);
    } catch (e) {
      setMessage({ kind: 'err', text: errorMessage(e) });
    }
  }

  function jump(dir: 1 | -1) {
    if (!changeIdx.length) return;
    const pos = changeIdx.findIndex((i) => i > cursor);
    const next = dir === 1 ? changeIdx[pos === -1 ? 0 : pos] : [...changeIdx].reverse().find((i) => i < cursor) ?? changeIdx[changeIdx.length - 1];
    setCursor(next);
    tableRef.current?.querySelector(`[data-row="${next}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }

  async function saveReport() {
    if (!textResult) return;
    const path = await save({ title: '비교 리포트 저장', defaultPath: '비교결과.html', filters: [{ name: 'HTML', extensions: ['html'] }] });
    if (!path) return;
    await run(() => tools.saveText(path, reportHtml('문서 비교 결과', left.name, right.name, textResult.rows, textResult.stats, changesOnly), false), `리포트를 저장했습니다: ${path}`);
  }

  const panel = (side: Side, setter: (s: Side) => void, label: string) => (
    <Card
      title={side.name}
      actions={
        <button className="btn small" onClick={() => openFile(setter)}>
          <FolderOpen size={14} aria-hidden /> 파일 열기
        </button>
      }
    >
      <textarea
        className="input code-area short"
        value={side.text}
        placeholder={`${label} 내용을 붙여넣거나 파일을 여세요 (TXT·MD·CSV·DOCX…)`}
        onChange={(e) => setter({ ...side, text: e.target.value })}
        spellCheck={false}
      />
    </Card>
  );

  return (
    <div className="page wide">
      <header className="page-head">
        <div>
          <h1>문서 비교</h1>
          <p className="muted">두 문서나 텍스트의 차이를 줄·단어·글자 단위로 보여줍니다. 계약서·안내문 수정 전후 확인, CSV 데이터 변경 확인에 쓰세요. DOCX는 본문 텍스트만 비교합니다.</p>
        </div>
      </header>
      <div className="split">
        {panel(left, setLeft, '원본')}
        {panel(right, setRight, '수정본')}
      </div>

      <div className="toolbar">
        <div className="seg" role="group" aria-label="비교 방식">
          <button className={mode === 'text' ? 'on' : ''} onClick={() => setMode('text')}>문서·텍스트</button>
          <button className={mode === 'csv' ? 'on' : ''} onClick={() => setMode('csv')}>CSV 행 비교</button>
        </div>
        {mode === 'text' && (
          <div className="seg" role="group" aria-label="강조 단위">
            <button className={opt.granularity === 'char' ? 'on' : ''} onClick={() => setOpt({ ...opt, granularity: 'char' })}>글자</button>
            <button className={opt.granularity === 'word' ? 'on' : ''} onClick={() => setOpt({ ...opt, granularity: 'word' })}>단어</button>
          </div>
        )}
        {mode === 'csv' && (
          <label className="check">
            기준 열
            <select className="input" value={keyCol} onChange={(e) => setKeyCol(Number(e.target.value))}>
              <option value={-1}>(순서대로)</option>
              {(csvResult?.header ?? []).map((h, i) => (
                <option key={i} value={i}>{h || `열${i + 1}`}</option>
              ))}
            </select>
          </label>
        )}
        <label className="check"><input type="checkbox" checked={opt.ignoreWhitespace} onChange={(e) => setOpt({ ...opt, ignoreWhitespace: e.target.checked })} /> 공백 무시</label>
        <label className="check"><input type="checkbox" checked={opt.ignoreCase} onChange={(e) => setOpt({ ...opt, ignoreCase: e.target.checked })} /> 대소문자 무시</label>
        <label className="check"><input type="checkbox" checked={changesOnly} onChange={(e) => setChangesOnly(e.target.checked)} /> 변경된 부분만</label>
        <span className="spacer" />
        <button className="icon-btn" title="좌우 바꾸기" aria-label="좌우 바꾸기" onClick={() => { setLeft(right); setRight(left); }}>
          <ArrowLeftRight size={16} />
        </button>
        {mode === 'text' && (
          <>
            <button className="icon-btn" title="이전 변경" aria-label="이전 변경" onClick={() => jump(-1)}><ArrowUp size={16} /></button>
            <button className="icon-btn" title="다음 변경" aria-label="다음 변경" onClick={() => jump(1)}><ArrowDown size={16} /></button>
            <button className="btn small" disabled={!textResult} onClick={() => textResult && run(() => tools.textCopy(changesText(textResult.rows)), '변경점을 복사했습니다')}>
              <Copy size={14} aria-hidden /> 변경점 복사
            </button>
            <button className="btn small" disabled={!textResult} onClick={saveReport}>
              <FileDown size={14} aria-hidden /> HTML 리포트
            </button>
          </>
        )}
      </div>
      <Notice message={message} />

      {stats && (
        <div className="diff-stats">
          <span className="chip mod">수정 {stats.modified}</span>
          <span className="chip add">추가 {stats.added}</span>
          <span className="chip del">삭제 {stats.removed}</span>
          <span className="chip">같음 {stats.same}</span>
        </div>
      )}

      {!stats && <Empty>양쪽에 내용을 넣으면 바로 비교합니다.</Empty>}

      {textResult && (
        <div className="diff-table" ref={tableRef}>
          <table>
            <colgroup><col className="no" /><col /><col className="no" /><col /></colgroup>
            <tbody>
              {textResult.rows.map((r, i) =>
                changesOnly && r.kind === 'same' ? null : (
                  <tr key={i} data-row={i} className={`${r.kind} ${cursor === i ? 'cursor' : ''}`}>
                    <td className="no">{r.leftNo ?? ''}</td>
                    <td><Segs segs={r.left} /></td>
                    <td className="no">{r.rightNo ?? ''}</td>
                    <td><Segs segs={r.right} /></td>
                  </tr>
                ),
              )}
            </tbody>
          </table>
        </div>
      )}

      {csvResult && (
        <div className="diff-table">
          <table className="csv">
            <thead>
              <tr>
                <th>상태</th>
                {csvResult.header.map((h, i) => <th key={i}>{h || `열${i + 1}`}</th>)}
              </tr>
            </thead>
            <tbody>
              {csvResult.rows.map((r, i) =>
                changesOnly && r.kind === 'same' ? null : (
                  <tr key={i} className={r.kind}>
                    <td className="no">{{ same: '같음', modified: '수정', added: '추가', removed: '삭제' }[r.kind]}</td>
                    {csvResult.header.map((_, c) => {
                      const changed = r.changedCols.includes(c);
                      const lv = r.left?.[c] ?? '';
                      const rv = r.right?.[c] ?? '';
                      return (
                        <td key={c}>
                          {changed ? (<><mark className="del">{lv}</mark> → <mark className="add">{rv}</mark></>) : (r.right ? rv : lv)}
                        </td>
                      );
                    })}
                  </tr>
                ),
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
