import { useEffect, useState } from 'react';
import { api, type ClaudeStatus, type ModuleView, type Overview } from '../lib/api';
import { prettyShortcut } from '../lib/format';
import { useAction } from '../lib/useCore';
import { Notice, Row, Toggle } from '../components/ui';

interface AutoPasteConfig {
  shortcut: string;
  trimTrailing: boolean;
  removeZeroWidth: boolean;
  nbspToSpace: boolean;
  collapseBlankLines: boolean;
  maxBlankLines: number;
  trimEnds: boolean;
  tabsToSpaces: boolean;
  tabWidth: number;
  stripHtml: boolean;
  lineEnding: 'keep' | 'lf' | 'crlf';
  autoCodeMode: boolean;
  notify: boolean;
  aiEnabled: boolean;
  aiConsent: boolean;
  aiShortcut: string;
  aiModel: string;
}

const RULES: [keyof AutoPasteConfig, string][] = [
  ['trimTrailing', '줄 끝 공백 제거'],
  ['removeZeroWidth', '보이지 않는 공백(U+200B, BOM) 제거 — 이모지 결합 문자는 보존'],
  ['nbspToSpace', 'NBSP(줄바꿈 없는 공백)를 일반 공백으로'],
  ['collapseBlankLines', '연속된 빈 줄 줄이기'],
  ['trimEnds', '맨 앞뒤 빈 줄·공백 제거'],
  ['tabsToSpaces', '탭을 공백으로'],
  ['stripHtml', 'HTML 태그 제거(텍스트로 복사된 경우)'],
  ['autoCodeMode', '코드/로그로 보이면 들여쓰기·공백을 건드리지 않기'],
  ['notify', '정리 후 알림 표시(건수만, 내용은 표시 안 함)'],
];

export function AutoPastePanel({ m, data }: { m: ModuleView; data: Overview }) {
  const cfg = m.config as unknown as AutoPasteConfig;
  const [shortcut, setShortcut] = useState(cfg.shortcut);
  const [claude, setClaude] = useState<ClaudeStatus | null>(null);
  const { message, run } = useAction();
  useEffect(() => {
    api.aiStatus().then(setClaude).catch(() => setClaude({ available: false, path: null, version: null }));
  }, []);

  const save = (patch: Partial<AutoPasteConfig>, ok = '저장했습니다') => run(() => api.setModuleConfig('autopaste', { ...cfg, ...patch }), ok);

  return (
    <>
      <p>
        텍스트를 복사한 뒤 <kbd>{prettyShortcut(cfg.shortcut, data.platform)}</kbd> 를 누르면 클립보드가 정리됩니다. 그다음 평소처럼 붙여넣기 하세요.
        클립보드는 단축키를 누른 순간에만 읽고, 내용은 어디에도 저장하지 않습니다.
      </p>
      <label className="field">
        <span>단축키(예: CommandOrControl+Alt+Shift+KeyV)</span>
        <div className="inline">
          <input className="input mono" value={shortcut} onChange={(e) => setShortcut(e.target.value)} />
          <button className="btn" onClick={() => save({ shortcut }, '단축키를 다시 등록했습니다')}>적용</button>
        </div>
      </label>
      <h3>정리 규칙</h3>
      {RULES.map(([key, label]) => (
        <Row key={key} label={label}>
          <Toggle label={label} checked={Boolean(cfg[key])} onChange={(v) => save({ [key]: v } as Partial<AutoPasteConfig>)} />
        </Row>
      ))}
      <Row label="빈 줄 최대 연속 개수">
        <input className="input num" type="number" min={0} max={5} value={cfg.maxBlankLines}
          onChange={(e) => save({ maxBlankLines: Number(e.target.value) })} />
      </Row>
      <Row label="줄바꿈 형식">
        <select className="input" value={cfg.lineEnding} onChange={(e) => save({ lineEnding: e.target.value as AutoPasteConfig['lineEnding'] })}>
          <option value="keep">그대로</option>
          <option value="lf">LF</option>
          <option value="crlf">CRLF</option>
        </select>
      </Row>

      <h3>선택 기능: Claude Code로 맞춤법 다듬기</h3>
      <p className="muted small">
        PC에 설치·로그인된 Claude Code CLI(<code>claude -p</code>)를 호출합니다. API 키는 저장하지 않으며, 사용 중인 Claude 구독 플랜의
        사용량 한도 안에서 동작합니다(추가 요금 없음, 한도 초과 시 실패로 표시). 전용 단축키를 누른 순간의 클립보드 텍스트만 Anthropic으로
        전송되며, 원문·결과는 로그에 남지 않습니다.
      </p>
      <p className="small">
        Claude Code:{' '}
        {claude == null ? '확인 중…' : claude.available ? <span className="ok-text">사용 가능 ({claude.version})</span> : <span className="warn-text">찾을 수 없음 — 설치 후 터미널에서 한 번 로그인하세요</span>}
      </p>
      <Row label="외부 전송 동의" hint="단축키를 누를 때 클립보드 텍스트를 Claude로 보내는 것에 동의합니다">
        <Toggle label="외부 전송 동의" checked={cfg.aiConsent}
          onChange={(v) => save(v ? { aiConsent: true } : { aiConsent: false, aiEnabled: false })} />
      </Row>
      <Row label={`AI 다듬기 단축키 사용 (${prettyShortcut(cfg.aiShortcut, data.platform)})`}>
        <Toggle label="AI 다듬기" checked={cfg.aiEnabled} disabled={!cfg.aiConsent || !claude?.available}
          onChange={(v) => save({ aiEnabled: v })} />
      </Row>
      <Row label="모델" hint="haiku가 가장 빠르고 사용량을 적게 씁니다">
        <select className="input" value={cfg.aiModel} onChange={(e) => save({ aiModel: e.target.value })}>
          <option value="haiku">haiku</option>
          <option value="sonnet">sonnet</option>
          <option value="">CLI 기본값</option>
        </select>
      </Row>
      <Notice message={message} />
    </>
  );
}
