import { Power, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { api, type Overview, type UninstallPreview } from '../lib/api';
import { useAction } from '../lib/useCore';
import { Card, Notice, Row, Toggle } from '../components/ui';

export type Theme = 'system' | 'light' | 'dark';

export function Settings({ data, theme, setTheme }: { data: Overview; theme: Theme; setTheme: (t: Theme) => void }) {
  const { message, run } = useAction();
  const [notifications, setNotifications] = useState(true);
  useEffect(() => {
    api.getSettings().then((s) => setNotifications(s.notificationsEnabled)).catch(() => undefined);
  }, []);

  async function toggleLogin(v: boolean) {
    if (v && !data.backgroundMode) {
      const ok = window.confirm('로그인 시 자동 실행하려면 백그라운드 상주가 필요합니다. 백그라운드 상주도 함께 켤까요?');
      if (!ok) return;
      await run(() => api.setLaunchAtLogin(true, true), '로그인 시 자동 실행을 켰습니다(백그라운드 상주 포함)');
      return;
    }
    await run(() => api.setLaunchAtLogin(v), v ? 'OS에 자동 실행을 등록했습니다' : 'OS 자동 실행 등록을 해제했습니다');
  }

  async function toggleBackground(v: boolean) {
    if (!v && data.launchAtLogin && !window.confirm('백그라운드 상주를 끄면 로그인 시 자동 실행도 해제됩니다. 계속할까요?')) return;
    await run(() => api.setBackgroundMode(v), v ? '창을 닫아도 트레이에서 계속 실행됩니다' : '이제 창을 닫으면 AutoDesk가 완전히 종료됩니다');
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>설정</h1>
      </header>
      <Notice message={message} />

      <Card title="실행">
        <Row label="백그라운드 상주" hint="켜면 창을 닫아도 트레이(메뉴 막대)에 남아 자동화를 계속합니다. 끄면 창 닫기 = 앱 종료(모든 감시 해제).">
          <Toggle label="백그라운드 상주" checked={data.backgroundMode} onChange={toggleBackground} />
        </Row>
        <Row
          label="로그인 시 자동 실행"
          hint={data.launchAtLoginError ?? '현재 사용자 계정 로그인 때 트레이로 조용히 시작합니다. 표시는 OS의 실제 등록 상태입니다.'}
        >
          <Toggle label="로그인 시 자동 실행" checked={data.launchAtLogin} onChange={toggleLogin} />
        </Row>
        <Row label="전체 자동화" hint="끄면 모든 모듈의 감시·단축키를 해제합니다(설정·이력은 유지).">
          <Toggle label="전체 자동화" checked={data.masterEnabled} onChange={(v) => run(() => api.setMaster(v))} />
        </Row>
        <Row label="알림">
          <Toggle
            label="알림"
            checked={notifications}
            onChange={(v) => {
              setNotifications(v);
              run(() => api.setSetting('notifications_enabled', v));
            }}
          />
        </Row>
        <Row label="테마">
          <select className="input" value={theme} onChange={(e) => setTheme(e.target.value as Theme)}>
            <option value="system">시스템</option>
            <option value="light">라이트</option>
            <option value="dark">다크</option>
          </select>
        </Row>
        <button className="btn" onClick={() => api.quit()}>
          <Power size={16} aria-hidden /> 프로그램 종료
        </button>
      </Card>

      <Card title="저장 공간 · 개인정보">
        <Row label="앱 데이터 위치" hint="설정·DB·백업·로그가 모두 이 폴더 안에만 저장됩니다">
          <span className="mono small">{data.dataDir}</span>
        </Row>
        <ul className="muted small bullets">
          <li>감시 대상은 직접 추가한 폴더와 다운로드 폴더뿐입니다. 디스크 전체를 검색하지 않습니다.</li>
          <li>클립보드는 단축키를 누를 때만 읽고, 내용은 DB·로그에 저장하지 않습니다.</li>
          <li>외부 전송은 AutoPaste의 &lsquo;Claude Code로 다듬기&rsquo;를 직접 켠 경우에만, 단축키를 누른 순간에만 일어납니다(기본 꺼짐).</li>
          <li>관리자 권한, Windows 서비스, macOS LaunchDaemon을 사용하지 않습니다.</li>
        </ul>
      </Card>

      <Card title="정보">
        <Row label="버전">
          <span>{data.version} · {data.platform}</span>
        </Row>
        <Row label="라이선스" hint="MIT. 사용한 오픈소스는 모두 무료(MIT/Apache-2.0 등) — docs/LICENSES.md">
          <span>무료 · 개인용</span>
        </Row>
        <p className="muted small">자동 업데이트 서버는 없습니다. 새 버전은 GitHub Releases에서 받아 설치하세요.</p>
      </Card>

      <UninstallCard />
    </div>
  );
}

function UninstallCard() {
  const [preview, setPreview] = useState<UninstallPreview | null>(null);
  const [keepBackups, setKeepBackups] = useState(false);
  const [confirm, setConfirm] = useState('');
  const [done, setDone] = useState<string | null>(null);
  const { message, run, busy } = useAction();

  return (
    <Card title="AutoDesk 완전 제거">
      <p className="muted">
        자동화를 멈추고, 로그인 자동 실행 등록을 해제하고, AutoDesk가 만든 설정·DB·로그·캐시·백업을 지운 뒤 앱을 제거합니다. 원본 파일과 감시 폴더,
        다운로드 파일은 지우지 않습니다.
      </p>
      {!preview ? (
        <button className="btn danger" onClick={() => run(async () => setPreview(await api.uninstallPreview()))}>
          <Trash2 size={16} aria-hidden /> 제거 대상 확인
        </button>
      ) : done ? (
        <p className="notice ok">{done}</p>
      ) : (
        <>
          <table className="table kv">
            <tbody>
              {preview.items.map((i) => (
                <tr key={i.label + (i.path ?? '')}>
                  <td>{i.label}</td>
                  <td className="mono small">{i.path ?? ''}</td>
                  <td className="nowrap">{i.size ?? i.note ?? ''}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="small">삭제될 데이터 합계: <strong>{preview.totalSize}</strong> (그중 백업 {preview.backupsSize})</p>
          {preview.devBuild && <p className="notice warn">개발 빌드로 실행 중이라 앱 파일은 남기고 데이터만 정리합니다.</p>}
          <fieldset className="radio">
            <legend className="small">백업 파일 처리</legend>
            <label className="check">
              <input type="radio" name="keep" checked={!keepBackups} onChange={() => setKeepBackups(false)} /> 완전 제거: 백업·설정·이력·캐시 모두 삭제
            </label>
            <label className="check">
              <input type="radio" name="keep" checked={keepBackups} onChange={() => setKeepBackups(true)} /> 앱만 제거: 백업을 문서 폴더(AutoDesk-Backups-날짜)로 내보낸 뒤 삭제
            </label>
          </fieldset>
          <details className="small">
            <summary>보존되는 항목</summary>
            <ul className="bullets mono">
              {preview.preserved.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </details>
          <label className="field">
            <span>계속하려면 <strong>제거</strong> 라고 입력하세요</span>
            <input className="input" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
          </label>
          <div className="inline">
            <button
              className="btn danger"
              disabled={busy || confirm !== '제거'}
              onClick={async () => {
                const r = await run(() => api.uninstallExecute(keepBackups, confirm));
                if (r) setDone(r);
              }}
            >
              지금 완전 제거
            </button>
            <button className="btn" onClick={() => setPreview(null)}>취소</button>
          </div>
        </>
      )}
      <Notice message={message} />
    </Card>
  );
}
