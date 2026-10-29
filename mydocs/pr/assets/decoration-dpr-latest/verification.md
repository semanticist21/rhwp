# 최신 devel production의 DPR 단독 전환 재현

**확인한 최신 devel의 새 최적화 빌드에서도 DPR 단독 변경의 main canvas 갱신은 미통과다. 제품 보정·전체 기능 완료 결과가 아니다.**

- 제품/증적 부모 `1cb3a6dae1094f6ed552a8fdbffea8b89868d555`, tree `3a8ee1bb9aded4e206dfd188d310af639cf2f2b5`다. 기존 e4와 별도인 격리 worktree를 빌드했다. 기존 후보·검토 head와 e4 prepared dist는 유지했다. 이 parent는 #8360·#7905·#8196/#8103 통합 이후 source다.
- Fedora 호스트 CLI의 설치된 Rust/Node/wasm-pack/Chrome을 사용했다. YLCC 제출 작업·컨테이너가 아니며 새 도구·외부 서비스·자격증명·개발 서버·데스크톱을 준비하지 않았다.
- `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, 기존 공용 review target에서 `bash scripts/wasm-pack-locked.sh --target web --out-dir pkg`를 실행했다. 새 source 경로의 rhwp 0.9.0 컴파일과 **wasm-opt 종료 성공**을 로그로 확인했다. 이어 `npm --prefix rhwp-studio run build`의 tsc·production bundle 생성이 성공했다.
- Studio lock의 비루트 package entry와 dependency 선언은 e4와 모두 같고 lock root의 version만 0.8.7→0.9.0이다. 이를 대조한 뒤 기존 설치 의존성만 재사용했다. 변경된 source를 구 bundle로 대신하지 않았다.
- [producer 원값](producer.json)에 source/tree·도구 버전·실제 명령/소요 시간·pkg/public/dist bytes와 SHA-256을 기록했다. pkg와 Studio/public의 glue/WASM이 같고 hashed dist WASM도 새 package와 같다. 실제 새 탭 초기 JS 응답 `865b00c164eef986f2a29b12cd323380339ff281ecc2d387fbf840c9a2bd3f7e`, engine WASM 응답 `4427af63857a12d23148858d9ee3aecdf82fc78f446bc909c36871535a7d1f12`을 빌드 산출물과 대조했다. engine WASM은 기존 e4 hash와 다르다.
- 준비한 production dist만 loopback static 서버로 제공했다. 기존 `samples/pic-crop-01.hwp` 1쪽의 실제 파일 입력 → CanvasView/PageRenderer, 요청/선택 Canvas2D·screen·100%를 확인했다. 원문 SHA-256 `3b5afbe4cdb18faa49452a9cbed770d5caa27aed2f83b708e9ab8516c65ebf4c`이다.

## 이벤트와 실제 화면

| 단계 | window DPR / 기록된 raw DPR | resolution change | window / visualViewport resize | 진단 container RO | main backing |
| --- | --- | --- | --- | --- | --- |
| 초기 안정화 뒤 | 1 / 1 | 0 | 0 / 0 | 0 | 794×1123 |
| DPR만 1→2 | **2 / 1** | 2 | 0 / 0 | 0 | **794×1123 유지** |
| DPR 2에서 1440×1600 resize | 2 / 2 | 0 | 1 / 1 | 1 | 1588×2246 |
| DPR 1·1280×1500 복원 | 1 / 1 | 2 | 1 / 1 | 1 | 794×1123 |

- [단계 TSV](stages.tsv)와 [원 이벤트 TSV](events.tsv)는 실패를 그대로 보존한다. 실제 원값의 `allChecksPassed=false`를 실행 성공으로 덮어쓰지 않았다. 자동 회귀/golden이나 전쪽 실루엣 gate를 새로 통과했다는 뜻이 아니다.
- 제품 메서드·prototype·반환값·상태를 바꾸지 않고 브라우저 resize/media-query 이벤트와 같은 컨테이너의 **별도 진단 RO**를 읽었다. 진단 초기 RO가 도착한 뒤 단계 경계를 표시했다. 제품 RO callback 자체를 직접 계측했다고 하지 않는다. trusted 이벤트는 물리 Windows 모니터 이동의 증거가 아니다.
- CDP는 DPR/viewport 브라우저 환경만 변경했다. raw Page.captureScreenshot 전후 DPR과 실제 PNG 크기=CSS viewport×DPR를 확인했다. 별도 canvas/renderer·강제 renderPage·toDataURL·제품 상태 setter/dispatchEvent를 쓰지 않았다.
- [DPR 단독 단계 실제 page](dpr2-only-page.png)는 전체 화면의 해당 page 사각형을 직접 캡처해 DOM 그림 합성을 포함한다. 각 단계에서 그림 2개는 decode 완료·같은 page 소유였고 page error·실패한 network request·열린 dialog는 0개다.
- [복원 page](all-restored-page.png)는 이 최신 실행물의 초기 page와 바이트 동일하다. 이전 e4 정상 대조군과의 page bytes 동일 여부는 producer의 `page_bytes_equal_previous_e4_control=true`로 기록한다. DPR PNG를 축소하거나 reference/threshold를 바꾸지 않았다.

## 원인 범위와 미결

- 빌드 중 devel은 #8377/#8040 통합의 `ad4bf269ffef5adf2142b33004a9981fad7ce804`로 진전했다. 비교한 manager/main/CanvasView 3파일은 1cb와 같지만 Rust의 rendering query·composition·표/단락 조판 등이 바뀌었다. **이 자료는 ad4의 새 engine 실행 결과가 아니다.** 다음 보정은 그 통합과 독립 Print 기반 구역 편집 회귀를 보존해야 한다.
- 최신 `ViewportManager.attachTo`의 컨테이너 RO → setTimeout → viewport-resize와 CanvasView의 raw DPR 소비 경로를 대조했다. 이 source의 manager blob은 `b293ed8efdcd0913fee534ab236c029905d6353b`이며 해상도 media-query를 공통 갱신 경로에 연결하는 관찰이 없다. DPR 단독 단계에서 브라우저 신호는 있었지만 관찰하던 CSS resize는 없었다는 실행 결과와 연결된다.
- [#6041](https://github.com/edwardkim/rhwp/issues/6041)의 raw DPR/예산 계약과 원 해결 [#6467](https://github.com/edwardkim/rhwp/pull/6467), merge `49525b299c44f7e7c985ed8ffc546e6059fff9f7`을 유지한다. 이 자료는 공통 viewport 경로 보정의 재현 근거이며 예산·공통 기하·threshold를 바꾸는 근거가 아니다.
- 제품 코드는 아직 보정하지 않았다. DPR 관찰의 연결·분리와 반복 전환 회귀, Canvas2D/CanvasKit 실제 소비, 필요한 전체 검증은 다음 보정 단위에 남는다. 물리 Windows 모니터 이동·별도 plane·저장/재열기·앱 인쇄도 미검증이다.
- 이 branch는 parent 대비 증적만 추가한다. 기존 검토 #8349 head·3/4/5번 후보·features.md를 유지하며 최신 source에 과거 기하 보정까지 모두 통합됐다고 가정하지 않는다. 새 Native 전체 nextest·전체 Studio screen/Print gate·신규 5번 Open PR 제출 완료로 표시하지 않는다.
