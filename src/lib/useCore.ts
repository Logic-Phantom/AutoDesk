import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { api, errorMessage, type Overview } from './api';

/** Core 상태를 구독한다. Rust가 `core://changed`를 보낼 때마다 다시 읽는다(UI가 상태를 소유하지 않음). */
export function useOverview() {
  const [data, setData] = useState<Overview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);

  const refresh = useCallback(async () => {
    try {
      setData(await api.overview());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    const un = listen('core://changed', () => {
      // 짧은 시간에 여러 번 와도 한 번만 다시 읽는다.
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(refresh, 120);
    });
    return () => {
      un.then((f) => f());
      window.clearTimeout(timer.current);
    };
  }, [refresh]);

  return { data, error, refresh };
}

/** 명령 실행 + 오류 메시지 상태. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null);
  const run = useCallback(async <T,>(fn: () => Promise<T>, okText?: string | ((r: T) => string)) => {
    setBusy(true);
    setMessage(null);
    try {
      const r = await fn();
      if (okText) setMessage({ kind: 'ok', text: typeof okText === 'function' ? okText(r) : okText });
      return r;
    } catch (e) {
      setMessage({ kind: 'err', text: errorMessage(e) });
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, message, setMessage, run };
}
