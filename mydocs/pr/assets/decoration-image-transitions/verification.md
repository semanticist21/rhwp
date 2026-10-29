# DOM 그림 배율·창 크기 복원과 DPR 단독 전환 관측

**배율·창 크기 복원은 확인했지만 DPR 단독 전환의 main canvas 갱신은 확인하지 못했다. 전체 조건 통과가 아니다.**

- 제품 source는 기존 `e4b792845f51a21a4f655579a1fc652203fb1dc1`, 증적 부모는 `f360464c1d3ba2df09354d9113ec397f03dd9437`다. 준비된 production `dist/`와 기존 `samples/pic-crop-01.hwp` 1쪽을 재사용했다. 원문 SHA-256 `3b5afbe4cdb18faa49452a9cbed770d5caa27aed2f83b708e9ab8516c65ebf4c`다.
- 2026-10-29 UTC, Fedora 호스트 CLI, Node 24.19.0, Chrome `156.0.8078.73`, headless Playwright, 요청/선택 Canvas2D·screen이다. 실제 Windows 모니터 이동이나 최신 devel 제품 실행을 측정한 결과는 아니다.
- 기존 production의 실제 파일 입력 → CanvasView/PageRenderer를 사용했다. 개발 서버·새 빌드·데스크톱·별도 renderer/canvas·강제 renderPage·제품 상태 setter/dispatchEvent를 쓰지 않았다.
- 실제 status bar range를 mouse로 클릭하고 `PageUp×2, ArrowUp×5`로 125%, `PageDown×2, ArrowDown×5`로 100%를 복원했다. 관측한 14개 keydown은 모두 `isTrusted=true`이며 실제 UI 값과 rendered zoom을 함께 읽었다.
- DPR/viewport는 CDP `Emulation.setDeviceMetricsOverride`로 브라우저 환경만 변경했다. raw `Page.captureScreenshot`을 사용했고 각 캡처 전후 `window.devicePixelRatio`가 같으며 실제 PNG 물리 크기가 viewport CSS 크기×DPR인지 확인했다. 캡처 wrapper가 DPR을 다시 설정하는 경우와 구분했다.

## 여섯 단계

| 단계 | zoom | CSS viewport | window DPR / 기록된 raw DPR | main canvas backing |
| --- | --- | --- | --- | --- |
| 초기 | 100% | 1280×1500 | 1 / 1 | 794×1123 |
| 실제 UI 확대 | 125% | 1280×1500 | 1 / 1 | 993×1404 |
| UI 배율 복원 | 100% | 1280×1500 | 1 / 1 | 794×1123 |
| DPR만 변경 | 100% | 1280×1500 | **2 / 1** | **794×1123 유지** |
| DPR 2에서 창 크기 변경 | 100% | 1440×1600 | 2 / 2 | 1588×2246 |
| DPR·창 크기 모두 복원 | 100% | 1280×1500 | 1 / 1 | 794×1123 |

- [단계별 TSV](transitions.tsv)는 `raw_dpr_matches_window=0`인 DPR 단독 전환을 그대로 기록한다. 실행 완료와 `allChecksPassed=false`를 구분하며 실패를 성공으로 바꾸지 않았다. 새 실루엣 TSV나 regression golden이 아니다.
- 매 단계에서 실제 DOM 그림은 2개이고 모두 decode 완료·같은 page parent·page 0 소유였다. 125%의 frame 좌표/크기를 1.25로 나눈 값과 초기 값의 최대 차이는 0.0125 CSS px였다. 다른 DPR 단계에서는 같은 CSS frame 좌표/크기를 유지했다.
- [125% 실제 화면](zoom125-screen.png)과 [그림 영역](zoom125-images.png)을 직접 확인했다. 125%에서는 paper가 scroll container의 가시 영역보다 크므로 전체 page 캡처라고 하지 않는다. 두 그림의 실제 보이는 영역을 캡처했다.
- [DPR 단독 변경](dpr2-only-images.png)과 [DPR 2에서 resize 후](dpr2-resized-images.png)의 실제 그림 영역을 직접 열었다. 두 PNG는 물리 DPR 2 캡처다. window DPR 2는 유지됐지만 resize 이전 main backing이 갱신되지 않았다는 관측과 DOM 그림 자체의 누락을 구분한다. 그림 두 개는 계속 표시됐다.
- [최종 복원 페이지](all-restored-page-1.png)는 초기 및 UI 배율 복원 페이지와 **바이트 동일**하다. SHA-256 `eb2e5a16a481af1f6d3500a58d4ca65dc9cb578b6fda3138c438a31018a6c028`이며 [이전 DOM 대조군의 page](../decoration-dom-image-screen/canvas2d-page-1.png) bytes와도 같다. scroll top/left도 초기와 최종 모두 0이다.
- 이전 [독립 Print 대조](../decoration-dom-image-screen/verification.md)의 같은 page bytes를 재사용할 수 있지만 새 DPI sweep이나 새 픽셀 동일 증거로 표시하지 않는다. 공식 reference·threshold를 바꾸거나 DPR 2 PNG를 임의로 축소해 정상 기준으로 쓰지 않았다.
- page error·실패한 network request는 0개이고 단계별 열린 dialog도 0개였다. 초기 engine WASM 응답 hash `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`은 기존 5번 producer와 같다. 초기 실제 JS 응답 hash는 `5f43daf3425c692ed4c456cd240efb5041614a2eb5ef29ce9a4b114857823cf5`다.

## 원 해결·소스 대조와 남은 범위

- [#6041의 raw DPR/예산 계약](https://github.com/edwardkim/rhwp/issues/6041)과 원 해결 [#6467](https://github.com/edwardkim/rhwp/pull/6467), merge `49525b299c44f7e7c985ed8ffc546e6059fff9f7`을 먼저 확인했다. CSS 표시 배율과 내부 raster 해상도는 별개이며 이 자료만으로 기존 surface 예산 정책을 없애지 않는다.
- 실행 제품의 `ViewportManager.attachTo`는 ResizeObserver에서 `viewport-resize`를 내고 `CanvasView`가 그 event로 render plan과 raster를 갱신한다. 실제 읽은 두 진입 파일에는 DPR 전용 matchMedia 관찰이 없었다. 이 경로는 DPR 단독 emulation의 backing 미갱신과 resize 뒤 갱신을 설명하지만 실제 OS 이벤트까지 입증하지 않는다.
- 최신 devel은 `6a3cb5a9c194366169a9bb11ee75b6addfa2e8c4`이며 #7905 통합을 포함한다. `viewport-manager.ts` blob `b293ed8efdcd0913fee534ab236c029905d6353b`는 실행 e4와 같았지만 `canvas-view.ts`와 `main.ts`는 달랐다. 이 증적은 최신 통합본의 실제 재현·보정 결과가 아니다. 기존 backend 전환 surface 해제·SVG glyph raster·조판 보정을 보존하며 최신 제품에서 원인과 필요성을 더 확인해야 한다.
- DPR 단독 전환을 통과·완료로 표시하지 않는다. 독립 Print의 위치·굵기 문제로 단정해 공통 기하나 threshold를 바꾸지 않았고 새로운 DPR 수정/전체 Rust·WASM 검사 결과도 만들지 않았다. BehindText/InFrontOfText plane·이 그림 입력의 CanvasKit 전환·Windows UI·저장/재열기·앱 인쇄도 미검증이다.
- 제품·테스트·원문·reference·features.md·검토 PR #8349와 기존 후보 head는 유지하고 증적만 추가했다. 별도 신규 Open PR 제출 조건을 완료했다고 표시하지 않는다.
