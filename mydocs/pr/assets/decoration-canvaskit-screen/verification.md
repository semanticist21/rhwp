# 5번 후보의 실제 Studio CanvasKit 화면과 준비 상태

- 대상 제품 source는 `e4b792845f51a21a4f655579a1fc652203fb1dc1`이며 기존 production `dist/`를 재사용했다. 이 증적 branch의 부모 `fe67436e0c3fb7f0f2a5746f0aa52a475e939dad`는 같은 제품의 Canvas2D 증적만 추가한 커밋이다.
- 2026-10-29 UTC, Fedora 호스트 CLI, Node 24.19.0, Chrome `156.0.8078.73`, headless Playwright, viewport 1280×1500, DPR 1, 100%, `screen`이다. 새 제품 빌드나 Windows 실행 결과가 아니다.
- [원문 top-probe-shape-5.hwp](../raised-decoration-hancom/top-probe-shape-5.hwp), SHA-256 `deade69da52406b917d05a766744df63c49dcd187e783963d64afbfdc926325e`, 1쪽. 원문·공식 reference·threshold·제품 코드·테스트는 수정하지 않았다.

## 실제 제품 진입과 관측

- 준비된 `dist/`만 loopback static HTTP로 제공했다. 새 context·임시 포트로 열고 service worker를 차단했으며 개발 서버·소스 변환·사용자 데스크톱 조작은 하지 않았다.
- `/?renderer=canvaskit&canvaskitMode=default&canvaskitSurface=software&renderProfile=screen`과 실제 `#file-input`을 사용했다. 제품의 `loadFile → initializeDocument → CanvasView.loadDocument → PageRenderer`가 그렸다. 별도 renderer/canvas 생성·강제 `renderPage`·`toDataURL()`는 사용하지 않았다.
- DEV 전역은 production에서 노출되지 않는다. 실제 초기화에 쓰인 production chunk의 기존 `diagnostics` 메서드 한 번에 CDP function-call breakpoint를 걸어 호출 receiver를 읽고 즉시 제거·재개했다. 함수·prototype·반환값·제품 전역은 교체하지 않았다. 성능 측정 결과로 쓰지 않는다.
- 읽은 receiver의 `surfaceCache.target.canvas`가 현재 DOM의 `.document-page-canvas`와 같은 객체이고 연결돼 있음을 확인했다. 숨겨진 다른 renderer의 결과로 대신하지 않았다. 이후 이 receiver의 원래 getter와 준비된 자원을 읽기만 했다.
- DOM cache key의 요청/선택은 `canvaskit`, profile은 `screen`, revision 4, resources 4다. backing 794×1123, requested/effective DPR 1이며 실제 surface의 요청/선택은 `software`, fallback reason은 null이다.
- [전체 Studio 화면](canvaskit-screen.png), [브라우저의 실제 한 쪽 영역](canvaskit-page.png). 후자는 793×1122 PNG이며 CSS page 사각형을 그대로 캡처했다. 별도 확대·축소·raster 대체를 하지 않았다. 이 표본에는 DOM 그림·앞뒤 별도 레이어가 없다.
- 글꼴·제품 문서 초기화 완료 뒤 1초 간격의 두 전체 화면은 바이트가 같았다. 캡처 전후 모두 아래 준비 조건이 같고 9~48pt 행과 파란 밑줄의 실제 표시를 직접 열어 확인했다. caret과 편집 가이드도 실제 화면에 포함된다.

| 실제 renderer의 캡처 전후 상태 | 값 |
| --- | --- |
| `lastRenderCompleted`, `passesRuntimeReadinessGate` | true, true |
| `lastRenderError`, `readinessBlockers`, `lastUnsupportedOps` | null, 빈 배열, 빈 배열 |
| pending image / local Typeface, local / bundled Typeface 실패 | 모두 0 |
| 실제 bundled Typeface / 미등록 font fallback | 1 / 0 |
| `hiddenCanvas2dOverlayUsed`, image failure | false, 0 |
| page error / 실패한 network request / 열린 dialog | 모두 0 |

## 실제 글꼴·초기화 응답 신원

- `bundledTypefaces` 안의 실제 `HCR Batang` bytes는 11,613,312 bytes, SHA-256 `b3192018e3bd4da76b29b0530f71c79fb6f465a2b7cd41308bc38f9a4ae09a6c`다. [HANBatang.woff](https://cdn.jsdelivr.net/gh/projectnoonnu/noonfonts_2104@1.0/HANBatang.woff)의 실제 HTTP 200 응답 body hash와 같다. `함초롬바탕` alias와 실제 replay의 family cache가 같은 준비된 Typeface를 가리킨다. 글꼴 바이너리는 게시하지 않았다.
- 그 Typeface의 읽기 전용 `getGlyphIDs('0123456789TBH한')` 결과는 `[21,22,23,24,25,26,27,28,29,30,57,39,45,49970]`으로 모두 0보다 크다. 이 문자열의 coverage 확인이며 모든 문자·글꼴의 증거는 아니다.
- 이 입력의 실제 replay는 `TextRun`이며 `glyphRuns=0`, glyph-run blob/Typeface도 0이다. 이를 누락으로 간주하지 않았다. `drawTextRun → findPreparedTypeface/findBundledTypeface → 준비된 HCR Batang` 소비와 40개 text blob cache를 확인했다. CSS face 성공으로 CanvasKit Typeface 준비를 대신하지 않았다.
- 초기화 JS `/assets/index-DePSEk_P.js`: SHA-256 `5f43daf3425c692ed4c456cd240efb5041614a2eb5ef29ce9a4b114857823cf5`; 실제 CanvasKit chunk `/assets/canvaskit-renderer-TNJeUBBk.js`: `838ec13f4dd68e65a28743829b2ed0b5ba116dde7cb128b85d238c0ac1bfde45`.
- 초기 engine WASM `/assets/rhwp_bg-DCK6zzId.wasm`: HTTP 200, `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`. 기존 5번 producer와 같다. 초기 CanvasKit WASM `/assets/canvaskit-SCAxUyCS.wasm`: HTTP 200, `25ebed8e60158c5854f8dc807b936daca21354f8bfb6a2231266b0a93812f301`.
- WASM은 새 탭 초기화 요청의 CDP response body를 읽었다. 별도 사후 fetch로 대신하지 않았다. 중복 font 요청 한 건의 inspector body는 읽지 못했지만 같은 URL의 실제 응답과 renderer 안의 실제 bytes hash를 대조했다.

## 범위와 남은 검증

- [Canvas2D 증적](../decoration-studio-screen/verification.md), [기존 Engine/Print 증적](../bottom-long-dash-hancom/verification.md)과 구분한다. 준비 완료·backend 간 일치만으로 독립 한컴 일치나 픽셀 동일을 주장하지 않는다. 새 screen 전쪽 TSV·DOM 그림/앞뒤 레이어 대조군·zoom/DPR 전환·다른 surface는 아직 확인하지 않았다.
- 실행 중 latest devel은 `f125c735c7d2d44eaa44b78c70465bc89af45fb1`로 진전했다. #8360의 실제 source diff에서 backend 전환 시 surface 해제와 SVG glyph raster 변경을 확인했다. 이 증적은 그 통합본의 실행 검증이 아니며 기존 5번 후보를 자동 갱신하지 않았다.
- 실제 Windows UI·저장/재열기·앱 인쇄·다른 글꼴/회전/세로쓰기/겹침은 미검증이다. `features.md`와 기존 PR #8349/후보 head를 유지했고 별도 신규 Open PR 제출 조건의 완료로 표시하지 않는다. 증적 전용 추가이므로 Rust/WASM 전체 검사는 반복하지 않았다.
