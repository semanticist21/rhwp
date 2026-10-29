# DPR 단독 전환의 브라우저 이벤트 경계

**DPR 단독 전환은 여전히 미통과다. 이 자료는 기존 e4 실행물의 이벤트 전달을 분류하며 최신 제품이나 실제 Windows 모니터 이동의 결함을 확정하지 않는다.**

- 실행 제품 `e4b792845f51a21a4f655579a1fc652203fb1dc1`, 증적 부모 `e7f44c1160143d81f345413bc350c05999735729`. 준비된 production dist와 기존 `samples/pic-crop-01.hwp` 1쪽을 재사용했다. 원문 SHA-256 `3b5afbe4cdb18faa49452a9cbed770d5caa27aed2f83b708e9ab8516c65ebf4c`다.
- 2026-10-29 UTC, Fedora 호스트 CLI, Chrome `156.0.8078.73`, headless Playwright, 실제 파일 입력 → CanvasView/PageRenderer의 Canvas2D·screen·100%다. YLCC 제출 작업·컨테이너 결과가 아니다.
- 새 탭의 실제 초기 JS 응답 SHA-256 `5f43daf3425c692ed4c456cd240efb5041614a2eb5ef29ce9a4b114857823cf5`, 초기 engine WASM `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`을 확인했다. 기존 5번 producer와 같다.
- 제품 함수·prototype·반환값·상태를 바꾸지 않고 window/visualViewport resize, 해상도 media-query change, 같은 scroll container를 관찰하는 **별도 진단 ResizeObserver**만 등록했다. 제품 ResizeObserver callback 자체를 계측했다고 하지 않는다. 개발 서버·데스크톱·새 빌드·강제 renderPage·별도 renderer/canvas는 쓰지 않았다.
- 진단 observer의 최초 알림을 받은 뒤 안정화하고 단계 경계를 표시했다. CDP `Emulation.setDeviceMetricsOverride`로 브라우저 환경만 바꿨으며 수동 dispatchEvent를 호출하지 않았다. `isTrusted=true`는 브라우저가 전달한 이벤트라는 뜻이며 물리 모니터 이동의 증거는 아니다.

## 관측

| 단계 | window DPR / canvas raw DPR | window resize | visualViewport resize | resolution change | 진단 container RO | canvas backing |
| --- | --- | --- | --- | --- | --- | --- |
| 초기 안정화 뒤 | 1 / 1 | 0 | 0 | 0 | 0 | 794×1123 |
| DPR만 1→2 | **2 / 1** | 0 | 0 | **2** | 0 | **794×1123 유지** |
| DPR 2에서 1440×1600 resize | 2 / 2 | 1 | 1 | 0 | 1 | 1588×2246 |
| DPR 1·1280×1500 복원 | 1 / 1 | 1 | 1 | 2 | 1 | 794×1123 |

- DPR 단독 전환에서 `(resolution: 1dppx)`는 false, `(resolution: 2dppx)`는 true로 각각 바뀌었다. 이벤트의 DPR은 2이며 둘 다 trusted였다. 컨테이너 CSS 크기는 1260×1338로 유지됐다. resize 뒤에는 1420×1438이 됐고 RO가 전달됐다.
- [원 이벤트 TSV](events.tsv)는 최초 RO를 포함한 11개 전달과 시간·단계·DPR·CSS 크기·trusted를 보존한다. [단계 TSV](stages.tsv)는 DPR 단독 단계의 `raw_dpr_matches_window=0`을 그대로 남긴다. `allChecksPassed=false`이며 진단 실행 완료를 제품 통과로 바꾸지 않는다.
- 각 raw CDP `Page.captureScreenshot` 전후 DPR이 같고 PNG 물리 크기는 CSS viewport×DPR였다. DPR 단독 화면은 2560×3000, resize 뒤 화면은 2880×3200이었다. 캡처 wrapper의 emulation 재설정과 구분했다.
- 실제 DOM 그림 2개는 모든 단계에서 decode 완료·같은 page parent였다. 초기/복원 page PNG는 이전 [복원 대조군](../decoration-image-transitions/all-restored-page-1.png)과 바이트 동일하며 SHA-256 `eb2e5a16a481af1f6d3500a58d4ca65dc9cb578b6fda3138c438a31018a6c028`이다. page error·실패한 network request·열린 dialog는 0개다.

## 소스와 판정 범위

- 실행 e4와 확인한 최신 devel `1cb3a6dae1094f6ed552a8fdbffea8b89868d555`의 `viewport-manager.ts`는 같은 Git blob `b293ed8efdcd0913fee534ab236c029905d6353b`다. [attachTo](https://github.com/edwardkim/rhwp/blob/1cb3a6dae1094f6ed552a8fdbffea8b89868d555/rhwp-studio/src/view/viewport-manager.ts#L69)는 컨테이너 RO → setTimeout → viewport-resize를 연결한다. 읽은 manager/main/CanvasView에는 해상도 media-query change를 이 경로로 연결하는 관찰이 없었다.
- 최신 CanvasView blob `85fd20d071f226e3e341dccab09712c8064e6329`, main blob `59d7d6e11707d56d6e76f7c26f0a7bfbf70f4b73`는 실행 e4와 다르다. 최신 commit은 #8196/#8103의 Undo 선택 배치·CanvasKit 전환 경합 보정을 포함한다. **이 진단은 그 최신 bundle을 실행하지 않았다.**
- 관측과 소스를 함께 보면 e4는 CSS 크기가 같은 DPR 변경에서 관찰 중인 container RO를 받지 못했고 새 해상도 raster를 만들지 않은 것으로 해석할 수 있다. 브라우저의 해상도 변화 신호 자체가 전혀 없었다는 설명과는 구분된다. 제품 callback 미호출은 직접 breakpoint 관측이 아닌 이 경로에 대한 추론이다.
- [#6041](https://github.com/edwardkim/rhwp/issues/6041)과 원 해결 [#6467](https://github.com/edwardkim/rhwp/pull/6467), merge `49525b299c44f7e7c985ed8ffc546e6059fff9f7`의 raw DPR/예산 계약은 유지한다. 화면을 맞추려 surface 예산·공통 기하·threshold를 바꾸지 않았다.
- 최신 source/build에서의 재현과 필요한 보정, 실제 OS 모니터 이동, 별도 plane·CanvasKit 전환·Windows UI·저장/재열기·앱 인쇄는 미검증이다. 최신 backend 수명·SVG glyph raster·셀 높이 권한 및 glyph 변환·겹침 잉크 경계·body clip·decoration replay·resolve_page_phases를 보존해야 한다.
- 제품·테스트·reference·features.md·검토/후보 head를 유지한 증적 전용 변경이다. 새 제품 빌드·전체 Rust/WASM 검사·신규 Open PR 제출·전체 기능 완료로 표시하지 않는다.
