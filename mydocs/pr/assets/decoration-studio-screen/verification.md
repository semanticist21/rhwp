# 5번 후보의 실제 Studio Canvas2D 화면 확인

- 범위: `e4b792845f51a21a4f655579a1fc652203fb1dc1`의 기존 production `dist/`를 재사용한 단일 표본 확인이다. 새 제품 빌드·전체 screen gate·Windows 검증 결과가 아니다.
- 2026-10-29 UTC, Fedora 호스트 CLI, Node 24.19.0, Chrome `156.0.8078.73`, headless Playwright, viewport 1280×1500, DPR 1, 배율 100%, `screen`, 요청/선택 backend `canvas2d`다.
- 원문: [top-probe-shape-5.hwp](../raised-decoration-hancom/top-probe-shape-5.hwp), SHA-256 `deade69da52406b917d05a766744df63c49dcd187e783963d64afbfdc926325e`, 1쪽이다. 원문 수정·재저장은 하지 않았다.
- 기존 [5번 Engine/Print 검증](../bottom-long-dash-hancom/verification.md)과 별도 범위다. Print consumer나 `toDataURL()`를 screen 결과로 쓰지 않았다.

## 실제 진입과 산출물

- loopback 임시 HTTP 서버가 준비된 `rhwp-studio/dist/` 파일만 제공했다. 개발 서버·소스 변환·새 빌드·사용자 데스크톱 조작은 하지 않았다. 새 browser context와 임시 포트를 쓰고 service worker 등록을 차단했다.
- `/?renderer=canvas2d&renderProfile=screen`으로 연 뒤 실제 `#file-input`에 원문 파일을 전달했다. 제품의 `loadFile → initializeDocument → CanvasView.loadDocument → PageRenderer`를 사용했으며 별도 capture canvas나 강제 `renderPage` 호출을 만들지 않았다.
- [전체 화면](canvas2d-screen.png)은 실제 Studio chrome과 문서 화면이다. [한 쪽 영역](canvas2d-page.png)은 같은 브라우저 화면의 page 사각형 `x=253.15625, y=150, w=793.6875, h=1122.5`를 캡처한 793×1122 PNG다. 확대·리사이즈하지 않았다.
- DOM의 `.document-page-canvas` backing은 794×1123, CSS는 793.6875×1122.5였다. `data-rhwp-rendered-zoom=1`, requested/effective DPR 1, `render-tier=screen`, surface cache key의 `request:canvas2d`, `backend:canvas2d`, `profile:screen`, revision 3을 확인했다.
- page parent의 실제 자식은 caret, composition caret, 본문 canvas뿐이며 이 입력에는 DOM 그림·앞뒤 별도 레이어가 없었다. 화면 캡처는 해당 영역의 DOM 합성을 포함하지만 그림·앞뒤 레이어의 존재/paint를 검증한 사례는 아니다.
- 제품의 문서 초기화 완료 로그·글꼴 준비 후 2초 대기하고 1초 간격으로 두 전체 화면을 캡처했다. 두 PNG SHA-256이 같았다. page error·실패한 network request·열린 dialog는 각각 0개였고 9~48pt 행의 실제 표시를 직접 열어 확인했다.

## 초기화 산출물과 글꼴 신원

- 초기 앱 요청의 JS 응답 `/assets/index-DePSEk_P.js`: SHA-256 `5f43daf3425c692ed4c456cd240efb5041614a2eb5ef29ce9a4b114857823cf5`.
- 초기 WASM 응답 `/assets/rhwp_bg-DCK6zzId.wasm`: HTTP 200, SHA-256 `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`. 기존 후보 producer와 같다. 새로 연 탭의 초기 요청을 CDP `Network.responseReceived/getResponseBody`로 읽었으며 별도 사후 fetch로 대신하지 않았다.
- `document.fonts.status=loaded`, 요청 family `함초롬바탕`의 loaded face와 제품 font loader의 실제 공급 성공을 확인했다. CSS face 등록만으로 실제 TTF가 Native와 같다고 가정하지 않는다.
- [HANBatang.woff](https://cdn.jsdelivr.net/gh/projectnoonnu/noonfonts_2104@1.0/HANBatang.woff): HTTP 200, SHA-256 `b3192018e3bd4da76b29b0530f71c79fb6f465a2b7cd41308bc38f9a4ae09a6c`.
- [HCRDotum.woff](https://cdn.jsdelivr.net/gh/projectnoonnu/noonfonts_four@1.0/HCRDotum.woff): HTTP 200, SHA-256 `49bf312aec309ba88c3eb612c591c2d8e28b765c156c808691850137431bb1bf`.
- 앱의 `fonts/RHWPOldHangul-Regular.woff2`: HTTP 200, SHA-256 `1a45a61e9a320c1bc62ca1401f7b23ceffffe8133dc2a9650ad4e12524f8dcab`. 다른 loaded face는 원문에 실제 쓰였다는 뜻이 아니다. font binary는 증적에 복제하지 않았다.

## 재현 경로와 남은 검증

- `renderer_baseline.py`는 내부에서 `npm run dev`를 시작한다. 그 호출은 사용하지 않았다. 기존 `e2e/helpers.mjs`의 `loadHwpFile`도 `/src/core/font-loader.ts` import와 DEV 전용 전역에 의존하므로 production 진입으로 대신하지 않았다.
- 동일한 준비된 production 산출물을 임시 loopback static 서버로 제공하고, 설치된 Playwright로 위 URL·viewport를 열어 파일 입력과 실제 page 영역 캡처를 수행하면 된다. 초기화 JS/WASM 응답 hash, DOM backend/revision/DPR와 font response hash를 위 값과 확인하고 image/font 준비·화면 안정화 실패는 성공 처리하지 않는다.
- 이번 확인은 Canvas2D 한 표본의 실제 production 화면 진입·표시·안정화 증거다. 한컴 Print와의 새 전쪽 screen TSV, Native와 CDN 글꼴의 outline 동일성, 확대/축소·DPR 전환·DOM 그림/앞뒤 레이어의 정상 대조군은 아직 확인하지 않았다.
- CanvasKit의 실제 Studio screen과 선택 surface·Typeface 준비, 실제 Windows UI, 저장·재열기·앱 인쇄도 미검증이다. 이 자료만으로 신규 Open PR 제출 조건이나 `features.md`의 완료 조건을 충족했다고 표시하지 않는다.
- 제품 코드·테스트·입력·공식 reference·threshold·feature 체크상태와 검토 PR #8349 및 기존 후보 head는 유지했다. 증적만 추가했으므로 새 Rust/WASM 전체 검사를 실행하지 않았다.
