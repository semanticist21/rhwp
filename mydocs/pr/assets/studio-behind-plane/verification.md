# 실제 Canvas2D BehindText의 초기 합성

**기존 HWPX 1쪽의 배경 → 글 뒤 글상자 → 본문·그림 합성을 확인했다. 글꼴 공급·coverage와 화면을 연결했으며, 전환·앱 전체 완료나 브라우저 내부 glyph 선택의 증거는 아니다.**

- 실행 제품은 DPR 후보 `3ef5046f42042c6ad50d47e0054e2132467395cd`, engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 증적 부모 `afd34cb84ab0018598fc2471a8d44ed55bd4c3e2` 대비 이 폴더만 추가한다. 제품·검사·입력·reference·threshold·features.md·기존 후보와 #8349 검토 head는 유지했다.
- [기존 입력](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/samples/issue8271/tac_picture_behindtext_textbox.hwpx)은 89161 bytes, SHA-256 `19ac4952c2d2254fce267b950b40dba10cfe607c9c6bf9b6139804dfdbbc7b00`, 실제 1쪽이다. 원래 #8271/#8283의 편집 API 합성 입력으로, 사용자 실문서라고 하지 않는다. 새 입력을 생성·수정하거나 저장/재열기를 실행하지 않았다.
- [같은 입력의 독립 Print](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/pdf/issue8271/tac_picture_behindtext_textbox-2020.pdf)는 47160 bytes, SHA-256 `fe9dd3caedb3dabc7e38f81de03a3804bbf7430597f9c4fc3c3e86f3ba2e0ab0`, 1쪽이다. Creator Hwp 2020 0.0.0.0 / Producer Hancom PDF 1.3.0.550 / 생성 2026-10-27이며 PDF의 HCRBatang은 embedded CID TrueType다. 새 Print/reference/golden을 만들지 않았다.
- [원 PR #8283](https://github.com/edwardkim/rhwp/pull/8283)의 저장 줄 없는 그림 호스트 보정과 [메인터너 적용·보정](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/mydocs/pr/archives/pr_8283_review_impl.md), [정식 회귀의 독립 Print 연결](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/tests/cases/issue_8271_hwpx_reopen_tac_picture_float.rs)을 먼저 읽었다. 원 PR의 과거 PDF 미확보 설명을 현재 이 입력의 근거로 대신 쓰지 않았다. #8319/#8328 뒤의 source와 기존 입력 자체의 PDF를 사용한다.
- manifest에는 이 입력 항목이 없다. 기존 extended shape-object의 front 표본은 목적이 다르므로 재사용하지 않았고, 기존 정상 BehindText 후보를 필요한 범위로 추가 조사했다. 다른 후보 #1994는 원문·한컴 PDF 모두 4쪽이어서 이 단위에서는 전체 화면 검증하지 않았다.
- [Node 읽기 구조](structure.json)는 hasBehind=true/hasFront=false, image1/rawSvg0/flowImage1, pageBackground1/textRun4/image1/rectangle1이다. root.children.3 및 그 자식의 layer는 behindText/zOrder0/stableIndex3이며 그룹 자식은 2개다. 브라우저 글꼴 등록 전의 별도 구조 probe로서 실제 Studio 호출 payload나 그 정밀 좌표를 대신하지 않는다.
- [PageRenderer](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L567)는 배경/behind를 별도 canvas로 만들고 본문 배경을 투명하게 둔다. shouldSplitStaticFlow는 hasBehind일 때 false이므로 이 입력의 flow 그림도 본문 canvas에서 그리며 DOM 그림 0개가 정상이다. layerCount는 1+behind2=3으로 source에서 정했다.

## 실제 화면과 비동기 준비

- 준비된 production의 loopback static 서버, 실제 파일 입력 → loadFile/initializeDocument → CanvasView/PageRenderer를 사용했다. 개발 서버·사용자 데스크톱·DEV 전역·제품 setter/dispatchEvent·강제 renderPage·별도 renderer/canvas·toDataURL을 쓰지 않았다.
- Chrome 156.0.8078.73 / viewport1280×1500 / screen / DPR1 / 100%다. 실제 cache key는 요청·선택 Canvas2D/default·revision/resources3·layers3·budget within이다. background-0, behind-0, main의 backing은 모두 794×1123, 실제 surface 픽셀 합은 2674986이다.
- 세 canvas는 같은 page parent에 연결돼 상대 원점 (0,0), 사각형 793.6875×1122.5 CSS px가 같다. background/behind/flow의 z-index와 sibling 순서는 각각 0/1/2, 앞의 두 canvas pointer-events는 none이다. 기존 canvas에서 읽은 nonzero alpha는 배경891662/behind17633/flow7627픽셀이다. 개수만으로 형태·정확성을 판정하지 않았다.
- document.fonts.ready·DOM decode·두 rAF와 비동기 paint 안정화를 기다렸고 자연스러운 같은 visible 캐럿 phase에서 1초 간격 전체 화면 두 장이 바이트 동일했다. page error·실패한 network request·열린 dialog는 0이다. 초기 검사 코드가 background canvas를 본문으로 잘못 골랐던 로컬 시도는 보정했으며 최종 성공 run만 제출한다. 제품은 바꾸지 않았다.
- [실제 전체 화면](canvas2d-screen.png)과 [page 사각형](canvas2d-page.png)은 raw CDP Page.captureScreenshot이다. page 전체가 viewport 안에 있었고 캡처 전후 DPR1을 확인했다. 기존 표시 canvas의 alpha 읽기는 보조 진단이고 전체 합성의 증거는 이 브라우저 화면이다.
- HWPX의 JPEG 1개는 저장된 moogung.jpg와 bytes/hash가 같다([producer](producer.json)). Node 구조의 그림 bbox는 (133.013,132.267,113.387,59.293), rectangle bbox는 (117.16,203.773,226.773,75.587)이다. 실제 화면에서 그림·네 문구·글상자 테두리를 확인했고 흰 글상자 뒤로 그림이 사라지지 않았다. 아직 다른 중첩·clip 반례나 image cache 생명주기를 검증한 것은 아니다.
- 새 탭의 실제 초기 JS body SHA-256 `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`, engine WASM `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`이 준비된 3ef producer와 같다. 같은 URL·사후 fetch로 대신하지 않았다.

## 실제 글꼴 공급과 한계

- 초기 HTTP200 HANBatang.woff body는 11613312 bytes/SHA-256 `b3192018e3bd4da76b29b0530f71c79fb6f465a2b7cd41308bc38f9a4ae09a6c`다. CSS의 함초롬바탕 규칙은 이 URL을 가리키고 loaded face와 document.fonts.check를 확인했다. HCRDotum/RHWPOldHangul 초기 응답의 200/hash 및 중복 unloaded face 원값도 producer에 남겼다.
- 기존 flow/behind context의 실제 font 문자열은 13.333px 함초롬바탕을 첫 family로 하고 Haansoft Batang 등 fallback chain을 가진다. [공급 파일 coverage](font-coverage.json)는 설치된 fc-query로 실제 HTTP body를 읽었으며 family=HCR Batang/함초롬바탕, Regular, 「앞 글 뒤 글다음 문단중요」의 공백 포함 고유 문자10개가 모두 존재한다. 새 도구나 글꼴을 설치하지 않았다.
- 이는 공급 bytes/coverage·요청 chain·실제 paint의 연결이며 Chromium 내부의 각 glyph가 선택한 face/outline 추적은 아니다. CSS loaded만으로 glyph를 입증하지 않았고 CanvasKit Typeface 준비나 Native HANBatang.ttf와 동일 outline이라는 주장도 하지 않는다. 글꼴 정밀 일치와 다른 글꼴은 미검증이다.

## 독립 Print 비교와 남은 범위

- [전쪽 TSV](silhouette.tsv)는 각 backend 1/1쪽·누락0, 공식2px raw 실루엣 Canvas2D **97.59%**, Native Skia **100.00%**다. 100%는 픽셀 동일이 아니다. 보조값 silhouette_only/not_evaluated를 전체 PR gate 성공으로 바꾸지 않는다.
- Native는 기존 검증 바이너리 SHA-256 `a6c32ee47df10497b0b3994d9a0184723a3f69308c28cc8d6cef3bc1ef8d12ec`의 실제 export-png/print/compat2024/scale1을 이 입력에 새로 실행했다. [PNG](native-skia.png)·[review](native-skia-review.png)·[overlay](native-skia-overlay.png)를 직접 열었다. Fedora 호스트 CLI이며 YLCC 제출 작업/컨테이너 실행이 아니다.
- 독립 PDF는 설치 Poppler26.01.0/pdftoppm96dpi로 raster했다. 실제 page794×1123과 PDF794×1122의 아래 흰 여백1px만 기존 padded_pair로 보충했다. resize·가이드/캐럿 masking·exclusion·threshold 변경이 없다. [화면 review](canvas2d-review.png)·[overlay](canvas2d-overlay.png)에 글자·가이드·경계 raster 차이를 남겼고 정밀 일치 완료로 표시하지 않는다.
- 표본의 초기 합성 증적만 추가했다. 제품/검사 source를 바꾸지 않아 Rust/Studio 전체 gate·새 WASM 빌드를 반복하지 않았고 base/head 하락폭이나 backend 전환을 검사했다고 하지 않는다. 최신 확인 devel698906bf4aeaa6d0d7101a4fd4d7c4a9229f6bb2의 #8165는 Rust rendering/header_footer_layout을 바꿨으므로 이 3ef/ad4 실행을 최신 engine 결과로 쓰지 않는다.
- 이 BehindText의 zoom/DPR/resize·CanvasKit 소비·실제 backend 전환·다른 중첩 가림·브라우저 내부 glyph/다른 글꼴·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규5번 Open PR 조건은 미결이다. #8349 본문은 여유16자 때문에 유지하고 이 공개 링크를 최종 알림/자동화에 남긴다.
