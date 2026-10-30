# CanvasKit software의 InFrontOfText 실제 소비

**같은 정상 도형 1쪽의 front 그룹을 실제 Studio CanvasKit software surface에서 확인했다. 이 표본은 path만 사용하므로 Typeface·glyph 검증은 별도 미결이다.**

- 실행 제품은 `3ef5046f42042c6ad50d47e0054e2132467395cd`, engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 증적 부모 `e57ebde70dbb70120d32080dac08ba0f1e34e4da`는 같은 제품의 기존 증적을 담고 있다. 이 branch는 이 증적 폴더만 추가하며 제품·검사·입력·reference·threshold·features.md·기존 후보/검토 #8349 head를 유지한다.
- manifest shape-object의 `samples/shape-group-02.hwp`와 [기존 독립 PDF](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/pdf/shape-group-02-hwp-2020.pdf) 전체 1/1쪽이다. SHA-256은 HWP `12a4b4962a57b4d292d23a1809255ddb7e8400af4dc7884c5bcb7b4767a6628f`, PDF `ab759c9c2031f4a6fe64c75381a2c5dbf215777a98b1b0b7cd0243f801662dc7`이다. PDF 실제 Creator는 Hwp 2022이며 새 Print/golden을 만들지 않았다.
- [PageRenderer](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L429)는 CanvasKit에서 별도 DOM plane을 제거하고 같은 profile의 PageLayerTree를 [원래 renderPage](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit-renderer.ts#L971)에 전달한다. 전체 root의 replay가 한 surface를 소비하므로 Canvas2D의 별도 front-0 DOM canvas가 없는 것이 정상이다.

## 실제 제품 진입과 payload 관측

- 기존 production dist만 loopback static 서버로 제공하고 실제 파일 입력 → loadFile/initializeDocument → CanvasView/PageRenderer로 진입했다. 개발 서버·DEV 전역·제품 setter/dispatchEvent·별도 renderer/canvas·강제 renderPage·toDataURL을 사용하지 않았다.
- Chrome 156.0.8078.73·1280×1500·DPR 1·100%·screen, 요청/선택 CanvasKit/default/software·revision/resources 4다. 실제 main canvas 1개(794×1123), layerCount 1·budget within과 DOM 그림/별도 front/behind canvas 0개를 읽었다.
- 실제 초기화 chunk의 원래 renderPage 한 호출과 뒤따른 원래 diagnostics 호출에 CDP function-call breakpoint를 걸었다. 인자와 receiver를 읽고 각 breakpoint를 제거·재개했으며 두 receiver가 같은 객체임을 확인했다. 함수/prototype/반환값을 바꾸지 않았고 Runtime.queryObjects도 쓰지 않았다. 이 계측을 성능 검증으로 표시하지 않는다.
- 원래 renderPage에 전달된 실제 tree는 screen·scale 1·793.707×1122.507이며 root.children.3의 `layer.textWrap=inFrontOfText`, zOrder 0·stableIndex 2·자식 2개다. op는 pageBackground 1개와 path 2개, fontResources는 0개였다. [producer 원값](producer.json)의 actualRenderInput은 이 한 실제 호출의 관측이며 별도 Node probe나 모든 재렌더 호출을 기록한 결과가 아니다.
- 캡처 전후의 원래 renderer는 실제 표시된 DOM canvas와 surfaceCache.target.canvas가 같은 객체였다. software preference/backend·fallbackReason=null, render 완료·passesRuntimeReadinessGate=true, unsupported op·readiness blocker·이미지/Typeface pending·실패·미등록 fallback·hidden Canvas2D overlay는 0/없음이었다.
- 실제 payload는 글자/이미지 op가 없고 캡처 전후 image cache·local/bundled Typeface·glyph-run blob/Typeface·glyphRuns도 0이다. 이 경로에서는 Typeface/glyph 내용 검증이 비해당이며, 앱 CSS face 성공이나 캐시 수치로 다른 문서의 글꼴 준비를 입증하지 않는다. paragraphFontEntries 3 등 전역 원값은 그대로 보존했다.

## 실행 신원·자원 관측의 한계

- 새 탭 초기 JS 응답은 `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`, engine WASM은 `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`로 기존 실제 최적화 producer와 같다. 초기 응답 body를 읽었으며 사후 별도 fetch로 대신하지 않았다.
- CanvasKit WASM `25ebed8e60158c5854f8dc807b936daca21354f8bfb6a2231266b0a93812f301`, 실제 renderer chunk `34f1c749212f8a65ca6f9f636b672f384acb9924628f5dbb157995052d9763b6`도 초기 응답 body와 현재 production 파일 bytes/hash가 같았다.
- page error·실패한 network request·열린 dialog는 0이다. 다만 중복 HANBatang/RHWPOldHangul HTTP 200 응답 2개의 body는 진단 API가 `No data found for resource`를 반환해 hash를 읽지 못했다. 같은 URL의 다른 성공 응답 hash와 이 관측 실패를 모두 producer에 남겼으며 두 사본을 같은 bytes라고 대신 확정하지 않았다. 이 path-only 표본의 Typeface 준비 성공으로 사용하지 않는다.

## 실제 화면과 독립 Print

- [실제 전체 화면](canvaskit-screen.png)과 [실제 page 사각형](canvaskit-page.png)을 raw CDP Page.captureScreenshot으로 캡처했다. 자연스러운 같은 visible 캐럿 phase에서 1초 간격 전체 화면 2장은 바이트 동일했고 캡처 뒤 DPR도 1이다. 내용·가이드·캐럿을 숨기거나 masking하지 않았다.
- [전쪽 TSV](silhouette.tsv)는 이 새 backend의 전체 1/1쪽·누락 0, 공식 2px raw 실루엣 **100.00%**다. 100%는 픽셀 동일을 뜻하지 않으며 silhouette_only/not_evaluated를 전체 PR gate 통과로 바꾸지 않는다. [review](review.png)·[overlay](overlay.png)를 직접 확인했다.
- 기존 Poppler 26.01.0·96dpi reference PNG의 SHA-256 `4efa2f2202003deb7d0d12acb8b7050926e2b72dce8adbe2c27cfac8bfa3aee1`을 공개 초기 producer와 대조해 재사용했다. 실제 page 794×1123/PDF 794×1122의 기존 흰 여백 1px만 사용했으며 resize·guide masking·exclusion·threshold 변경은 없다.
- [기존 실제 Native PNG](https://github.com/semanticist21/rhwp/blob/e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd/mydocs/pr/assets/studio-front-plane/native-skia.png)·[Native 검증/TSV](https://github.com/semanticist21/rhwp/blob/e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd/mydocs/pr/assets/studio-front-plane/verification.md)는 같은 Rust 제품과 입력의 기존 범위로 재사용한다. 같은 Native export·Canvas2D 초기/전환·기하/9pt density·제품 빌드/전체 검사를 반복하지 않았다. 새 Native 실행이나 두 Native 바이너리 비교로 표시하지 않는다.
- CanvasKit의 이 표본 zoom/DPR/resize 복원·실제 backend 전환·BehindText·본문과 겹친 개체의 가림·실제 glyph/Typeface·SVG glyph raster·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규 5번 Open PR 제출 조건은 미결이다. 검토 PR 본문은 남은 16자 때문에 변경하지 않고 이 공개 증적 링크를 최종 알림/자동화에 남긴다.
