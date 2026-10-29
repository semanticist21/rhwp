# Canvas2D InFrontOfText의 실제 별도 canvas

**기존 정상 도형 1쪽에서 앞 plane의 소유·배치·초기 안정화를 확인했다. 본문과 겹친 개체의 가림이나 다른 plane·전환을 완료했다는 뜻은 아니다.**

- 실행 제품은 DPR 후보 `3ef5046f42042c6ad50d47e0054e2132467395cd`이며 engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 이 branch는 그 부모 대비 이 증적 폴더만 추가한다. 제품·검사·입력·reference·threshold·features.md와 검토 #8349 head는 유지한다.
- [기존 manifest](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/scripts/renderer_baseline_manifest.json)의 extended `shape-object`, `samples/shape-group-02.hwp` 1/1쪽을 사용했다. HWP SHA-256 `12a4b4962a57b4d292d23a1809255ddb7e8400af4dc7884c5bcb7b4767a6628f`.
- 독립 [Hancom PDF](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/pdf/shape-group-02-hwp-2020.pdf)는 SHA-256 `ab759c9c2031f4a6fe64c75381a2c5dbf215777a98b1b0b7cd0243f801662dc7`, 89651 bytes·1쪽이다. 파일명과 달리 실제 Creator는 Hwp 2022, Producer는 Hancom PDF 1.3.0.550이다. 새 Print·원문·golden을 만들지 않았다.
- [읽기 구조](structure.json)의 root.children.3은 `layer.textWrap=inFrontOfText`, zOrder 0·stableIndex 2와 자식 2개를 가진다. Node WASM의 getPageOverlayImages도 hasFront=true/hasBehind=false·image/rawSvg=0을 반환한다. 이 읽기 probe는 Studio 캡처 증거와 구분한다.
- [실제 소비 source](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L254)는 본문을 flow 필터로 그린 뒤 같은 부모에 front 필터 canvas를 합성한다. [front 적용](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L608)은 pointer-events:none·상위 z-index·append 순서다. 이 표본은 behind=false라 background/behind canvas가 없는 것이 정상이다.

## 실제 화면·공급 신원

- 준비된 production을 loopback static 서버로 제공하고 실제 파일 입력 → loadFile/initializeDocument → CanvasView/PageRenderer를 거쳤다. 개발 서버·DEV 전역·제품 setter/dispatchEvent·별도 renderer/canvas·강제 renderPage·toDataURL을 사용하지 않았다.
- Chrome 156.0.8078.73·1280×1500·DPR 1·screen·100%, 요청/선택 Canvas2D·revision/resources 2다. cache key의 layers:2·surfaces:main:794x1123,front:794x1123을 실제 DOM에서 읽었다.
- 본문과 `front-0`은 같은 page parent에 연결돼 같은 사각형 793.6875×1122.5 CSS px를 차지한다. backing은 각각 794×1123, 상대 원점은 (0,0)이다. 본문 z-index 1/sibling 0, front z-index 2/sibling 1이며 front의 pointer-events는 none이다.
- 기존 front canvas를 읽었을 때 alpha가 있는 픽셀은 145984개, 잉크 bbox는 (55,87)~(436,664)다. 별도 빈 canvas나 숨겨진 캡처 renderer를 만들지 않았다. [실제 전체 화면](canvas2d-screen.png)과 [해당 page 사각형](canvas2d-page.png)을 raw CDP Page.captureScreenshot으로 캡처했다.
- 자연스러운 같은 visible 캐럿 phase에서 1초 간격 전체 화면 2장은 바이트 동일했다. document.fonts는 loaded, DOM 그림 0·page error/network 실패/열린 dialog도 0이다. CSS font 객체의 unloaded 중복 등록도 [원값](producer.json)에 보존했으며 실패로 추정하거나 제거하지 않았다.
- 새 탭 초기 JS 응답은 `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`, engine WASM 응답은 `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`로 기존 실제 최적화 후보 producer와 같다. 초기 응답 body를 읽었으며 나중의 별도 fetch로 대신하지 않았다.
- 앱의 실제 HANBatang.woff·HCRDotum.woff·RHWPOldHangul 응답 200/hash와 CSS loaded 상태를 기록했다. 이 도형 표본을 다른 글꼴의 glyph/outline 검증이나 CanvasKit Typeface 준비 증거로 쓰지 않는다.

## 독립 Print 대조와 남은 범위

- [전쪽 TSV](silhouette.tsv)는 각 경로 1/1쪽·누락 0, 공식 2px raw 실루엣 Canvas2D **99.83%**, Native Skia **100.00%**다. 100%는 픽셀 동일을 뜻하지 않는다. 이 보조값의 silhouette_only/not_evaluated를 전체 PR gate 성공으로 바꾸지 않는다.
- Native는 기존 검증 producer `a6c32ee47df10497b0b3994d9a0184723a3f69308c28cc8d6cef3bc1ef8d12ec`의 실제 export-png·print·compat2024 출력이다. [Native PNG](native-skia.png)·[review](native-skia-review.png)·[overlay](native-skia-overlay.png)를 직접 확인했다. Fedora 호스트 CLI이며 YLCC 제출 작업/컨테이너 결과가 아니다.
- 독립 PDF는 설치된 Poppler **26.01.0**, pdftoppm -r 96으로 만들었다. 실제 Studio page 794×1123과 PDF 794×1122의 기존 padded_pair 흰 여백 1px만 허용하며 resize·가이드 masking·exclusion·threshold 변경은 없다. [화면 review](canvas2d-review.png)·[overlay](canvas2d-overlay.png)에 가이드 차이와 경계 raster 차이를 남겼다.
- 이미지 bytes는 그대로 page-1.png 이름으로 복사한 뒤 기존 review helper에 전달해 쪽 번호를 1로 고정했다. 원래 파일명의 canvas2d/shape-group-02에 든 숫자를 페이지로 잘못 읽은 초기 로컬 패널은 게시하지 않았다. 표준 helper와 점수 계산은 바꾸지 않았다.
- 이 단위는 제품 보정 없는 증적 전용이다. 기존 3ef의 코드/최적화 WASM을 재사용했으며 새 전체 Rust/Studio 검사나 base/head 회귀 하락폭 측정으로 표시하지 않는다. 기존 기하·DPR 회귀를 반복하지 않았다.
- BehindText·이 표본의 zoom/DPR/resize 복원·CanvasKit front 소비·실제 backend 전환·본문과 개체가 겹치는 가림·물리 Windows·저장/재열기·앱 인쇄는 미검증이다. 기존 작은 글자 농도·다른 글꼴/회전/세로쓰기/겹침·앱 통합·features.md·신규 5번 Open PR 제출 조건도 미결이다.
