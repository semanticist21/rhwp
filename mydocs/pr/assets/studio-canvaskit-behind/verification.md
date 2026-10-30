# CanvasKit software의 BehindText·글자·그림 초기 소비

**같은 HWPX 1쪽의 글 뒤 글상자·본문·그림을 실제 Studio CanvasKit surface에서 확인했다. 실제 Typeface·이미지 준비를 읽었으며 픽셀 정밀 일치·다른 글꼴·전환의 완료가 아니다.**

- 실행 제품은 `3ef5046f42042c6ad50d47e0054e2132467395cd`, engine base는 `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 증적 부모 `2d28aec3c3b3bcb813e1b50629db069a39310438` 대비 이 폴더만 추가하며 제품·검사·입력·reference·threshold·features.md·기존 후보와 #8349 검토 head는 유지한다. 현재 devel168aaaf2bf5a4ef5ecc70a61087aeabf2d9d2433의 #8165/#8145 Rust 보정을 이 실행에 포함했다고 하지 않는다.
- [기존 입력과 독립 Print](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/verification.md)은 samples/issue8271/tac_picture_behindtext_textbox.hwpx / pdf/issue8271/tac_picture_behindtext_textbox-2020.pdf, 각각 1쪽이다. SHA-256은 HWPX `19ac4952c2d2254fce267b950b40dba10cfe607c9c6bf9b6139804dfdbbc7b00`, PDF `fe9dd3caedb3dabc7e38f81de03a3804bbf7430597f9c4fc3c3e86f3ba2e0ab0`다. 기존 편집 API 합성 입력/한컴2020 Print이며 새 fixture·Print·저장/재열기를 만들거나 실행하지 않았다.
- #8283 원 해결→#8319/#8328 메인터너 보정·정식 회귀의 동일 입력/PDF 연결은 기존 증적의 범위를 유지한다. [PageRenderer](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/page-renderer.ts#L420)는 원래 tree와 이미지/글꼴 resolver를 CanvasKit에 전달하고 별도 DOM plane을 제거한다. [replay plane](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit/replay-plane.ts)의 background→behindText→flow→inFrontOfText를 [한 surface에 재생](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit-renderer.ts#L1070)하므로 Canvas2D의 별도 세 canvas가 없는 것이 정상이다.

## 실제 제품 진입·payload·소유

- 준비된 production/static 서버·실제 파일 입력 → loadFile/initializeDocument → CanvasView/PageRenderer를 사용했다. 개발 서버·사용자 데스크톱·DEV 전역·제품 setter/dispatchEvent·별도 renderer/canvas·강제 renderPage·toDataURL을 쓰지 않았다.
- Chrome156.0.8078.73 / viewport1280×1500 / DPR1 / 100% / screen, 요청·선택 CanvasKit/default/software·revision/resources4다. 표시된 main canvas1개794×1123·layerCount1·budget within·실제891662surface픽셀과 같은 page 소유/사각형을 읽었다. DOM 그림과 별도 front/behind canvas는 0개다.
- 실제 초기화 chunk의 원래 renderPage 한 호출과 뒤따른 원래 diagnostics에 CDP function-call breakpoint를 걸어 인자·receiver를 읽고 각각 제거·재개했다. 두 receiver는 같은 객체였고 캡처 전후 surfaceCache.target.canvas는 실제 표시 DOM canvas와 동일했다. 메서드/prototype/반환값을 바꾸거나 Runtime.queryObjects를 쓰지 않았으며 성능 검증이 아니다.
- 그 실제 tree는 screen·scale1·793.707×1122.48, root.children.3 및 그 자식의 behindText/zOrder0/stableIndex3·그룹자식2를 포함한다. op는 pageBackground1/textRun4/image1/rectangle1이다. 「앞 」「글 뒤 글」「다음 문단」「중요」의 요청/paint family는 모두 함초롬바탕 regular다. [producer actualRenderInput](producer.json)은 이 한 실제 호출의 관측이며 별도 Node probe나 모든 재렌더의 payload를 대신하지 않는다.
- fontResources는 길이0인 배열로 추정하지 않고 실제 object의 blobs/faces 배열이 각각0임을 읽었다. 이 표본은 TextRun 경로이며 glyphRuns·glyph-run blob/Typeface는0이다. 실제 bundled Typeface가1개 준비돼 있으므로 이0을 글꼴 실패나 path-only 비해당으로 해석하지 않는다.

## 실제 준비된 자원

- 캡처 전후 원래 getter의 software surface/preference·fallbackReason=null, render 완료·RuntimeReadinessGate=true를 확인했다. unsupported op·readiness blocker·image/Typeface pending·실패·미등록 fallback·hidden Canvas2D overlay는 0/없음이다. requirePreparedFontFamilies=true였다.
- 실제 bundled HCR Batang Typeface의 bytes는11613312, SHA-256 `b3192018e3bd4da76b29b0530f71c79fb6f465a2b7cd41308bc38f9a4ae09a6c`로 초기화 HTTP200 HANBatang.woff의 읽은 body와 같다. 함초롬바탕 alias와 실제 paint가 만든 family cache는 같은 준비된 Typeface 객체를 가리킨다.
- 그 원래 Typeface의 「 글다단뒤문앞요음중」10문자 getGlyphIDs는 `[5,39894,41146,41150,41594,43278,45876,46186,46370,46823]`로 모두 양수다. 새 Typeface를 만들거나 Canvas2D CSSface/외부 coverage로 대신하지 않았다. 다른 fallback 후보의 cache=null 원값은 보존했으며 실제 glyph 실패로 해석하지 않는다. 다른 크기·style·글꼴·glyph-run/outline 경로의 검증은 미결이다.
- 실제 image cache는1개·1000×523=523000픽셀, pending/실패/eviction0이다. 내부 key의 source는 bin:0:1:src이며 key fingerprint를 SHA-256이라고 표시하지 않는다. 기존 HWPX의 BinData/image1.jpg는81040bytes/SHA-256 `7af06b6410ea950341606e2ffe793f99e680030b39513b4bd835dc5c42822f76`로 moogung.jpg와 같다는 기존 근거를 연결했다. cache 준비만으로 이미지 색/샘플링 정밀 일치를 입증하지 않는다.
- 새 탭 초기 JS body `1e069446d51bc2ef56646ee0a6321dc2610c912f6ca3b6043ec028be781b13aa`, engine WASM `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`, CanvasKit WASM `25ebed8e60158c5854f8dc807b936daca21354f8bfb6a2231266b0a93812f301`, renderer chunk `34f1c749212f8a65ca6f9f636b672f384acb9924628f5dbb157995052d9763b6`가 준비된 실제 최적화 산출물 bytes/hash와 같았다. 사후 fetch나 같은 URL로 대신하지 않았다.
- 중복 HANBatang/RHWPOldHangul HTTP200 사본2개는 진단 API No data found로 body hash를 읽지 못한 원값을 남겼다. 다른 성공 사본의 hash로 이 두 사본을 대신 확정하지 않았다. 준비된 실제 Typeface bytes는 별도로 직접 읽었으며 관측 실패를 실제 network 실패로 표시하지 않는다.

## 실제 화면·독립 Print·남은 범위

- [전체 화면](canvaskit-screen.png)·[page 사각형](canvaskit-page.png)은 raw CDP Page.captureScreenshot이다. 자연스러운 같은 visible 캐럿 phase에서 1초 간격 전체 화면 두 장은 바이트 동일했고 캡처 뒤 DPR1을 확인했다. 글자·그림·가이드·캐럿을 숨기거나 masking하지 않았으며 page error/실패한 network request/dialog는0이다.
- [새 backend의 전쪽 TSV](silhouette.tsv)는 전체1/1쪽·누락0, 공식2px raw 실루엣 **100.00%**다. 100%는 픽셀 동일이 아니다. silhouette_only/not_evaluated를 전체 PR gate 성공으로 바꾸지 않는다. [review](review.png)·[overlay](overlay.png)를 직접 열어 글상자·본문·그림의 존재와 배치를 확인했다. 그림 내부의 색/샘플링 차이는 overlay에 남으며 원인·색 정밀 일치는 미결이다.
- 기존 Poppler26.01.0/96dpi reference PNG SHA-256 `387d24d72a54d2a43686150b7b95a27ab08884917bdf236725955f5aa80aac95`를 공개 초기 producer와 대조해 재사용했다. Studio794×1123/PDF794×1122의 아래 흰 여백1px외 resize/masking/exclusion/threshold 변경이 없다.
- [기존 Native/Canvas2D 독립 검증](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/verification.md)은 같은 Rust 제품·입력의 기존 범위로 재사용했다. 같은 Native export·Canvas2D 초기/9단계·기하/density·제품 빌드/전체 Rust/Studio 검사는 반복하지 않았다. 후보의 검증을 #8349 검토 head의 CI로 표시하지 않는다.
- CanvasKit BehindText 전환·이미지 색/샘플링 정밀 원인·실제 backend 전환·다른 중첩/clip·다른 글꼴/glyph·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·features.md·신규5번 Open PR 조건은 미결이다. #8349 본문은 여유16자 때문에 유지하고 새 링크는 최종 알림/자동화에 남긴다.
