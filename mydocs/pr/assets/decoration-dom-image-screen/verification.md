# 실제 Studio DOM 그림 합성 대조군 · Canvas2D

- 제품 source `e4b792845f51a21a4f655579a1fc652203fb1dc1`의 준비된 production `dist/`를 재사용했다. 이 증적 branch의 부모 `2469e6f6306ea0db9a0101de0df16d1c70d725fd`는 같은 제품에 기존 screen 증적만 추가한 커밋이다. 제품 코드·테스트·원문·reference·threshold는 바꾸지 않았다.
- 2026-10-29 UTC, Fedora 호스트 CLI, Node 24.19.0, Chrome `156.0.8078.73`, headless Playwright, viewport 1280×1500, DPR 1, 100%, `screen`, 요청/선택 backend `canvas2d`다. Windows 검증이나 새 제품 빌드 결과가 아니다.
- 기존 `renderer_baseline_manifest.json`의 대표 입력 `image-crop`, page index 0, [pic-crop-01.hwp](../../../../samples/pic-crop-01.hwp)를 썼다. SHA-256 `3b5afbe4cdb18faa49452a9cbed770d5caa27aed2f83b708e9ab8516c65ebf4c`, 전체 1쪽이다.
- 독립 기준은 [image-crop-2020.pdf](../../../../samples/issue7688/reference/image-crop-2020.pdf), SHA-256 `c9d29de7680e5642a24818a6924ef5f42b18e1194766b693d636853d8782d16d`다. Hwp 2020 / Hancom PDF 1.3.0.550의 전체 1쪽이며 [#7688의 원문·독립 Print 연결](../issue_7688_renderer_backends/README.md)과 원 해결 PR·메인터너 검증 기록을 먼저 읽었다. 새 Print를 만들거나 정상 기준을 교체하지 않았다.

## 실제 진입·소유·자원 준비

- loopback static 서버가 준비된 `dist/`만 제공했다. 새 context·임시 포트·service worker 차단을 사용했고 개발 서버·소스 변환·사용자 데스크톱·한컴 환경 준비를 실행하지 않았다.
- `/?renderer=canvas2d&renderProfile=screen`에서 실제 `#file-input`으로 열었다. 제품의 `loadFile → initializeDocument → CanvasView.loadDocument → PageRenderer`가 구성한 DOM을 사용했다. 별도 capture canvas·강제 `renderPage`·flow canvas `toDataURL()`로 전체 화면을 대신하지 않았다.
- `PageRenderer`의 flow 분리와 `usesDomFlowImages → createDomFlowImageLayer → applyFlowImageCrop` 호출을 먼저 추적했다. 실제 DOM은 page 0의 `flow-images-0` DIV와 본문 `.document-page-canvas`가 같은 page parent에 있는 순서이며, `data-rhwp-surface-layer-count=2`, cache key는 `flow-images-0:dom,main:794x1123`이다.
- DOM 그림 2개는 모두 page 0 소유 layer에 연결돼 있고 decode 완료·natural size 양수였다. 두 element의 실제 blob bytes는 각각 47,749 bytes, SHA-256 `12c915cab099857f388d9e70db615d6b0c52299e4d2dddbfa9614be9bdc556b0`, natural size 639×70이다. 같은 source를 쓰면서 아래처럼 서로 다른 frame/crop을 소비한다.

| 실제 DOM frame, page 원점 기준 CSS px | 첫 그림 | 둘째 그림 |
| --- | --- | --- |
| x / y | 79.25 / 227.078125 | 79.25 / 328.625 |
| frame width / height | 635.1875 / 75.109375 | 635.1875 / 62.09375 |
| image width / height | 638.625 / 75.109375 | 635.1875 / 74.703125 |

- 두 frame은 `overflow:hidden`이며 둘째 image는 frame보다 높다. 그림 존재만 세지 않고 실제 frame/crop·두 배너의 내용과 전체 위치를 독립 Print·review·overlay에서 직접 확인했다. resampling/경계 차이는 남아 있으므로 픽셀 동일을 주장하지 않는다.
- 문서 초기화·글꼴 준비·두 image의 `decode()`와 다음 paint frame을 기다린 뒤 캡처했다. 1초 간격의 두 전체 화면은 바이트 동일이고 page error·실패한 network request·열린 dialog는 모두 0이었다. `document.fonts.status=loaded`지만 배너 안 글자는 image 자체의 픽셀이며 Typeface coverage 검증으로 계산하지 않았다.
- DOM cache key는 revision 2/resources 2, 요청/선택 Canvas2D, screen, requested/effective DPR 1이다. 이 입력에는 BehindText/InFrontOfText 별도 canvas가 없으므로 그 plane의 합성까지 확인했다고 하지 않는다.

## 최종 화면·독립 기준 대조

- [전체 Studio 화면](canvas2d-screen.png), [실제 한 쪽 브라우저 캡처](canvas2d-page-1.png), [review](canvas2d-review-1.png), [overlay](canvas2d-overlay-1.png), [전쪽 TSV](canvas2d-screen-silhouette.tsv). 캡처는 page 사각형의 정수 외접 영역 794×1123이며 source canvas만 추출하지 않아 DOM 그림도 포함한다.
- PDF는 `pdftoppm -r 96 -png -singlefile`로 읽은 794×1122 PNG다. 기존 `visual_sweep.py`의 `padded_pair`가 아래 흰 여백 1px만 맞췄으며 이미지를 resize하거나 편집 가이드·그림을 mask/exclude하지 않았다.
- 같은 공식 2px raw silhouette는 **99.20882%**이며 TSV는 99.21%, below_90=0, 전체 쪽수 1/1, 누락 0이다. 내용 union 41,457 pixels, boundary 보정 0이다. 기존 내용 threshold·2px 반경·허용치를 변경하지 않았다.
- 이 TSV 호출 자체는 `silhouette_only`, `pr_review_gate.status=not_evaluated`다. review/overlay를 직접 열어 이 한 대조군의 crop/소유/합성을 확인한 결과와 전체 PR 승인 gate를 구분한다. Native PNG·새 fresh WASM build·CanvasKit image screen의 검증으로 재분류하지 않는다.
- 새 탭 초기 engine WASM 응답 `/assets/rhwp_bg-DCK6zzId.wasm`: HTTP 200, SHA-256 `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`. 기존 5번 producer와 같다. 초기 JS `/assets/index-DePSEk_P.js`: `5f43daf3425c692ed4c456cd240efb5041614a2eb5ef29ce9a4b114857823cf5`. 초기 응답 body를 읽었고 별도 사후 fetch로 대신하지 않았다.

## 남은 범위

- 이 정상 대조군의 초기 100%/DPR 1 Canvas2D DOM 그림 합성만 추가 확인했다. zoom/DPR/resize 후 복원, BehindText/InFrontOfText plane, 이 그림 입력의 CanvasKit screen, Windows UI·저장/재열기·앱 인쇄는 아직 미검증이다.
- latest devel `f125c735c7d2d44eaa44b78c70465bc89af45fb1`에는 backend 전환 surface 해제·SVG glyph raster 보정이 있다. 이 자료는 기존 e4 제품의 실행 결과이며 새 통합본의 검증이나 그 보정의 이식 완료로 쓰지 않는다.
- 제품 변경 없이 증적만 추가했으므로 Rust/WASM 전체 검사를 반복하지 않았다. 기존 검토 PR #8349·후속 후보 head·features.md 체크상태를 유지했고 이 한 건으로 신규 Open PR 제출 조건을 완료했다고 표시하지 않는다.
