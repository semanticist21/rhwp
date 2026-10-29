# 아래 긴 파선의 독립 Print 검증

2026-10-29 측정. upstream/devel `f291d772a105d46a4d4d7f40ed13e82391ebdc1a`에 아래 5번만 보정했다. #8349와 별도 4번 후보의 head를 바꾸지 않았으며 3·4번 보정을 통합했다고 주장하지 않는다.
이전 #8038용 합성 프로브의 실제 HWP 저장본과 한컴 2024 13.0.0.4132 / HancomPDF 1.3.0.550의 기존 Print PDF를 재사용했다. 20개 HWP와 20개 PDF 모두 각 1쪽이며 동일 번호 전체 쪽을 96dpi·Print·2024 호환·DPR 1로 대조했다. Native는 Fedora 호스트 CLI이며 YLCC 제출 작업·컨테이너 결과가 아니다.
`visual_sweep.write_silhouette_tsv`(script SHA-256 `08dc51bc238330d254d773de3b5479ec662d2318353e49d8e4bce204b1c30ccb`, threshold_boundary_color_support_v1)의 색 경계 보정 전 2px raw 지표: 60개 결과 최저 92.3858%; 90% 미만·2pp 초과 하락 0개. 대상 shape-5 1쪽 Native/fresh WASM/실제 CanvasKitLayerRenderer software Print는 각 100%. 픽셀 동일을 뜻하지 않는다. Native/Canvas2D/CanvasKit의 19개 대조 문서 PNG와 Native SVG는 각각 바이트 동일하고, 대상 변화는 파란 밑줄에 한정된다. 글자·위 밑줄·clip·cursor가 유지된다.
9~48pt 획 수는 [5,6,6,6,6,6,6,6]으로 독립 Print와 일치한다. 벡터 끝점 최대 오차 0.502095238px, y 최대 오차 0.958666667px. 48pt 기준본은 22획이었다. 원문 run 경계의 위상을 SVG와 layer JSON에서 함께 검사한다.
재현: `RHWP_FONT_PATH=<허가된 글꼴 경로> cargo run --locked --profile release-test --features native-skia --bin rhwp -- export-png <HWP> --output <OUT> --font-path <허가된 글꼴 경로> --scale 1 --profile print --compat 2024 --json`; `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 bash scripts/wasm-pack-locked.sh --target web --out-dir <PKG>`(release·실제 wasm-opt), 새 package의 Canvas2D와 Studio CanvasKitLayerRenderer software surface를 각각 실행. PDF는 `pdftoppm -r 96 -png`로 raster화했다. Chrome 156.0.8078.73에서 기준본과 후보를 동일 조건으로 캡처했다.
검증 명령·실제 종료 코드·통과/skip 수: Engine/Print 후보 범위의 30개 명령이 모두 exit 0. cargo fmt --all -- --check; native·WASM32·workspace all-target Clippy(-D warnings); cargo build --workspace; source policy 4099개/296모듈·suite/manifest 정책; 관련 회귀와 기본 글꼴 검색; SVG snapshot 4개씩 2회(golden 불변); cargo nextest run --locked --cargo-profile release-test --tests --test-threads 4 --no-fail-fast는 11909개 통과/45개 정상 skip. cargo test --features native-skia --lib 합계 4040개 통과/12개 ignored; 누락 그림 2개·직접 PDF 14개 통과. Studio tsc·npm test(2632개 통과/2개 정상 skip)·production build 통과. 동일 회귀가 base 제품 소스에서 8 대 독립 6.876으로 1개 FAIL, 후보에서 1개 PASS이며 기본 글꼴 검색에서도 PASS.
Native Git diff patch SHA-256 `25faf7291c7dbb7351164c4b8e8a80c9f6dedaa72c41481393263eb236d3de9f`; full-index patch SHA-256 `6393a50fda1b466b4082f40281b5f9208a0e0f1c5dc771f551c84251476a0dd1`; 후보 Native `203bcae3e1d543491b0db31cc0a33bc41f7a2ee5aa9a1c04b5ee37d000fc6d51`, WASM `290c17ab16f5126e11533fa7ce1bb72036198a8e21e5ed1e4a43e43d9e95a8f1`; 기준 Native `2b8efd12b6098de57e71afcb2df2d6ceaeb033c2ce6d3ccbb0682bcbbe2dd557`, WASM `4f34b31f148bf9f8627ae51fa7055559de8f95d9ca09f73103637b2e1f06248d`.
독립 Print 시각 환경의 글꼴 SHA-256: HANBatang.ttf `c5d544769d832477d48b7318222b9c3c2eced25455f7ba9acb23060dcecac104`; arial.ttf `b3658eadae55e682b5f69eb64c439c1ecc8f196c0bb8d4756d145d13bc86476a`; HANDotum.ttf `1d0437906d52e96af36943008f144ad74d6e994493df3a4210a9761e077ea898`. 글꼴 바이너리는 첨부하지 않는다. 회귀는 em 치수·상대 위상으로 검사하며 시스템 특정 글꼴에 고정하지 않는다.
원본 HWP는 `../raised-decoration-hancom/`, 기준 PDF는 `../../../../pdf/raised-decoration-hancom/`에 있다. 아래 SHA-256은 HWP / PDF 순서다.

| 문서 | HWP SHA-256 | PDF SHA-256 |
| --- | --- | --- |
| body-emphasis-36pt | `a386956956b17c7b288ae2f637450fd7ffec248cb4f5960c20717fa8e76eeb85` | `01dd10caa0d00464799e0643f5bcbc76c5b609eec22e533d72e7b189616b3ea0` |
| body-top-36pt | `2964eacbf6143a7ef9767225f9b334e7d8372786d598f9b9de77c7c84cc5b248` | `418f121161d31be875783fd62795f291e5add074ec9ce5d88ff3cf6e274785ec` |
| cell-top-10-36pt | `c5e77e2d42a30b5b06c5d351f9be051d4f24624d8af174d7e780bb156431448e` | `9c6292c9db84a3d26e65a8bdb1de357dfb65ba193d5b61add9e082e1422a7a9b` |
| emphasis-probe-1 | `99685bf70ae34c540cc2c225d3b0086f34754143bcaf0edf8b98e9f43eeb5378` | `4b93ceb6e431f7da5d9c268ab2ae718bf3cfe95cdc0e3aed55c42bf45d90981f` |
| emphasis-probe-2 | `725958e63073d9faf3d3c60d72eced19d489e869007a4a18e982e249ed2383dc` | `891ac87e371ec84b8808a15c6e5e12a1e454c3882081c4926229972e75654f1e` |
| emphasis-probe-3 | `682ac2c7e45c45c608b392f9956835098537b5fbb3e19a5a5fafe2edfa757690` | `394a638937221705249edd457855007d81508419dd48939b2e187ed60feff8a1` |
| emphasis-probe-4 | `00e8a986d8c2a3349c629343a47fa81d5b869ac019b71cd9cb81d8e74ea0aa6d` | `9bee2226c21834489066662f8070436723afe1276d133b7d8d9c5dc30838408d` |
| emphasis-probe-5 | `43329ca0ab4ea7e09fbbaa43805a130f07958c802dd399b53d78bcf09a4ae1c1` | `f03f1e88763792b1073535b5abcdfc773079daefb4a79824281891f8af882b0a` |
| emphasis-probe-6 | `de70385adaac4ec5aefe3eed25dd9f6fc914be799844698748cd79914c4d6be4` | `a9377ed2cfdfa0901ef3b18054fd3662fb9d8d59e17940a3790f06163b1ad159` |
| top-probe-shape-0 | `63b12b22e5057f636d86440de2f11d3476ae7c2ef1de0ec58a8bd796152ca7a5` | `d069300bbf7da2899e63023bae5354abc94b6edefb1be92c34ae74d18c2b0a94` |
| top-probe-shape-1 | `b55cec7fb6974a2ee2592baf681e8be6b33ff6162809d373a3e3fde0934eb5c3` | `18f80bad905b6108c09916ed6ca197e5370d751c09a464cd6c3bad1c64822b30` |
| top-probe-shape-10 | `7de89153c2a9dc90acf8eae2767f0cd9fde6445fef63c42a655efb764a88d06c` | `4652346c5db07fab1564a90138ae69b03c3e12af0c3153d2f4054ec9ebadfa85` |
| top-probe-shape-2 | `b9c4583a8325367624d8ebd8818dc2ad9efc6d278feee5e35fc751ad94021223` | `0742ea0f7e9164f0b59e02648de8855db048214f629d1303eda0d989f4407e5d` |
| top-probe-shape-3 | `966c229e3a1e0686bff63bcf5f6699450f168eba0229c98802235bf93371b5d5` | `829148d46d65fa7cf524034b87781ccfef61a5950a8bada859c749ab71544951` |
| top-probe-shape-4 | `d29b4ef3d8f5210258e20a45639115bd7d82c66bc45a13e1189010bfb654d7ef` | `1fac76ad1f2dcd2076882850c77006eff15a12fa907e4259e10c4ed19611bb41` |
| top-probe-shape-5 | `deade69da52406b917d05a766744df63c49dcd187e783963d64afbfdc926325e` | `f9a1d4e0f2071730269f1f9360ee31d5c13b3d79591761b398581a7c5029da25` |
| top-probe-shape-6 | `647f25a29d8650a939dbd78534c393570cef915ab3df219311f82c5b6f00750c` | `6da0f1e55cb39cf7c334adba69b43e67dc9d8f6167c10b56f5d7b4788718a460` |
| top-probe-shape-7 | `95dadeb501d80d158d4514c6990a9d17a25b5841e328cb42ba022f735883e27b` | `97a966259c1da9034c4967aae9e0abe79339b76034a96570c67c2440c1122a0e` |
| top-probe-shape-8 | `1dfcb9ca9d581800a1618b2a389321151a1aa96cc0ea46fe24358c96b5fbd544` | `095dc7fea0d5ab769d37d8c9d6fb528299cbecfeac7127f710479fbb5a69f2bc` |
| top-probe-shape-9 | `12dbe1066237eec3b9cf33d539c0ed7bf70729602196509b5730f904005c6051` | `80096764f575e3e2292cee06c7c3595be72a00d08cffd47843569ef3367d6c22` |

전쪽 TSV: [Native](native-silhouette.tsv) · [fresh WASM](wasm-silhouette.tsv) · [CanvasKit](canvaskit-silhouette.tsv). 기준본도 [Native](base-native-silhouette.tsv) · [WASM](base-wasm-silhouette.tsv) · [CanvasKit](base-canvaskit-silhouette.tsv). [전후 차이](baseline-vs-candidate.tsv).
최종 판독 패널: [Native review](native-review.png) · [WASM review](wasm-review.png) · [CanvasKit review](canvaskit-review.png); [Native overlay](native-overlay.png) · [WASM overlay](wasm-overlay.png) · [CanvasKit overlay](canvaskit-overlay.png); [48pt 전후](comparison-48pt.png) · [9pt 잉크 확대](small-ink-9pt.png).
잔여 한계: 작은 글자 잉크 농도·다른 글꼴·회전·세로쓰기·글자겹침·앱 통합·실제 UI·저장/재열기·앱 인쇄는 미결이다. 9pt 파란 잉크 영역의 임계값 [5,10,20,30,40,60]별 픽셀 수: native [71, 68, 35, 33, 32, 30]; wasm [71, 68, 35, 33, 32, 30]; canvaskit [71, 68, 35, 33, 32, 30]; Print [34, 33, 33, 33, 33, 33]. 이 픽셀 관측을 실루엣 100%와 혼동하지 않는다.
Merge 후 contributor PR comment 계획: 실제 Studio screen 필수 검증과 후속 PR 제출 조건을 충족하여 통합된 뒤에만 이 문서의 8개 최종 PNG와 7개 TSV·20 HWP·20 원본 PDF를 `<merge-commit-sha>`에 고정한 `https://raw.githubusercontent.com/edwardkim/rhwp/<merge-commit-sha>/mydocs/pr/assets/bottom-long-dash-hancom/<file>` 및 원본 폴더 링크로 사용한다. 최종 패널과 미결 한계를 함께 게시하고, UTF-8 본문·이미지 raw·API 재조회로 검증한다. 중간 SVG/JSON/raster와 실행 로그는 커밋하지 않는다.

검사 소스 tree `4f4ca2be5b0159cb19250df3bf69c76eb622835e`를 고정했다. 최종 제출과의 차이는 이 검증 MD 한 경로뿐이고 제품·테스트·입력 blob은 같다. Rust 1.93.1·Node 24.19.0·nextest 0.9.140의 Fedora 호스트 CLI 결과다.
실제 Studio CanvasView/PageRenderer 전체 screen은 [실제 소비 경로 규칙](../../../manual/verification/renderer_backend_verification.md)에 따라 미검증이며 Print consumer로 대체했다고 주장하지 않는다. 신규 Open PR 전체 검증 완료로 표시하지 않고 #8349와 분리한 후보로 연결한다. 앱 기능 체크상태는 유지한다.
