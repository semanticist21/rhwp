# CanvasKit front의 제어된 전환·surface 수명·복원

**path 전용 InFrontOfText 1쪽을 실제 CanvasKit software 화면에서 전환하고 복원했다. 관측한 객체의 수명과 화면에 대한 결과이며 물리 Windows·글꼴·모든 native 할당의 검증이 아니다.**

- 실행 제품 `3ef5046f42042c6ad50d47e0054e2132467395cd`/engine base `ad4bf269ffef5adf2142b33004a9981fad7ce804`와 기존 준비된 production을 재사용했다. 증적 부모 `d290570765560d28b41acc32f8f2e4020289c981` 대비 이 폴더만 추가하며 제품·검사·입력·reference·threshold·features.md·기존 후보와 검토 #8349 head는 유지했다.
- 기존 manifest의 shape-group-02.hwp/독립 Hancom PDF 전체 1쪽과 [CanvasKit 초기 증적](https://github.com/semanticist21/rhwp/blob/d290570765560d28b41acc32f8f2e4020289c981/mydocs/pr/assets/studio-canvaskit-front/verification.md)을 사용했다. HWP SHA-256 `12a4b4962a57b4d292d23a1809255ddb7e8400af4dc7884c5bcb7b4767a6628f`, PDF `ab759c9c2031f4a6fe64c75381a2c5dbf215777a98b1b0b7cd0243f801662dc7`다. 새 Print/golden은 만들지 않았다.
- [SurfaceCache.get/clear](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit/surface-cache.ts)는 같은 canvas/크기면 재사용하고 바뀌면 이전 surface.delete를 호출한다. [정적 picture context](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit-renderer.ts#L1033)는 scale 등을 포함하며 cache는 32개/64MiB로 한정된다. 공통 viewport/budget 경로와 실제 소비를 연결했다.
- 기존 production/static 서버·실제 파일 입력 → CanvasView/PageRenderer를 사용했다. 개발 서버·사용자 데스크톱·DEV 전역·제품 setter/dispatchEvent·별도 renderer/canvas·강제 renderPage·toDataURL은 사용하지 않았다.

## 실제 9단계

- Chrome 156.0.8078.73, 요청/선택 CanvasKit/default/software/screen에서 initial 1280×1500/DPR1 → DPR2 → 3 → 1.25 → 1 → 실제 UI125% → 100% → 1440×1600/DPR2 → 원복을 확인했다. [9단계 TSV](stages.tsv)·[이벤트 19개](events.tsv)·[원값](producer.json)을 남겼다.
- 배율은 실제 status bar mouse와 PageUp×2/ArrowUp×5, PageDown×2/ArrowDown×5를 썼고 14개 keydown 모두 trusted였다. DPR은 device metrics+screen resolution media의 같은 controlled 조건이며 metrics-only 반복·물리 OS 모니터 전달의 성공으로 확장하지 않는다.
- 모든 단계는 표시된 DOM canvas 1개와 같은 software surface target을 소비했고 raw/effective DPR이 같았다. backing은 DPR1 794×1123, 2 1588×2246, 3 2382×3368, 1.25 993×1404다. layerCount 1·실제 surface pixel 수·budget within을 읽었으며 예산/허용치를 바꾸지 않았다.
- 매 단계 document.fonts.ready·DOM decode·두 rAF 뒤 준비를 기다렸다. 캡처 전후 RuntimeReadinessGate/render 완료·software/DOM 소유·backing을 확인했다. 미지원 op·image/Typeface pending·실패·미등록 fallback·hidden Canvas2D overlay·page error·실패한 network request·dialog는 0/없음이다.
- 원래 첫 renderPage/diagnostics 호출의 읽기 breakpoint를 제거·재개한 뒤 같은 실제 receiver를 계속 읽었다. 첫 payload의 front 그룹은 inFrontOfText/zOrder0/stableIndex2/자식2, op는 pageBackground1/path2·fontResources0이다. 모든 후속 호출의 payload를 추적한 결과는 아니다. glyph/Typeface 0을 실패나 별도 글꼴 준비 성공으로 해석하지 않는다.
- 각 단계 raw CDP 전체 화면 2장은 1초 간격·같은 자연스러운 visible 캐럿 phase에서 바이트 동일했다. 캡처 뒤에도 표시 canvas/소유/backing을 읽었다. 125%는 종이가 scroll 영역 밖으로 일부 벗어나므로 [실제 UI 화면](zoom125-screen.png)만 제출하고 전쪽이라고 하지 않는다.

## 관측한 객체와 캐시

- CDP가 읽은 실제 DOM canvas 객체 ID는 9단계 내내 1이다. 각 관측 시점의 SkSurface 객체 ID는 1→9로 달랐으며, 한 단계의 두 캡처 전후에는 같은 surface/canvas였다. remote handle은 진단에서만 보유했고 제품 메서드/prototype/반환값을 바꾸지 않았다.
- 관측했던 이전 8개 surface의 원래 isDeleted()는 최종 true, 현재 surface9는 false였다. source의 clear→delete와 연결되는 관측이며 모든 native 할당이나 전역 메모리 해제를 입증했다고 하지 않는다. 별도 heap query는 하지 않았다.
- surfaceCreated 누계는 2→12, reused 누계는 시작/끝 2다. 이 누계를 활성 canvas 수나 이번 전환에서 새 warm get이 실행된 횟수로 해석하지 않는다. 14개 UI 키 사이의 모든 중간 surface를 포착한 결과도 아니다.
- 정적 picture cache는 최대 32개를 유지했고 hits 8→40, misses 8→56, evictions 0→24였다. 같은 scale=1.25인 DPR1.25/zoom100%와 DPR1/zoom125%의 기존 표시 canvas RGBA 해시는 같았다. 이는 raster 진단이며 서로 다른 UI 전체 화면이나 독립 Print 점수를 대신하지 않는다.
- 기존 표시된 software canvas의 getImageData로 RGBA hash와 비백색 픽셀 수를 읽었다. 새 캡처 canvas를 만들거나 색 threshold를 조정하지 않았다. 내용 존재 수치만으로 위치·형상의 정확성을 판정하지 않고 실제 [DPR2 page](dpr2-page.png)·[DPR3 page](dpr3-page.png)·[DPR1.25 page](dpr125-page.png)·[resize/DPR2 page](resized-dpr2-page.png)를 직접 확인했다.

## 복원·독립 기준·한계

- DPR 원복·배율 원복·[최종 page](restored-page.png)는 이번 initial과 [기존 초기 page](https://github.com/semanticist21/rhwp/blob/d290570765560d28b41acc32f8f2e4020289c981/mydocs/pr/assets/studio-canvaskit-front/canvaskit-page.png)에 바이트 동일하다. SHA-256 `3c6fc245f39b0de8c854412e446534ef334d1caeda301619dc55b7d95110c433`, scroll top/left도 0으로 복원됐다. DPR3의 screenshot 높이3369와 backing3368은 CSS clip 올림과 구분한다.
- 기존 같은 source/build/input/output bytes의 [독립 Print/Native 검증](https://github.com/semanticist21/rhwp/blob/d290570765560d28b41acc32f8f2e4020289c981/mydocs/pr/assets/studio-canvaskit-front/verification.md)을 그 기존 범위로 재사용하며 같은 Native export·공식 초기 실루엣·Canvas2D 전환을 반복하지 않았다. 다른 DPR의 새 공식 PDF 점수·전쪽 sweep·base/head 하락폭이나 전체 PR gate를 산출했다고 하지 않는다.
- 새 탭 초기 JS `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`·engine WASM `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`·CanvasKit WASM `25ebed8e60158c5854f8dc807b936daca21354f8bfb6a2231266b0a93812f301`·renderer chunk `34f1c749212f8a65ca6f9f636b672f384acb9924628f5dbb157995052d9763b6`의 초기 응답 body와 준비된 파일 hash가 같았다.
- 중복 RHWPOldHangul HTTP200 응답 1개의 body는 진단 API No data found로 hash를 읽지 못한 원값을 남겼다. 다른 성공 사본의 hash로 이를 대신 확정하지 않았고 실제 network 실패나 이 path-only 문서의 Typeface 실패로 표시하지 않았다. 성능 검증도 아니다.
- 증적 전용이므로 제품 빌드·전체 Rust/Studio 검사·기하·9pt density를 반복하지 않았다. 실제 backend 전환·BehindText·중첩 가림·다른 글꼴/glyph/SVG glyph raster·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규 5번 Open PR 제출 조건은 미결이다. PR 본문은 남은16자 때문에 유지하고 이 링크를 최종 알림/자동화에 남긴다.
