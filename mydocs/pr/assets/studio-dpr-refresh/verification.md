# DPR 해상도 알림을 공통 viewport 갱신에 연결

**브라우저가 해상도 변경 알림을 전달했는데 CSS 크기가 같아 raster 갱신이 빠지던 경로를 보정했다. 물리 Windows 모니터 이동·전체 앱 통합의 완료를 뜻하지 않는다.**

- 부모 `ad4bf269ffef5adf2142b33004a9981fad7ce804`, 실제 검증한 code tree `b2251606c62d0b6559865499f807ee28c7785f81`. 제품 변경은 [ViewportManager](../../../../rhwp-studio/src/view/viewport-manager.ts)와 [회귀 4개](../../../../rhwp-studio/tests/viewport-manager-dpr.test.ts)뿐이다. Rust·WASM API·기하·글꼴·reference·threshold·features.md는 바꾸지 않았다.
- 컨테이너 RO와 해상도 change가 기존 같은 setTimeout 예약과 viewport-resize를 소비한다. 현재 DPR의 query를 다시 구독하고 detach 때 listener와 대기 예약을 해제한다. backend별 좌표/굵기 보정은 없다.
- 같은 검사 본문으로 부모는 1 PASS/3 FAIL, 후보는 4 PASS다. 반복 2→3→1.25→1, 정상 RO와의 예약 병합, detach 취소와 container 재연결을 확인했다. [이전 로그](regression-before.txt), [후보·인접 회귀 125개 로그](regression-after.txt). 게시 텍스트의 줄 끝 공백만 제거했다.
- TypeScript·Studio 전체 검사 **2677 PASS/2 정상 skip**·production build, source policy **4099/296**, suite 준비/정책, fmt와 diff 검사가 통과했다. [실제 명령·exit·소요 시간](checks.json). 초기 generated-suite 미준비와 기본 feature snapshot 뒤 Native CLI 선택 문제는 준비를 바로잡고 실제로 재실행했다. 실패한 시도 원값도 남겼다.
- 변경 범위는 [로컬 검증 4.3](https://github.com/edwardkim/rhwp/blob/ad4bf269ffef5adf2142b33004a9981fad7ce804/mydocs/manual/pr_review/local_validation.md#43-변경-범위별-기본-검증)의 Studio-only 행이다. Rust blob·lock·fixture가 같으므로 전체 Rust Clippy/nextest를 새로 통과했다고 하지 않는다. 추가로 Native Skia CLI를 빌드했고 #8040 구역 편집 2개·#5730 기준선 1개, SVG snapshot 4개씩 2회와 golden 불변을 확인했다.
- Native는 Fedora 호스트 CLI이며 YLCC 제출/컨테이너가 아니다. 기존 설치 의존성·공용 review target·Rust 1.93.1/Node 24.19.0/Chrome 156.0.8078.73을 사용했다. 새 도구·외부 서비스·자격증명·개발 서버·데스크톱을 준비하지 않았다.
- 부모의 새 release+**wasm-opt**가 실제 성공했고 pkg/Studio public/dist를 대조했다. Rust 입력이 같은 후보는 같은 최적화 WASM `f5c1452c503bd7789d126a78ff5004e7c8ca282ab08822dbfb9bbd04bc759791`을 재사용하고 후보 TypeScript를 새 production bundle로 빌드했다. 새 탭의 실제 초기 JS/WASM 응답 hash가 각 산출물과 같았다. [source/build·공급·실제 자원 원값](producer.json).

## 실제 Studio 소비와 프로토콜 한계

- 기존 production dist만 loopback static 서버로 제공하고 실제 파일 입력 → CanvasView/PageRenderer를 사용했다. 별도 renderer/canvas·강제 renderPage·toDataURL·제품 상태 setter/dispatchEvent·DEV 전역은 쓰지 않았다.
- 같은 환경에서 부모 4회/16단계는 DPR 단독 변경 때 raw DPR 1·backing 794×1123을 유지했다. 후보는 Canvas2D/CanvasKit × 그림/글자 2입력의 **4회/36단계가 통과**했다. [52단계 TSV](stages.tsv), [브라우저 이벤트 원값](events.tsv).
- 후보는 DPR 1→2→3→1.25→1, 실제 status bar mouse/keyboard의 100%→125%→100%, DPR 2·창 크기 변경 후 전체 복원을 확인했다. 14개 trusted keydown을 각 run에서 읽었다. raw/effective DPR·budget·실제 backing을 함께 기록했으며 예산을 완화하지 않았다.
- **반복 emulation은 CDP device metrics와 screen의 resolution media feature를 함께 설정했다.** metrics만 바꾼 bare-page 진단에서는 DPR/getter와 달리 CSS media 스타일·change 알림이 갱신되지 않았다. [프로토콜 진단](protocol.txt)은 Studio 출력 증거가 아니다. 양쪽 gate에 같은 coherent 설정을 썼고 전달된 알림은 trusted였다. JavaScript 가짜 이벤트를 제품에 보내지 않았다. metrics-only 반복과 물리 OS 전달은 이 통과 범위 밖이다.
- 글자 표본의 24px 초기/원복 차이는 source의 2px×12px, 500ms 캐럿이었다. 이후 모든 raw 캡처를 자연스러운 동일 visible blink phase에서 읽었다. 캐럿·문서·가이드를 숨기거나 pixel masking/exclusion을 적용하지 않았다.
- 페이지 사각형을 브라우저 화면에서 직접 캡처했다. 각 캡처 전후 DPR과 물리 PNG 크기를 확인했다. 125%에서는 종이가 scroll container보다 크므로 전체 page라고 하지 않는다. [복원/배율 단계](stages.tsv)의 full_page_visible을 구분한다.
- Canvas2D 그림 대조군은 같은 page parent의 DOM 그림 2개가 decode 완료됐다. CanvasKit은 source상 같은 그림을 자신의 surface에 replay하며 DOM 그림 0개가 정상이다. 실제 software surface/DOM canvas의 객체 소유·자원 준비를 읽었고 단일 공유 이미지 639×70의 decode/cache를 확인했다.
- CanvasKit은 실제 초기화 chunk 원래 diagnostics 호출의 receiver를 CDP breakpoint로 읽고 제거·재개했다. queryObjects·함수/prototype 교체는 하지 않았다. 실제 화면의 surface와 동일한 canvas, 준비된 Typeface의 bytes/hash·glyph coverage를 확인했다. hidden Canvas2D overlay·readiness blocker·사용 자원 pending/실패·미등록 fallback은 0이다. CSS face 성공을 CanvasKit Typeface 준비로 대신하지 않았다.
- 최종 유효 run의 page error·실패한 network request·열린 dialog는 0이다. 모든 후보 원복 page는 자신의 초기 및 같은 backend/입력의 부모 page와 **바이트 동일**했다. 그림 frame의 배율 정규화 최대 차이는 **0.0125 CSS px**다.

## 독립 Print와 유지한 범위

- [입력·독립 PDF SHA/원위치](inputs.json): 기존 pic-crop-01과 top-probe-shape-5는 각각 1쪽이다. 앞 것은 #7688의 원 해결 Print, 뒤는 과거 #8038 한컴 Print다. 새 원문·reference·golden을 만들지 않았다.
- 공식 동일 blob `ef2ae9f3de3d90a8141bd191d5ba17777d9228a1`의 write_silhouette_tsv, 색 경계 보정 전 **2px raw 지표**다. Native 2개·부모/후보 screen 8개 모두 90% 이상이며 screen 최저 **98.64%**, 부모 대비 하락 0pp다. [전쪽 TSV](silhouette.tsv). 기존 padded_pair의 흰 여백 정렬만 사용했고 resize·가이드 masking·threshold 변경은 없다.
- 각 backend/입력의 부모/후보 DPR 1 page가 바이트 동일하다. Native는 같은 Rust blob을 소비하는 한 producer의 공통 출력이며 별도 두 Native 바이너리 비교로 표시하지 않는다. 이 실루엣 보조값의 silhouette_only/not_evaluated를 전체 PR gate 판정으로 바꾸지 않았다.
- 실제 [Canvas2D 그림 review](canvas2d-pic-review.png)·[글자 review](canvas2d-top-review.png), [CanvasKit 그림 review](canvaskit-pic-review.png)·[글자 review](canvaskit-top-review.png), 각 overlay와 원복/실제 DPR 2 page를 함께 남겼다. Native [그림](native-pic.png)·[글자](native-top.png)도 실제 Skia PNG다.
- #8360 backend surface 해제/SVG glyph raster, #7905 셀 높이 권한, #8196/#8103 Undo/선택, #8377/#8040 구역 편집 및 glyph 변환·겹침 잉크 경계·body clip·decoration replay·resolve_page_phases의 Rust/다른 TS blob은 유지했다.
- 기존 검토 #8349 `ffc0550e8b9c6db8627f9a82cc7f0f3ff39a5415`와 3/4/5번 후보 head를 바꾸지 않는다. 이 부모에 과거 위/아래 기하 보정까지 모두 통합됐다고 가정하지 않는다.
- 작은 글자 잉크 농도·다른 글꼴/회전/세로쓰기/겹침·별도 plane·실제 backend 전환·물리 Windows 모니터 이동·앱 통합·저장/재열기·앱 인쇄는 미결이다. features.md와 신규 5번 Open PR 제출 조건을 완료로 표시하지 않는다.
- 최종 제출 tree와 검증 code tree의 차이는 이 증적 폴더뿐이며 제품·검사·기존 입력 blob이 같음을 별도로 확인한다. 새 PR 생성이나 기존 검토 head의 검증/CI 결과로 표시하지 않는다.
