# Canvas2D front plane의 DPR·배율·창 크기 복원

**별도 InFrontOfText canvas가 있는 같은 1쪽을 제어된 브라우저 환경에서 전환하고 복원했다. 물리 Windows 모니터 이동이나 다른 backend 검증이 아니다.**

- 실행 제품은 `3ef5046f42042c6ad50d47e0054e2132467395cd`, engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 이 증적 branch의 부모 `e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd`는 같은 제품에 초기 front 증적만 추가했다. 제품·검사·원문·reference·threshold·features.md와 기존 후보/검토 #8349 head는 유지했다.
- [기존 초기 증적](https://github.com/semanticist21/rhwp/blob/e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd/mydocs/pr/assets/studio-front-plane/verification.md)의 manifest shape-object `samples/shape-group-02.hwp`·독립 Hancom PDF 1쪽을 재사용했다. HWP SHA-256 `12a4b4962a57b4d292d23a1809255ddb7e8400af4dc7884c5bcb7b4767a6628f`, PDF `ab759c9c2031f4a6fe64c75381a2c5dbf215777a98b1b0b7cd0243f801662dc7`다. 새 Print나 golden은 만들지 않았다.
- [ViewportManager](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/viewport-manager.ts)의 해상도 change/RO 공통 예약 → [CanvasView.onViewportResize](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvas-view.ts#L1842) → raw DPR/2층 예산/visible raster → [PageRenderer.applyOverlays](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L496)의 front 재합성 경계를 읽고 실제 소비를 확인했다.
- 준비된 production을 loopback static 서버로 제공하고 실제 파일 입력 → CanvasView/PageRenderer로 진입했다. 개발 서버·사용자 데스크톱·DEV 전역·제품 setter/dispatchEvent·별도 renderer/canvas·강제 renderPage·toDataURL은 사용하지 않았다.

## 실제 9단계와 읽은 불변식

- Chrome 156.0.8078.73, Canvas2D/screen, 초기 1280×1500·100%·DPR 1에서 시작했다. [9단계 TSV](stages.tsv)는 initial → DPR 2 → 3 → 1.25 → 1 → zoom 125% → 100% → 1440×1600/DPR 2 → 원복 순서다.
- 배율은 실제 status bar mouse 클릭과 PageUp×2/ArrowUp×5, PageDown×2/ArrowDown×5로 바꿨다. 실제 keydown 14개가 모두 trusted였다. DOM 상태 setter나 가짜 입력 이벤트로 대체하지 않았다.
- DPR은 CDP device metrics와 screen resolution media feature를 함께 설정했다. [이벤트 19개 원값](events.tsv)과 raw/effective DPR을 남겼다. metrics-only 반복 및 실제 OS 모니터 전달의 성공으로 확장하지 않는다. 물리 PNG 크기는 각 캡처 전후 DPR과 비교했다.
- 모든 단계에서 본문+front-0 canvas 2개만 같은 page parent에 연결되고 backing은 서로 같았다. 앞뒤 순서·front z-index·pointer-events:none을 유지했으며 CSS 사각형 x/y/width/height의 본문 대비 차이는 **모두 0**이었다. 실제 픽셀 수가 두 surface backing의 합과 같았다.
- 각 surface backing은 DPR 1에서 794×1123, 2에서 1588×2246, 3에서 2382×3368, 1.25에서 993×1404였다. 모든 단계의 raw/effective DPR이 같고 budget은 within이었다. [producer 원값](producer.json)의 실제 budget/pixel/bytes를 그대로 남겼으며 허용치를 완화하지 않았다.
- 기존 front canvas의 alpha 잉크는 모든 단계에서 0보다 컸다. 읽기 getImageData는 기존 canvas만 소비했고 새 캡처 canvas를 만들지 않았다. 잉크 bbox 원값도 TSV에 남겼으며 threshold나 위치/굵기를 조정하지 않았다.
- 매 단계에서 document.fonts.ready와 DOM 그림 decode, 두 rAF 뒤 준비를 기다렸다. 이 표본의 DOM 그림은 0개다. 초기 폰트 HTTP 200/hash를 읽었지만 이 도형을 glyph/CanvasKit Typeface 검증으로 표시하지 않는다. page error·실패한 network request·열린 dialog는 0이다.
- 매 단계 1초 간격의 실제 전체 화면 2장이 자연스러운 같은 visible 캐럿 phase에서 바이트 동일했다. 각 캡처 후에도 두 canvas의 연결·소유·backing을 다시 확인했다. 글자/캐럿/가이드를 숨기거나 masking하지 않았다.

## 실제 캡처와 기존 독립 기준

- [DPR 2 page](dpr2-page.png)·[DPR 3 page](dpr3-page.png)·[DPR 1.25 page](dpr125-page.png)·[resize/DPR 2 page](resized-dpr2-page.png)는 브라우저 화면의 실제 page 사각형이다. DPR 3 backing 높이 3368과 PNG 높이 3369는 별도로 기록했다. PNG clip은 CSS 높이를 올림한 1123×DPR 3이며 backing과 같은 크기라고 하지 않는다.
- [125% 실제 전체 화면](zoom125-screen.png)은 paper가 scroll container 밖으로 일부 벗어나므로 전쪽 캡처라고 하지 않는다. PNG를 reference에 맞춰 축소하거나 guide/margin을 제외하지 않았다.
- [최종 원복 page](restored-page.png), DPR 단독 원복, 배율 원복은 이번 initial 및 [기존 초기 page](https://github.com/semanticist21/rhwp/blob/e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd/mydocs/pr/assets/studio-front-plane/canvas2d-page.png)와 모두 바이트 동일하다. SHA-256 `c2340f7520ebda89ecaacdba61902af3e8ba3d665c7b047fc42754c425581ac7`, scroll top/left도 초기 값 0으로 복원됐다.
- 기존 독립 PDF/Native Skia와 [전쪽 실루엣 TSV](https://github.com/semanticist21/rhwp/blob/e3b9f441e0d7c0979827aa37bbabb70f0b70f1fd/mydocs/pr/assets/studio-front-plane/silhouette.tsv)는 초기 증적에서 검증한 범위로 재사용한다. 이번 초기/원복 bytes와 source/build/input이 같음을 확인했으며 같은 Native export·Print 대조를 반복하지 않았다. 다른 DPR PNG의 새 공식 PDF 점수·전쪽 sweep·base/head 하락폭을 산출한 것으로 표시하지 않는다.
- 초기화 JS 응답은 `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`, engine WASM은 `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`로 실제 검증한 준비된 최적화 producer와 같다. 새 탭의 초기 응답 body를 읽었으며 사후 별도 fetch로 대신하지 않았다.
- 증적 전용이므로 제품 빌드·Rust/Studio 전체 검사·기존 3/4/5번 기하·9pt density 회귀를 반복하지 않았다. 실제 backend 전환·CanvasKit front 소비·BehindText·본문과 겹친 개체의 가림·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규 5번 Open PR 제출 조건은 미결이다.
