// AutoDesk 사용 설명서(.docx) 생성 스크립트 — 실행: npm i docx@9 && node docs/guide/build-guide.cjs docs/guide/AutoDesk_사용설명서.docx
// 스크린샷(docs/guide/images)은 실제 설치본 화면. 목차/PDF는 Word에서 필드 업데이트 후 PDF로 내보내기.
const fs = require('fs');
const path = require('path');
const {
  Document, Packer, Paragraph, TextRun, HeadingLevel, AlignmentType, ImageRun, Table, TableRow, TableCell,
  WidthType, ShadingType, BorderStyle, LevelFormat, PageBreak, Footer, Header, PageNumber, TableOfContents,
} = require('docx');

const IMG = 'C:/eclipse_AI/AutoDesk/docs/guide/images';
const OUT = process.argv[2];
const FONT = 'Malgun Gothic';
const MONO = 'Consolas';
const ACCENT = '2563EB';
const CONTENT_W = 9026; // A4 폭 11906 - 여백 1440*2

// ---------- helpers ----------
const t = (text, o = {}) => new TextRun({ text, font: o.mono ? { ascii: MONO, hAnsi: MONO, eastAsia: FONT } : FONT, ...o });
function runs(s) {
  // **굵게**, `코드` 간단 마크업
  const parts = s.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean);
  return parts.map((p) => {
    if (p.startsWith('**')) return t(p.slice(2, -2), { bold: true });
    if (p.startsWith('`')) return t(p.slice(1, -1), { mono: true, size: 19, shading: { type: ShadingType.CLEAR, fill: 'F0F1F3', color: 'auto' } });
    return t(p);
  });
}
const P = (s, o = {}) => new Paragraph({ children: runs(s), spacing: { after: 120, line: 300 }, ...o });
const H1 = (s, pageBreak = true) => new Paragraph({ heading: HeadingLevel.HEADING_1, pageBreakBefore: pageBreak, children: [t(s)] });
const H2 = (s) => new Paragraph({ heading: HeadingLevel.HEADING_2, children: [t(s)] });
const H3 = (s) => new Paragraph({ heading: HeadingLevel.HEADING_3, children: [t(s)] });
const bullets = (items) => items.map((s) => new Paragraph({ numbering: { reference: 'bullets', level: 0 }, children: runs(s), spacing: { after: 60, line: 290 } }));
let stepCount = 0;
const numberingConfigs = [];
function steps(items) {
  const ref = `steps${++stepCount}`;
  numberingConfigs.push({
    reference: ref,
    levels: [{ level: 0, format: LevelFormat.DECIMAL, text: '%1.', alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 500, hanging: 360 } } } }],
  });
  return items.map((s) => new Paragraph({ numbering: { reference: ref, level: 0 }, children: runs(s), spacing: { after: 80, line: 290 } }));
}
function image(file, caption, w = 560) {
  const h = Math.round((w * 720) / 1080);
  return [
    new Paragraph({
      alignment: AlignmentType.CENTER,
      spacing: { before: 120, after: 60 },
      children: [new ImageRun({ outline: { type: 'solidFill', solidFillType: 'rgb', value: 'D9DCE1' }, type: 'png', data: fs.readFileSync(path.join(IMG, file)), transformation: { width: w, height: h },
        altText: { title: caption, description: caption, name: file } })],
    }),
    new Paragraph({ alignment: AlignmentType.CENTER, spacing: { after: 240 }, children: [t(caption, { size: 18, color: '646B75', italics: true })] }),
  ];
}
const border = { style: BorderStyle.SINGLE, size: 4, color: 'D9DCE1' };
const borders = { top: border, bottom: border, left: border, right: border };
function table(head, rows, widths) {
  const cell = (s, i, isHead) =>
    new TableCell({
      borders,
      width: { size: widths[i], type: WidthType.DXA },
      shading: isHead ? { fill: 'EEF2FB', type: ShadingType.CLEAR, color: 'auto' } : undefined,
      margins: { top: 70, bottom: 70, left: 110, right: 110 },
      children: [new Paragraph({ children: isHead ? [t(s, { bold: true, size: 19 })] : runs(s).map((r) => r), spacing: { line: 270 } })],
    });
  return new Table({
    width: { size: widths.reduce((a, b) => a + b, 0), type: WidthType.DXA },
    columnWidths: widths,
    rows: [new TableRow({ tableHeader: true, children: head.map((h, i) => cell(h, i, true)) }), ...rows.map((r) => new TableRow({ children: r.map((c, i) => cell(c, i, false)) }))],
  });
}
function callout(title, lines, fill = 'EEF4FF', bar = ACCENT) {
  return new Table({
    width: { size: CONTENT_W, type: WidthType.DXA },
    columnWidths: [CONTENT_W],
    rows: [
      new TableRow({
        children: [
          new TableCell({
            width: { size: CONTENT_W, type: WidthType.DXA },
            shading: { fill, type: ShadingType.CLEAR, color: 'auto' },
            borders: { top: { style: BorderStyle.NONE, size: 0, color: 'FFFFFF' }, bottom: { style: BorderStyle.NONE, size: 0, color: 'FFFFFF' }, right: { style: BorderStyle.NONE, size: 0, color: 'FFFFFF' }, left: { style: BorderStyle.SINGLE, size: 24, color: bar } },
            margins: { top: 100, bottom: 100, left: 180, right: 180 },
            children: [new Paragraph({ children: [t(title, { bold: true, color: bar })], spacing: { after: 60 } }), ...lines.map((l) => new Paragraph({ children: runs(l), spacing: { after: 40, line: 280 } }))],
          }),
        ],
      }),
    ],
  });
}
const gap = () => new Paragraph({ children: [], spacing: { after: 120 } });
const TIP = (lines) => [callout('팁', lines), gap()];
const WARN = (lines) => [callout('주의', lines, 'FDF3DC', 'A16207'), gap()];

// ---------- content ----------
const body = [];

// 표지
body.push(
  new Paragraph({ spacing: { before: 2600 }, children: [] }),
  new Paragraph({ alignment: AlignmentType.CENTER, children: [new ImageRun({ type: 'png', data: fs.readFileSync('C:/eclipse_AI/AutoDesk/src-tauri/icons/128x128.png'), transformation: { width: 96, height: 96 }, altText: { title: 'AutoDesk', description: 'AutoDesk 아이콘', name: 'icon' } })] }),
  new Paragraph({ alignment: AlignmentType.CENTER, spacing: { before: 240, after: 120 }, children: [t('AutoDesk 사용 설명서', { bold: true, size: 56 })] }),
  new Paragraph({ alignment: AlignmentType.CENTER, spacing: { after: 600 }, children: [t('내 PC에 상주하며 반복 작업을 조용히 처리하는 개인용 자동화 앱', { size: 24, color: '646B75' })] }),
  new Paragraph({ alignment: AlignmentType.CENTER, children: [t('버전 0.2.0 · 2026년 11월 19일', { size: 20, color: '646B75' })] }),
  new Paragraph({ alignment: AlignmentType.CENTER, children: [t('Windows 10/11 · macOS 11 이상', { size: 20, color: '646B75' })] }),
  new Paragraph({ alignment: AlignmentType.CENTER, spacing: { before: 1800 }, children: [t('github.com/Logic-Phantom/AutoDesk', { size: 18, color: ACCENT })] }),
);

// 목차
body.push(
  new Paragraph({ pageBreakBefore: true, children: [t('목차', { bold: true, size: 32 })], spacing: { after: 200 } }),
  new TableOfContents('목차', { hyperlink: true, headingStyleRange: '1-1' }),
);

// 1. 소개
body.push(H1('1. AutoDesk란?'));
body.push(P('AutoDesk는 평소처럼 PC를 쓰는 동안 **파일 저장, 다운로드, 텍스트 복사** 같은 일상 동작을 감지해 반복 작업을 대신 처리하는 앱입니다. 안전한 일은 조용히 끝내고, 파일을 옮기는 것처럼 확인이 필요한 일은 **승인 센터**에 올려 사용자가 결정하게 합니다.'));
body.push(table(['기능', '하는 일', '언제 동작하나'], [
  ['**AutoBackup Guardian**', '등록한 폴더의 파일 내용이 바뀌면 이전 버전을 보관합니다. 언제든 비교하고 되살릴 수 있습니다.', '파일을 저장할 때마다 자동'],
  ['**Download Butler**', '다운로드 폴더에 새 파일이 들어오면 PDF·오피스·이미지·영상 등 종류별 폴더로 정리합니다.', '새 파일이 들어올 때(기본은 승인 후 이동)'],
  ['**AutoPaste**', '복사한 텍스트의 줄 끝 공백, 보이지 않는 문자, 중복 빈 줄을 정리합니다.', '단축키 `Ctrl + Alt + Shift + V`를 누를 때만'],
  ['**LogWatcher**', '선택한 로그 파일에 새로 쌓이는 줄에서 ERROR·Exception을 찾아, 같은 오류는 묶어서 알립니다.', '로그에 줄이 추가될 때마다 자동'],
  ['Claude Code 다듬기(선택)', '복사한 글의 맞춤법·띄어쓰기를 Claude로 교정합니다.', '직접 켠 뒤 `Ctrl + Alt + Shift + B`를 누를 때만'],
], [2300, 4426, 2300]));
body.push(gap());
body.push(H2('꼭 알아둘 원칙'));
body.push(...bullets([
  '**완전 무료·개인용**입니다. 유료 서비스, 서버, 광고, 사용 통계 전송이 없고 인터넷 없이도 동작합니다.',
  '**원본을 지우거나 덮어쓰지 않습니다.** 파일 이동은 모두 되돌릴 수 있고, 복원도 직접 눌러야만 실행됩니다.',
  '**프로그램은 하나만** 돕니다(작업 관리자의 `AutoDesk.exe`). 관리자 권한, 윈도우 서비스, 숨은 백그라운드 프로그램을 설치하지 않습니다.',
  '전체·기능별·백그라운드·로그인 자동 실행을 **각각 켜고 끌 수 있고**, 앱 안의 **완전 제거**로 흔적 없이 지울 수 있습니다.',
]));

// 2. 설치
body.push(H1('2. 설치하기'));
body.push(H2('Windows'));
body.push(...steps([
  'GitHub 저장소의 **Releases** 또는 **Actions** 탭에서 `AutoDesk_<버전>_x64-setup.exe`를 내려받습니다.',
  '파일을 더블클릭합니다. 서명하지 않은 개인용 프로그램이라 **"Windows의 PC 보호"** 창이 뜨면 **추가 정보 → 실행**을 누릅니다.',
  '설치는 현재 사용자 계정에만 되며(`%LOCALAPPDATA%\\AutoDesk`) 관리자 권한을 묻지 않습니다.',
  '설치가 끝나면 시작 메뉴의 **AutoDesk**로 실행합니다. 작업 표시줄 오른쪽 트레이에 아이콘이 생깁니다.',
]));
body.push(...TIP(['이미 설치된 상태에서 새 버전 설치 파일을 실행하면, 실행 중인 AutoDesk를 자동으로 정상 종료한 뒤 업데이트합니다. 설정과 백업은 그대로 유지됩니다.']));
body.push(H2('macOS'));
body.push(...steps([
  '`AutoDesk_<버전>_aarch64.dmg`를 열어 **AutoDesk.app**을 응용 프로그램 폴더로 끌어 놓습니다.',
  '처음 실행 시 "확인되지 않은 개발자" 경고가 뜨면 Finder에서 앱을 **우클릭 → 열기**를 누릅니다.',
  '알림, 다운로드 폴더 접근, 단축키(손쉬운 사용) 권한을 물으면 허용합니다. 거부하면 해당 기능만 "설정 필요" 상태가 되고 나머지는 계속 동작합니다.',
]));
body.push(...WARN(['macOS 버전은 아직 실기기에서 검증하지 않았습니다(빌드만 확인). 문제가 있으면 GitHub 이슈로 남겨 주세요.']));

// 3. 화면 구성
body.push(H1('3. 화면 둘러보기'));
body.push(P('왼쪽 메뉴는 **대시보드 · 자동화 · 이력 · 승인 센터 · 설정** 다섯 개입니다. 승인 센터 옆 숫자는 확인을 기다리는 작업 수, 왼쪽 아래 점은 현재 상태(초록 = 실행 중)입니다.'));
body.push(...image('01-dashboard.png', '그림 1. 대시보드 — 전체 상태를 한눈에 확인'));
body.push(table(['영역', '의미'], [
  ['전체 자동화 스위치', '끄면 모든 감시와 단축키가 즉시 멈춥니다. 설정과 대기 작업은 그대로 남습니다.'],
  ['오늘 처리', '오늘 실제로 끝낸 작업 수(백업 저장, 파일 이동, 텍스트 정리 등). 누르면 이력으로 이동합니다.'],
  ['확인 대기', '승인 센터에서 결정을 기다리는 작업 수.'],
  ['오늘 오류', '오늘 실패한 작업 수. 0이 아니면 이력에서 원인을 확인하세요.'],
  ['백그라운드', '창을 닫아도 트레이에서 계속 도는지, 로그인 시 자동 실행이 켜져 있는지.'],
  ['모듈 상태 / 최근 처리', '기능별 상태(작동 중·꺼짐·설정 필요·오류로 정지)와 최근 작업 8건.'],
], [2500, 6526]));
body.push(gap());
body.push(H2('상태 표시 읽는 법'));
body.push(table(['표시', '뜻', '해야 할 일'], [
  ['작동 중(초록)', '정상적으로 감시·처리 중', '없음'],
  ['꺼짐(회색)', '사용자가 이 기능을 끔', '쓰려면 스위치를 켭니다'],
  ['전체 일시정지(노랑)', '전체 자동화가 꺼져 있음', '대시보드에서 전체 자동화를 켭니다'],
  ['설정 필요(노랑)', '감시할 폴더가 없거나 접근할 수 없음', '폴더를 추가하거나 권한을 확인합니다'],
  ['오류로 정지(빨강)', '같은 기능이 5번 연속 실패해 그 기능만 멈춤', '이력에서 원인을 보고, 스위치를 껐다 켜면 다시 시작'],
], [2300, 3600, 3126]));

// 4. 켜고 끄기
body.push(H1('4. 켜고 끄기 — 원하는 만큼만 돌리기'));
body.push(P('AutoDesk의 스위치는 네 종류이고 서로 독립적입니다. 화면 표시만 바뀌는 가짜 스위치는 없습니다. 끄는 순간 실제 폴더 감시와 단축키 등록이 해제됩니다.'));
body.push(table(['스위치', '어디서', '켜면', '끄면'], [
  ['전체 자동화', '대시보드, 설정, 트레이', '켜진 기능들이 동작', '모든 감시·단축키 해제(앱은 떠 있음)'],
  ['기능별(모듈)', '자동화 화면, 트레이', '그 기능만 동작', '그 기능만 멈추고 나머지는 계속'],
  ['백그라운드 상주', '설정', '창을 닫아도 트레이에 남아 계속 동작', '창을 닫으면 프로그램이 완전히 종료'],
  ['로그인 시 자동 실행', '설정', 'PC 로그인 때 창 없이 트레이로 시작', 'OS 자동 실행 등록을 즉시 해제'],
], [2000, 2000, 2513, 2513]));
body.push(gap());
body.push(...image('02-automations.png', '그림 2. 자동화 화면 — 기능마다 오른쪽 스위치로 켜고 끔'));
body.push(H2('트레이(작업 표시줄) 메뉴'));
body.push(P('작업 표시줄 오른쪽의 AutoDesk 아이콘을 **왼쪽 클릭**하면 창이 열리고, **오른쪽 클릭**하면 메뉴가 나옵니다.'));
body.push(...bullets([
  '**AutoDesk 열기** — 메인 창 표시',
  '**전체 자동화** (체크) — 바로 일시정지/재개',
  '**AutoBackup Guardian / Download Butler / AutoPaste** (체크) — 기능별 빠른 켜고 끄기',
  '**확인 대기 N건** — 승인 센터 열기',
  '**최근 처리 내역 · 설정** — 해당 화면 열기',
  '**프로그램 종료** — 진행 중인 작업을 마무리하고 완전히 종료(백그라운드에 아무것도 남지 않음)',
]));
body.push(H2('상황별로 이렇게 하세요'));
body.push(table(['하고 싶은 것', '방법'], [
  ['회의 중 잠깐 멈추기', '트레이 → **전체 자동화** 체크 해제. 끝나면 다시 체크'],
  ['다운로드 정리만 끄기', '트레이 → **Download Butler** 체크 해제'],
  ['오늘은 완전히 끄기', '트레이 → **프로그램 종료**'],
  ['부팅할 때 안 뜨게', '설정 → **로그인 시 자동 실행** 끄기'],
  ['창 닫으면 그냥 꺼지게', '설정 → **백그라운드 상주** 끄기'],
], [3000, 6026]));
body.push(gap());
body.push(...TIP(['로그인 시 자동 실행은 백그라운드 상주가 켜져 있어야 의미가 있습니다. 백그라운드가 꺼진 상태에서 자동 실행을 켜면 "백그라운드도 함께 켤까요?"라고 묻습니다. 반대로 백그라운드를 끄면 자동 실행도 함께 해제됩니다.']));

// 5. AutoBackup
body.push(H1('5. AutoBackup Guardian — 파일 버전 자동 보관'));
body.push(P('실수로 내용을 지우고 저장했거나, 어제 버전이 더 나았을 때 되돌릴 수 있게 해 줍니다. 파일 내용이 **실제로 바뀐 경우에만** 새 버전을 만들기 때문에 같은 내용으로 여러 번 저장해도 버전이 늘지 않습니다.'));
body.push(H2('폴더 등록하기'));
body.push(...steps([
  '**자동화** 화면에서 **AutoBackup Guardian** 제목을 눌러 펼칩니다.',
  '하위 폴더까지 백업하려면 **하위 폴더 포함**을 체크한 상태로 **폴더 추가**를 누릅니다.',
  '보관할 폴더(예: 업무 문서, 프로젝트 폴더)를 고릅니다. 상태가 **작동 중**으로 바뀌면 준비 끝입니다.',
  '이제 그 폴더의 파일을 평소처럼 저장하면 이력에 "버전 저장"이 기록됩니다.',
]));
body.push(...image('03-backup-settings.png', '그림 3. AutoBackup 설정 — 감시 폴더와 보관 정책'));
body.push(H2('보관 정책'));
body.push(table(['항목', '기본값', '설명'], [
  ['파일당 보관 버전 수', '10개', '넘으면 가장 오래된 버전부터 정리'],
  ['최대 파일 크기', '50 MB', '이보다 큰 파일은 건너뜀(이력에 "크기 제한 초과" 표시)'],
  ['전체 백업 용량 제한', '2048 MB', '넘으면 오래된 버전부터 정리. 파일마다 최신 버전 1개는 항상 남김'],
  ['백업할 확장자', '문서·코드 위주', 'txt, md, docx, xlsx, pptx, hwp, js, java, py 등. 비우면 모든 파일'],
  ['제외 패턴', '임시 파일 등', '`~$*`, `*.tmp`, `node_modules/**, `.git/** 등은 백업하지 않음'],
], [2500, 1600, 4926]));
body.push(gap());
body.push(...WARN(['드라이브 전체(C:\\)나 Windows·Program Files 같은 시스템 폴더, AutoDesk 데이터 폴더는 등록할 수 없습니다. 꼭 필요한 작업 폴더만 등록하세요.']));
body.push(H2('버전 비교하고 되살리기'));
body.push(...steps([
  'AutoBackup 패널 아래 **버전 기록 · 복원**에서 파일 이름으로 검색하거나 목록에서 파일을 고릅니다.',
  '버전 목록에서 **비교**를 누르면 그 버전과 현재 파일의 차이가 나옵니다(빨강 = 빠진 줄, 초록 = 추가된 줄).',
  '되살리는 방법은 두 가지입니다.',
]));
body.push(table(['버튼', '동작', '언제 쓰나'], [
  ['**사본으로 복원**', '원본은 그대로 두고 `파일명 (복원 1119-1430).확장자` 사본을 옆에 만듦', '내용을 비교하며 일부만 가져오고 싶을 때(가장 안전)'],
  ['**덮어쓰기 복원**', '확인 후 현재 파일을 그 버전으로 바꿈. 바꾸기 직전 상태를 먼저 백업', '파일을 통째로 예전으로 되돌릴 때'],
], [2200, 4326, 2500]));
body.push(gap());
body.push(...image('04-backup-versions.png', '그림 4. 버전 기록 — 파일 선택 → 버전별 비교·복원'));
body.push(...TIP(['백업은 PC 안(앱 데이터 폴더)에만 저장됩니다. 디스크 고장에 대비한 외부 백업을 대신하지는 않으니, 중요한 자료는 클라우드나 외장 디스크에도 따로 보관하세요.']));

// 6. Download Butler
body.push(H1('6. Download Butler — 다운로드 폴더 자동 정리'));
body.push(P('다운로드 폴더에 새 파일이 생기면 종류를 판별해 하위 폴더로 정리합니다. 처음에는 **미리보기 모드**라서 파일을 바로 옮기지 않고 **승인 센터**에 "이렇게 옮길까요?"라는 제안만 올립니다.'));
body.push(table(['파일 종류', '옮겨지는 폴더'], [
  ['PDF', '`Downloads\\Documents\\PDF`'],
  ['Word·Excel·PowerPoint·한글·CSV', '`Downloads\\Documents\\Office`'],
  ['txt·md·json·log 등', '`Downloads\\Documents\\Text`'],
  ['png·jpg·gif·webp 등', '`Downloads\\Images`'],
  ['mp4·mov·mkv 등 / mp3·wav·m4a 등', '`Downloads\\Videos` / `Downloads\\Audio`'],
  ['zip·7z·rar 등', '`Downloads\\Archives`'],
  ['exe·msi·dmg 등 설치 파일', '`Downloads\\Installers` (실행하지 않음)'],
], [4000, 5026]));
body.push(gap());
body.push(...image('05-downloads.png', '그림 5. Download Butler 설정 — 자동 이동, 감시 폴더, 제외 확장자'));
body.push(H2('설정 항목'));
body.push(...bullets([
  '**자동 이동(AUTO)** — 켜면 확실히 분류되는 파일은 승인 없이 바로 옮깁니다. 확장자와 실제 내용이 다른 파일(예: 이름은 .pdf인데 실제로는 실행 파일)은 자동 모드에서도 승인을 요청합니다.',
  '**기본 다운로드 폴더 감시** — 윈도우의 기본 다운로드 폴더를 감시할지 정합니다. **폴더 추가**로 다른 폴더도 등록할 수 있습니다.',
  '**지금 있는 파일 정리 제안 받기** — 이미 쌓여 있는 파일에 대한 이동 제안을 한 번에 승인 센터에 올립니다(자동으로 옮기지 않음).',
  '**제외할 확장자** — 예: `torrent, iso`. 해당 파일은 건드리지 않습니다.',
  '다운로드 중인 파일(`.crdownload`, `.part` 등)과 숨김 파일은 무시하고, 크기가 더 이상 변하지 않을 때 처리합니다.',
]));
body.push(H2('승인 센터에서 결정하기'));
body.push(...steps([
  '왼쪽 메뉴 **승인 센터**를 엽니다(트레이의 "확인 대기 N건"으로도 열 수 있음).',
  '파일마다 제안된 폴더가 표시됩니다. 다른 곳이 좋으면 드롭다운에서 폴더를 바꿉니다.',
  '**이동**을 누르면 옮기고, **그대로 두기**를 누르면 파일을 건드리지 않고 제안만 지웁니다.',
  '같은 이름의 파일이 이미 있으면 덮어쓰지 않고 `파일명 (2).pdf`처럼 새 이름으로 옮깁니다.',
]));
body.push(...image('08-approvals.png', '그림 6. 승인 센터 — "분류 불확실" 항목은 특히 확인'));
body.push(H2('잘못 옮겼을 때 되돌리기'));
body.push(P('**이력** 화면에서 이동 기록 오른쪽의 **되돌리기**를 누르면 파일이 원래 위치로 돌아갑니다. 원래 위치에 같은 이름이 생겼다면 새 이름으로 돌려놓습니다.'));

// 7. AutoPaste
body.push(H1('7. AutoPaste — 복사한 텍스트 깔끔하게'));
body.push(P('웹 페이지나 PDF, 메신저에서 복사한 글에는 줄 끝 공백, 줄바꿈 없는 공백(NBSP), 눈에 보이지 않는 문자, 빈 줄이 잔뜩 섞여 있곤 합니다. AutoPaste는 단축키 한 번으로 이를 정리합니다.'));
body.push(H2('사용 방법'));
body.push(...steps([
  '평소처럼 텍스트를 복사합니다(`Ctrl + C`).',
  '**Ctrl + Alt + Shift + V**를 누릅니다(macOS는 `⌘ + ⌥ + ⇧ + V`). 정리되면 "N곳 정리" 알림이 뜹니다.',
  '평소처럼 붙여넣습니다(`Ctrl + V`). 정리된 텍스트가 들어갑니다.',
]));
body.push(...image('06-autopaste.png', '그림 7. AutoPaste — 단축키와 정리 규칙'));
body.push(H2('정리 규칙'));
body.push(table(['규칙', '기본', '설명'], [
  ['줄 끝 공백 제거', '켜짐', '각 줄 끝의 공백·탭 삭제'],
  ['보이지 않는 공백 제거', '켜짐', 'U+200B, BOM 삭제. 이모지를 이어 주는 문자(ZWJ)는 보존'],
  ['NBSP → 일반 공백', '켜짐', '웹에서 자주 섞이는 줄바꿈 없는 공백을 일반 공백으로'],
  ['연속된 빈 줄 줄이기', '켜짐(최대 1줄)', '빈 줄이 여러 개면 지정한 개수만 남김(0~5)'],
  ['맨 앞뒤 빈 줄·공백 제거', '켜짐', '텍스트 처음과 끝의 군더더기 삭제'],
  ['탭을 공백으로', '꺼짐', '탭을 공백 4칸으로'],
  ['HTML 태그 제거', '꺼짐', '`<p>` 같은 태그가 글자로 복사됐을 때 제거'],
  ['코드/로그 보호', '켜짐', '코드나 로그로 보이면 들여쓰기와 공백을 건드리지 않음'],
  ['줄바꿈 형식', '그대로', 'LF/CRLF로 통일 가능'],
], [2700, 1700, 4626]));
body.push(gap());
body.push(...TIP([
  '단축키가 다른 프로그램과 겹치면 단축키 칸에 새 조합을 입력하고 **적용**을 누르세요. 형식 예: `CommandOrControl+Alt+Shift+KeyV`, `CommandOrControl+Shift+KeyL`.',
  '등록에 실패하면 모듈 상태가 "오류로 정지"로 바뀌고 이유가 표시됩니다(성공한 것처럼 보이지 않음).',
]));
body.push(H2('선택 기능: Claude Code로 맞춤법 다듬기'));
body.push(P('PC에 설치된 **Claude Code**를 이용해 복사한 글의 맞춤법·띄어쓰기·오타를 고칩니다. 이미 쓰고 있는 Claude 구독 플랜 안에서 동작하므로 API 키나 추가 요금이 없고, 한도를 넘으면 실패로 표시될 뿐 유료로 전환되지 않습니다.'));
body.push(...steps([
  'Claude Code CLI를 설치하고 터미널에서 `claude`를 한 번 실행해 로그인합니다.',
  'AutoDesk를 다시 열면 AutoPaste 패널에 "Claude Code: 사용 가능"이 표시됩니다.',
  '**외부 전송 동의**를 켠 뒤 **AI 다듬기 단축키 사용**을 켭니다. 모델은 기본 `haiku`(가장 빠르고 사용량이 적음)를 권장합니다.',
  '글을 복사하고 **Ctrl + Alt + Shift + B**를 누릅니다. "다듬는 중…" 알림 후 결과가 클립보드에 들어가면 붙여넣습니다.',
]));
body.push(...image('07-autopaste-ai.png', '그림 8. AI 다듬기 — 동의를 켜야 단축키를 켤 수 있음'));
body.push(...WARN([
  'AI 다듬기 단축키를 누르는 순간의 클립보드 텍스트가 Anthropic(Claude)으로 전송됩니다. 비밀번호, 개인정보, 회사 기밀이 담긴 글에는 쓰지 마세요.',
  '원문과 결과는 AutoDesk 로그·DB에 저장되지 않습니다. 결과가 비었거나 지나치게 길면 적용하지 않고 원문을 유지합니다.',
]));

// 8. LogWatcher
body.push(H1('8. LogWatcher — 로그 오류 감시'));
body.push(P('서버나 개발 도구가 남기는 로그 파일을 등록해 두면, 새로 쌓이는 줄에서 **ERROR·Exception·SEVERE** 같은 문구를 찾아 알려 줍니다. 로그는 PC 안에서만 분석하고 밖으로 보내지 않습니다.'));
body.push(H2('로그 파일 등록하기'));
body.push(...steps([
  '**자동화** 화면에서 **LogWatcher**를 펼치고 **로그 파일 추가**를 누릅니다.',
  '감시할 파일(예: `catalina.out`, `server.log`, `JeusServer.log`)을 고릅니다. 폴더가 아니라 **파일**을 고릅니다.',
  '등록한 **뒤에** 쌓이는 줄만 검사합니다. 이미 있던 내용은 오류로 세지 않습니다.',
  '서버가 사용하는 로그 종류에 맞춰 **패턴 묶음**(일반 · Tomcat · JEUS/WebtoB)을 누르면 감지 문구가 한 번에 바뀝니다.',
]));
body.push(...image('13-logwatch.png', '그림 9. LogWatcher — 로그 파일과 감지 규칙'));
body.push(H2('어떻게 감지하나'));
body.push(table(['상황', '동작'], [
  ['ERROR·FATAL·SEVERE·Exception', '오류로 기록하고 알림'],
  ['WARN·WARNING', '경고로 기록(알림은 "경고도 알림"을 켰을 때만)'],
  ['같은 오류가 반복', '시간·숫자·ID만 다른 같은 오류는 하나로 묶어 **횟수**만 늘림. 다시 알림은 설정한 간격(기본 30분)이 지난 뒤에만'],
  ['스택 트레이스', '`at …`, `Caused by:` 줄은 따로 세지 않고 앞 오류의 **문맥**으로 붙여 보관'],
  ['줄이 아직 덜 써짐', '줄바꿈이 올 때까지 기다렸다가 읽음'],
  ['로그 파일이 새로 시작(로테이션)', '파일이 작아진 것을 감지해 처음부터 다시 읽음'],
  ['한글 로그', 'UTF-8과 EUC-KR(국내 WAS 기본) 모두 읽음'],
  ['LogWatcher를 껐다 켬', '꺼져 있던 동안 쌓인 줄도 이어서 검사(놓치지 않음)'],
], [3000, 6026]));
body.push(gap());
body.push(H2('감지된 오류 확인하기'));
body.push(P('패널 아래 **감지된 오류** 목록에 종류별로 횟수, 처음·최근 발생 시각, 파일 이름이 나옵니다. 항목을 누르면 오류 앞뒤 줄과 스택 트레이스가 펼쳐집니다(▶ 표시가 오류 줄). 확인이 끝나면 **확인함 — 목록 비우기**를 누르세요. 새 알림은 이력 화면에도 남습니다.'));
body.push(...image('14-logwatch-issues.png', '그림 10. 감지된 오류 — 같은 오류는 횟수로 묶이고 스택 트레이스를 함께 보관'));
body.push(...TIP(['오탐이 많으면 **대소문자 구분**을 켠 상태로 두고, 오류 문구를 서버 로그 형식에 맞게 좁히세요(예: `[ERROR]`). 문구는 쉼표로 구분해 입력하고 **패턴 저장**을 누릅니다.']))

// 9. 이력
body.push(H1('9. 이력 — 무엇을 했는지 확인'));
body.push(P('AutoDesk가 실제로 처리한 일만 기록됩니다. 결과(완료·실패·승인 대기·취소·건너뜀)와 기능별로 걸러 볼 수 있고, 파일 이동은 여기서 **되돌리기**할 수 있습니다. 클립보드 내용은 기록하지 않고 "3곳 정리"처럼 건수만 남깁니다.'));
body.push(...image('09-activity.png', '그림 11. 이력 — 결과·모듈 필터와 되돌리기 버튼'));
body.push(...TIP(['실패 항목 옆 괄호 안의 코드(예: `PERMISSION_DENIED`)가 원인입니다. 대부분 다른 프로그램이 파일을 쓰고 있거나 폴더 권한이 없는 경우로, 잠시 후 다시 저장하면 처리됩니다.']));

// 9. 설정
body.push(H1('10. 설정'));
body.push(...image('10-settings.png', '그림 12. 설정 — 실행 방식, 알림, 테마'));
body.push(table(['항목', '설명'], [
  ['백그라운드 상주', '켜면 창을 닫아도 트레이에서 계속 실행. 처음 창을 닫을 때 "계속 실행 중" 알림이 한 번 뜹니다.'],
  ['로그인 시 자동 실행', 'PC에 로그인하면 창 없이 트레이로 시작. 표시는 항상 윈도우의 실제 등록 상태입니다.'],
  ['전체 자동화', '대시보드의 스위치와 같습니다.'],
  ['알림', '백업 실패, 정리 결과 등 OS 알림 표시 여부.'],
  ['테마', '시스템 / 라이트 / 다크.'],
  ['프로그램 종료', '모든 자동화를 정리하고 완전히 종료.'],
  ['앱 데이터 위치', '설정·DB·백업·로그가 저장되는 단 하나의 폴더.'],
], [2600, 6426]));
body.push(gap());
body.push(...image('12-dashboard-dark.png', '그림 13. 다크 테마'));

// 10. 제거
body.push(H1('11. 완전히 제거하기'));
body.push(P('AutoDesk가 PC에 남기는 것은 **로그인 자동 실행 등록 1개**와 **앱 데이터 폴더**뿐이고, 둘 다 깨끗하게 지울 수 있습니다.'));
body.push(H2('방법 1 (권장): 앱 안에서 완전 제거'));
body.push(...steps([
  '**설정** 맨 아래 **AutoDesk 완전 제거**에서 **제거 대상 확인**을 누릅니다.',
  '지워질 항목과 용량을 확인합니다. 백업을 남기고 싶으면 **앱만 제거**를 고릅니다(문서 폴더에 `AutoDesk-Backups-날짜`로 내보냄).',
  '입력 칸에 **제거**라고 입력하고 **지금 완전 제거**를 누릅니다.',
  'AutoDesk가 종료되고 잠시 뒤 결과 보고서(메모장)가 열립니다. 실패한 항목이 있으면 보고서에 표시됩니다.',
]));
body.push(...image('11-uninstall.png', '그림 14. 완전 제거 — 삭제 대상과 용량을 확인한 뒤 실행'));
body.push(table(['지우는 것', '지우지 않는 것'], [
  ['실행 중인 AutoDesk와 트레이 아이콘', '감시 폴더 안의 원본 파일'],
  ['로그인 자동 실행 등록', '다운로드 폴더와 분류된 파일'],
  ['설정·DB·이력·로그·캐시·(선택) 백업', 'WebView2 등 다른 프로그램과 함께 쓰는 구성 요소'],
  ['설치된 프로그램 파일·시작 메뉴', '다른 프로그램의 설정·데이터'],
], [4513, 4513]));
body.push(gap());
body.push(H2('방법 2: 윈도우 설정 → 앱에서 제거'));
body.push(P('**설정 → 앱 → 설치된 앱 → AutoDesk → 제거**로도 지울 수 있습니다. 실행 중인 AutoDesk를 정상 종료하고 자동 실행 등록과 프로그램 파일을 지웁니다. 설정·백업까지 지우려면 제거 화면의 **"앱 데이터 삭제"**에 체크하세요.'));
body.push(...WARN(['macOS에서 앱을 Finder로 휴지통에 버리기만 하면 자동 실행 등록과 데이터가 남습니다. macOS에서는 반드시 **앱 안의 완전 제거**를 사용하세요.']));

// 11. FAQ
body.push(H1('12. 자주 묻는 질문'));
const faq = [
  ['백그라운드에서 몰래 계속 도는 건 아닌가요?', '프로세스는 `AutoDesk.exe` 하나뿐입니다. 트레이의 "프로그램 종료"나 백그라운드 상주를 끈 뒤 창을 닫으면 완전히 꺼지고, 작업 관리자에도 남지 않습니다. 서비스나 예약 작업을 설치하지 않습니다.'],
  ['PC가 느려지지 않나요?', '파일 변경을 OS 이벤트로만 받아 처리하므로 평소엔 거의 쉬고 있습니다(측정 시 메모리 약 34MB). 큰 파일은 크기 제한으로 건너뜁니다.'],
  ['파일을 저장했는데 버전이 안 생겨요.', '① 전체 자동화와 AutoBackup이 "작동 중"인지 ② 그 확장자가 백업 대상인지 ③ 제외 패턴에 걸리지 않는지 ④ 내용이 실제로 바뀌었는지 확인하세요. 같은 내용 재저장은 버전을 만들지 않습니다.'],
  ['다운로드한 파일이 그대로 있어요.', '기본은 미리보기 모드라 승인 센터에서 "이동"을 눌러야 옮겨집니다. 바로 옮기려면 자동 이동(AUTO)을 켜세요.'],
  ['단축키를 눌러도 반응이 없어요.', '다른 프로그램이 같은 단축키를 쓰는지 확인하고 AutoPaste 패널에서 단축키를 바꾸세요. 이미지나 파일을 복사한 경우는 건드리지 않습니다.'],
  ['"오류로 정지"가 떴어요.', '같은 기능이 5번 연속 실패하면 그 기능만 멈춥니다. 이력에서 오류 코드를 확인하고 원인을 해결한 뒤 스위치를 껐다 켜세요.'],
  ['로그 알림이 너무 많아요.', 'LogWatcher의 "같은 오류 다시 알림 간격"을 늘리거나, 오류 문구를 서버 형식에 맞게 좁히세요(예: `ERROR` 대신 `[ERROR]`). 경고 알림은 기본으로 꺼져 있습니다.'],
  ['인터넷이 끊겨도 되나요?', '네. AI 다듬기를 제외한 모든 기능이 오프라인에서 동작합니다.'],
  ['데이터는 어디에 저장되나요?', '`%APPDATA%\\io.github.logicphantom.autodesk` 한 곳입니다(설정 → 앱 데이터 위치). 백업, DB, 로그가 모두 여기에 있습니다.'],
];
body.push(table(['질문', '답'], faq, [3000, 6026]));
body.push(gap());
body.push(H2('이 설명서의 화면'));
body.push(P('이 설명서의 스크린샷은 Windows 11에서 실제 설치본(v0.2.0)을 예제 폴더(`C:\\AutoDesk-Demo`)로 실행해 찍었습니다. 사용자 이름은 `user`로 가렸습니다.'));

// ---------- document ----------
const doc = new Document({
  creator: 'Logic-Phantom',
  lastModifiedBy: 'Logic-Phantom',
  title: 'AutoDesk 사용 설명서',
  description: 'AutoDesk v0.2.0 사용 설명서',
  features: { updateFields: true },
  styles: {
    default: { document: { run: { font: FONT, size: 21 } } },
    paragraphStyles: [
      { id: 'Heading1', name: 'Heading 1', basedOn: 'Normal', next: 'Normal', quickFormat: true,
        run: { font: FONT, size: 34, bold: true, color: '1F2328' },
        paragraph: { spacing: { before: 0, after: 200 }, keepNext: true, outlineLevel: 0,
          border: { bottom: { style: BorderStyle.SINGLE, size: 8, color: ACCENT, space: 6 } } } },
      { id: 'Heading2', name: 'Heading 2', basedOn: 'Normal', next: 'Normal', quickFormat: true,
        run: { font: FONT, size: 26, bold: true, color: ACCENT },
        paragraph: { spacing: { before: 280, after: 120 }, keepNext: true, outlineLevel: 1 } },
      { id: 'Heading3', name: 'Heading 3', basedOn: 'Normal', next: 'Normal', quickFormat: true,
        run: { font: FONT, size: 22, bold: true },
        paragraph: { spacing: { before: 200, after: 80 }, keepNext: true, outlineLevel: 2 } },
    ],
  },
  numbering: {
    config: [
      { reference: 'bullets', levels: [{ level: 0, format: LevelFormat.BULLET, text: '•', alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 500, hanging: 300 } } } }] },
      ...numberingConfigs,
    ],
  },
  sections: [{
    properties: { page: { size: { width: 11906, height: 16838 }, margin: { top: 1300, bottom: 1300, left: 1440, right: 1440 } }, titlePage: true },
    headers: { default: new Header({ children: [new Paragraph({ alignment: AlignmentType.RIGHT, children: [t('AutoDesk 사용 설명서', { size: 16, color: '9AA1AB' })] })] }) },
    footers: { default: new Footer({ children: [new Paragraph({ alignment: AlignmentType.CENTER, children: [new TextRun({ children: [PageNumber.CURRENT], font: FONT, size: 18, color: '9AA1AB' })] })] }) },
    children: body,
  }],
});

Packer.toBuffer(doc).then((buf) => {
  fs.writeFileSync(OUT, buf);
  console.log('wrote', OUT, buf.length);
});
