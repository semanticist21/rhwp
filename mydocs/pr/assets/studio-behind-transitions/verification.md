# Canvas2D BehindText의 배율·DPR·창 크기 전환과 복원

**같은 1쪽의 background → behind → flow 세 canvas가 제어된 9단계에서 함께 갱신되고 원래 화면으로 복원됐다. 실제 OS 모니터 이동·CanvasKit·다른 중첩·앱 전체 검증이 아니다.**

- 실행 제품은 `3ef5046f42042c6ad50d47e0054e2132467395cd`, engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 증적 부모 `3deefcc6937015121e7cd63927ec30dc9f9404a7` 대비 이 폴더만 추가하며 제품·검사·입력·reference·threshold·features.md·기존 후보와 #8349 검토 head는 유지했다.
- [기존 초기 증적](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/verification.md)의 samples/issue8271/tac_picture_behindtext_textbox.hwpx와 같은 독립 Hancom PDF 각 1쪽을 사용했다. HWPX SHA-256 `19ac4952c2d2254fce267b950b40dba10cfe607c9c6bf9b6139804dfdbbc7b00`, PDF `fe9dd3caedb3dabc7e38f81de03a3804bbf7430597f9c4fc3c3e86f3ba2e0ab0`다. 원래 편집 API 합성 입력이며 실문서·새 생성 fixture나 새 Print라고 하지 않는다.
- 기존 #8283→#8319/#8328 메인터너 보정과 원문/PDF 연결은 초기 증적의 범위를 유지했다. [ViewportManager](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/viewport-manager.ts)의 DPR/RO 예약 → [CanvasView](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvas-view.ts#L1657)의 budget·renderCanvas → [PageRenderer](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L567)의 background/behind/flow 합성 경계를 읽었다.
- [예산 계약](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/render-surface-budget.ts)은 raw DPR을 보존하고 필요할 때 낮은 effective DPR을 선택한다. 기본 visible/retained pixel budget은 32000000/40000000이며 이번 source나 허용치를 바꾸지 않았다. 세 canvas의 구성은 source상 1+behind2이고 hasBehind에서는 flow 그림을 별도 DOM 그림으로 분리하지 않는다.
- 준비된 production/static 서버·실제 파일 입력 → CanvasView/PageRenderer를 사용했다. 개발 서버·사용자 데스크톱·DEV 전역·제품 setter/dispatchEvent·강제 renderPage·별도 renderer/canvas·toDataURL을 쓰지 않았다.

## 실제 9단계

- Chrome 156.0.8078.73, Canvas2D/default/screen에서 initial1280×1500/DPR1 → DPR2 → DPR3 → DPR1.25 → DPR1 → 실제UI125% → 100% → 1440×1600/DPR2 → 원복을 확인했다. [단계 TSV](stages.tsv)·[이벤트 TSV](events.tsv)·[원값](producer.json)을 남겼다.
- 배율은 실제 status bar mouse 클릭과 PageUp×2/ArrowUp×5, PageDown×2/ArrowDown×5를 사용했다. 14개 keydown 모두 trusted였다. DPR은 CDP device metrics+screen resolution media feature의 같은 controlled 조건이며 metrics-only 반복·물리 OS 신호 전달의 성공으로 확장하지 않는다.
- 모든 단계에서 background-0/behind-0/본문 canvas는 같은 page parent에 연결됐고 CSS 사각형 x/y/width/height의 본문 대비 차이는 모두 0이었다. z-index와 sibling의 0→1→2 순서 및 background/behind pointer-events:none을 유지했다. DOM 그림 0개는 이 source 경로에서 정상이며 그림 준비의 대용으로 쓰지 않았다.
- 모든 단계 layerCount3, budget within, raw/effective DPR이 같았다. 필요시 낮아질 수 있는 계약을 무조건 같다고 강제하지 않고 실제 값을 읽었다. 각 canvas의 backing은 DPR1에서794×1123, 2에서1588×2245, 3에서2382×3368, 1.25에서993×1404였다. actualSurfacePixels는 세 backing 픽셀 수의 합과 같았다.
- 기존 세 canvas의 alpha가 있는 픽셀은 모든 단계에서 0보다 컸다. 본문의 기존 표시 canvas에서 초기 source image bbox×현재 renderScale 영역의 색 픽셀도 확인했다. 기본6780/DPR2 26784/DPR3 60516/scale1.25 10506개다. 이는 그림 존재 진단이며 새 PDF 점수·형상 정밀 일치·이미지 cache 수명 검사로 해석하지 않는다.
- 각 단계 document.fonts.ready·DOM decode·두 rAF·비동기 paint 안정화를 기다렸다. fonts는 loaded이고 실제 flow/behind context의 font chain 원값도 보존했다. 초기 HTTP body가 [기존 coverage 증적](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/font-coverage.json)의 HANBatang.woff 11613312 bytes/SHA-256 `b3192018e3bd4da76b29b0530f71c79fb6f465a2b7cd41308bc38f9a4ae09a6c`와 같음을 확인했다. 공급/10문자 coverage를 재사용하며 Chromium 내부 glyph 선택·CanvasKit Typeface·다른 글꼴 검증으로 확대하지 않는다.
- 매 단계 1초 간격 실제 전체 화면 두 장이 바이트 동일했다. 각 캡처 뒤에도 DPR·canvas 소유/backing을 다시 읽었고 PNG 물리 크기=CSS viewport×DPR을 확인했다. 최종 run의 page error·실패한 network request·열린 dialog는 0이다.

## 캐럿을 변경하지 않은 화면 판독

- [원래 캐럿](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/engine/caret-renderer.ts#L201)은 500ms마다 opacity가 바뀐다. 초기 진단은 캡처 후 DOM opacity도 1일 것을 요구해 DPR2/3에서 중단됐다. 실제 캡처 반환이 760~880ms 걸리는 관측을 확보했으며 이를 제품 paint 실패로 해석하지 않았다.
- 최종 진단은 자연스러운 hidden→visible 시작을 기다린 뒤 raw [Page.captureScreenshot](https://chromedevtools.github.io/devtools-protocol/tot/Page/) PNG를 읽었다. 설치된 Playwright의 PNG decoder로 실제 캐럿 중앙열 내부 픽셀이 원래 CSS 색 #000/alpha255인지 확인했다. 승인한 26개 캡처의 내부 픽셀은 전부 그 색이며 캡처 후 DOM opacity가 달라도 실제 PNG 상태와 구분했다.
- PNG를 다시 쓰거나 캐럿·가이드·글자를 숨기지 않았고, 함수/타이머/스타일/제품 상태를 변경하거나 threshold를 완화하지 않았다. 배경·본문 canvas를 캐럿 없는 별도 화면으로 대신하지 않았다. 도중의 인코딩 속도 옵션 시도는 최종 run에 쓰지 않았으며 최종 PNG는 기본 CDP 인코딩이다. 성능 benchmark 결과가 아니다.

## 캡처·복원과 독립 기준

- [DPR2 page](dpr2-page.png)·[DPR3 page](dpr3-page.png)·[DPR1.25 page](dpr125-page.png)·[resize/DPR2 page](resized-dpr2-page.png)를 실제 브라우저 page 사각형에서 캡처하고 직접 확인했다. DPR2 PNG높이2246/backing2245, DPR3 PNG3369/backing3368은 CSS clip 높이 올림과 backing의 차이로 따로 남겼다.
- [125% 전체 UI](zoom125-screen.png)는 paper가 scroll 영역 밖으로 일부 벗어나 전쪽 캡처가 아니다. paper 위치·가시 영역·scroll 원값을 기록했으며 reference에 맞춘 축소·마스킹은 없다.
- DPR 원복·배율 원복·[최종 page](restored-page.png)는 이번 initial과 [기존 초기 page](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/canvas2d-page.png)에 모두 바이트 동일하다. SHA-256 `b10fad9a0298c5936a993792c85c3000ca0981eb0e0906c28ead32170c04775c`, scroll top/left도 0으로 복원됐다. 창 크기만 달랐던 DPR2 두 page도 바이트 동일하다.
- 같은 source/build/input과 초기/원복 bytes를 확인해 [기존 전쪽 독립 Print/Native 검증](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/verification.md)을 그 범위로 재사용했다. 기존 Canvas2D97.58689%/Native100.00%는 초기 1쪽의 2px raw 실루엣이며 100%는 픽셀 동일이 아니다. 이번 다른 DPR의 새 공식 PDF 점수·전쪽 sweep·base/head 하락폭·전체 PR gate를 산출하지 않았다.
- 새 탭 초기 JS body `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`와 engine WASM `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`은 준비된 실제 최적화 3ef 산출물과 같았다. 같은 URL이나 사후 fetch로 대신하지 않았다. 최종 응답 원값과 오류도 producer에 보존했다.
- 최신 확인 devel168aaaf2bf5a4ef5ecc70a61087aeabf2d9d2433에는 #8165와 #8145 cellzone 배경 보정이 추가돼 Rust engine이 다르다. 이 3ef/ad4 실행을 최신 source 실행으로 표시하지 않고 해당 메인터너 보정을 유지한다. 새 #8385 셀 블록 크기 PR은 다른 작업으로 확인했으며 중복 변경하지 않았다.
- 증적 전용이므로 제품 빌드·Native export·Rust/Studio 전체 검사·기존 기하/density를 반복하지 않았다. CanvasKit BehindText·실제 backend 전환·다른 중첩/clip·브라우저 내부 glyph/다른 글꼴·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규5번 Open PR 조건은 미결이다. #8349 본문은 여유16자 때문에 유지하며 공개 링크는 최종 알림/자동화에 남긴다.
