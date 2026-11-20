import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { Dock } from './windows/Dock';
import { Editor } from './windows/Editor';
import { Overlay } from './windows/Overlay';
import './styles.css';
import './tools.css';

// 같은 번들을 여러 창이 공유한다: 메인 / 플로팅 독 / 영역 선택 / 캡처 편집기
const route = window.location.hash.replace(/^#\/?/, '');
const Root = route === 'dock' ? Dock : route === 'overlay' ? Overlay : route === 'editor' ? Editor : App;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
