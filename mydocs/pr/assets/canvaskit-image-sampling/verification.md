# #8271 그림 내부 차이의 읽기 전용 분류

**기존 CanvasKit 화면은 평균 색의 큰 이동보다 국소 명암 변화가 크다. source의 screen 축소 정책 차이와 일치하므로 샘플링을 우선 원인 후보로 좁혔다. 실제 draw 인자·디코더 픽셀을 읽은 원인 확정이나 제품 보정은 아니다.**

- 실행 제품은 `3ef5046f42042c6ad50d47e0054e2132467395cd` / engine base `ad4bf269ffef5adf2142b33004a9981fad7ce804`다. 증적 부모 `84048b1decde6d9c7826fe6a792087c1d283b13b` 대비 이 폴더만 추가한다. 제품·검사·입력·reference·threshold·features.md와 #8349 검토/기존 후보 heads는 유지한다.
- 확인한 최신 devel `2d535febe00a61585b6770cb95860fee912bbe54`는 #8144/#8388 제목 표와 저장 흐름·구분선의 메인터너 보정을 포함한다. ad4 이후 분석 대상 이미지 source와 Studio source 변경은 없지만 Rust 조판은 달라 이 기존 실행을 최신 engine 실행으로 표시하지 않는다. 관련 열린 PR의 새 리뷰/미해결/실제 CI 변화는 없었으며 다른 작업 head를 변경하지 않았다.
- 자료는 [CanvasKit 초기 소비](https://github.com/semanticist21/rhwp/blob/84048b1decde6d9c7826fe6a792087c1d283b13b/mydocs/pr/assets/studio-canvaskit-behind/verification.md)와 [Native·Canvas2D·독립 Print](https://github.com/semanticist21/rhwp/blob/3deefcc6937015121e7cd63927ec30dc9f9404a7/mydocs/pr/assets/studio-behind-plane/verification.md)의 같은 1쪽이다. 기존 #8271 편집 API 합성 HWPX와 #8283→#8319/#8328 회귀의 한컴2020 PDF를 재사용했으며 새 입력·Print·screen capture·제품 빌드를 만들지 않았다.
- HWPX SHA-256 `19ac4952c2d2254fce267b950b40dba10cfe607c9c6bf9b6139804dfdbbc7b00`, PDF `fe9dd3caedb3dabc7e38f81de03a3804bbf7430597f9c4fc3c3e86f3ba2e0ab0`와 기존 page PNG bytes를 Git 증적/공개 producer에 대조했다. reference는 기존 Poppler26.01.0/96dpi PNG `387d24d72a54d2a43686150b7b95a27ab08884917bdf236725955f5aa80aac95`다. [측정 원값](measurements.json)에 모든 PNG·입력·source blob·도구 버전과 기존 네 초기 응답 hash를 연결했다. 이번에 새 탭 초기 응답을 관측했다고 하지 않는다.

## 원좌표와 원 JPEG·PDF

- 기존 별도 Node 구조의 image bbox와 imagePaintBBox는 모두 `(133.013,132.267,113.387,59.293)` px, crop `(0,0,75000,39225)` HU / originalSizeHu `(75000,39225)`로 원본 전체다. realPic/brightness0/contrast0·회전/반전 없음이다. 이는 브라우저 글꼴 등록 전 구조이며 실제 drawImageOp 인자의 직접 관측을 대신하지 않는다.
- PDF는 595×841pt, `/Im1` 1000×523·8bit DeviceRGB/DCTDecode다. 실제 `cm [85.034,0,0,44.471,99.786,697.397]` 뒤 `/Im1 Do` 한 번을 읽었다. 위쪽 원점/96dpi로 환산한 그림은 `(133.048,132.176,113.378667,59.294667)` px다. Node와의 x/y/w/h 차이는 `(-0.035,+0.091,+0.008333,-0.001667)` px이며 subpixel phase의 영향까지 배제하지 않는다.
- 원 JPEG는 81040bytes/SHA-256 `7af06b6410ea950341606e2ffe793f99e680030b39513b4bd835dc5c42822f76`, HWPX BinData/image1.jpg와 moogung.jpg가 바이트 동일하다. PDF 안 JPEG는 같은 1000×523이나 35485bytes/`a8786dd100ed9fd140acd638fbec630f56404f75f455cd2335efc7f434849d2a`로 다르다. 원본과 Print가 동일 압축/디코드 RGB라는 가정은 성립하지 않는다.
- 원 JPEG의 524byte ICC `c2` hash는 `61bb24bd075ae1179b562cb484a2737fa63bcf35133a951843db1f1f7c97ce7f`, PDF JPEG에는 ICC가 없다. 설치된 Pillow/LittleCMS의 원본→sRGB 변환은 이 입력 RGB에서 최대 차이0이었다. 원본/PDF JPEG를 같은 Pillow에서 원크기 디코드한 RGB MAE는 채널별 `[4.0900,3.3692,4.5971]`, 평균 차이는 `[+0.03164,-0.02726,-0.03593]`다. 이는 오프라인 관측이며 실제 Chromium/CanvasKit colour management·디코더 픽셀 동일성의 증명이 아니다.

## 고정 ROI 관측

- 좌표는 각 수정하지 않은 page PNG의 `[135,134,245,190)` 즉 110×56=6160px다. geometry에서 내부 사각형을 정해 같은 좌표만 읽었으며 resize/정렬/출력 변경·mask/exclusion·공식 threshold 변경이 없다. [1:1 원픽셀 crop 비교](roi-comparison.png)는 `[130,129,250,195)`의 120×66px를 확대 없이 나열한 보조 그림이다.
- 아래 Y′는 8bit gamma-coded RGB의 `.2126R+.7152G+.0722B`로 물리 선형 휘도가 아니다. 변화량은 이웃 행/열의 `abs(diff(Y′))` 평균이다. [TSV](roi-statistics.tsv)와 JSON의 quantile·RGB std·쌍별 MAE/RMSE·평균 bias가 원값을 소유한다. 이 ROI는 공식 전쪽 gate나 새 정상 reference가 아니다.

| 기존 출력 | 평균 RGB | Y′ 표준편차 | 행 방향 변화 | 열 방향 변화 | PDF 대비 RGB MAE |
| --- | --- | ---: | ---: | ---: | ---: |
| 독립 PDF | 167.611 / 160.548 / 126.235 | 54.435 | 14.605 | 12.173 | 0 |
| Native Print | 167.472 / 159.989 / 125.985 | 54.368 | 14.614 | 12.113 | 2.299 |
| Canvas2D screen | 167.480 / 159.961 / 125.888 | 53.448 | 13.185 | 11.015 | 6.837 |
| CanvasKit screen | 168.466 / 161.199 / 126.816 | 57.584 | 20.094 | 17.184 | 11.348 |

- CanvasKit−PDF 평균 bias는 `[+0.85471,+0.65097,+0.58084]`이지만 국소 변화는 PDF보다 행 37.58%/열 41.16% 크다. “색이 다르다”를 일괄 색 보정으로 해석하지 않는다. 그림 질감과 경계의 세부 차이가 크며 단순 평균 밝기 이동으로 설명되지 않는다.
- 고정 주변 사각형에서 `max(RGB)-min(RGB)>16 && min(RGB)<240`인 색 픽셀 범위는 PDF/Native/CanvasKit `[133,132,247,192)`, Canvas2D `[133,132,246,192)`였다. 이는 경계 진단일 뿐 성공 mask가 아니다. 원픽셀 ±1px 이웃 9조건의 CanvasKit/PDF MAE는 원위치11.348이 최저이고 나머지13.526~23.801이었다. 공식 출력을 이동하지 않았으며 fractional placement·디코드·압축 차이는 남긴다.

## source와 분류의 한계

- [CanvasKit renderImage](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit-renderer.ts#L2301)는 imagePaintBBox를 사용하고 [drawImageOp](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/canvaskit-renderer.ts#L2834)는 공통 crop과 실제 matrix로 source filtering을 계산한다. [image-sampling](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/rhwp-studio/src/view/image-sampling.ts)은 선언된 nearest/linear도 확대에만 적용한다. 현재 자료의 전체 원본 축소는 binary/tone 효과와 구분한다.
- 같은 파일 2860~2907행의 공통 bitmap 사전 축소는 print/highQuality만, mipmap은 compat 모드 또는 print/highQuality만 허용한다. 기존 실제 mode=default/profile=screen은 이 두 정책에서 제외된다. 2998행은 drawImageRectOptions에 filtering과 실제 mipmap 사본 유무를 넘긴다. 이것은 source에서 도출한 경로 후보이며 이번에 그 호출의 인자·matrix·선택된 filter/mipmap을 breakpoint로 관측하지 않았다.
- [공통 bitmap_raster](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/src/renderer/bitmap_raster.rs#L105)는 print/highQuality만 처리하며 Print의 불투명 축소에 면적 적분을 쓴다. [Native 소비](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/src/renderer/skia/image_conv.rs#L477)와 [Canvas2D 소비](https://github.com/semanticist21/rhwp/blob/3ef5046f42042c6ad50d47e0054e2132467395cd/src/renderer/web_canvas.rs#L3192)는 프로필/전처리/축소 품질 경계가 다르다. 이 Native 자료는 Print, 두 브라우저 자료는 screen이므로 같은 sampling 계약의 세 구현이라고 하지 않는다.
- `image-reduction`의 완만한 축소 cubic, `anisotropic-image`의 Print 프레임/혼합 축 확대·축소, #8191/`049965c3a4`의 Print 색 전달 재구성과 `afa58fda1b`의 WMF 격자 보정을 읽었다. 일반 JPEG 양축 약8.82배 축소를 그 특수 경로의 실행 증거로 쓰지 않으며 SVG glyph·crop·확대 선언·공통 Print 계약을 변경할 근거로 쓰지 않는다.
- 결론은 **축소 시 넓은 필터 적용 정책 차이를 우선 후보로 분류**한 것이다. 원본/PDF JPEG 재압축·실제 디코더/색 관리·subpixel phase의 기여는 완전히 분리하지 못했다. 실제 원래 draw 호출과 준비된 이미지 픽셀을 읽는 다음 확인 전에는 공통 기하·DPI·threshold·색을 보정하지 않는다.
- 제품/기준 이미지는 변경하지 않았고 측정 self-check·기존 bytes 불변·source diff를 확인했다. 새 Native export·WASM/Studio build·전체 Rust/Studio gate·전쪽 점수·base-head 하락폭을 실행했다고 하지 않는다. 기존 2px raw 100%는 픽셀 동일이 아니며 silhouette_only/not_evaluated와 features.md 미결을 유지한다.
- CanvasKit BehindText 전환·실제 backend 전환·다른 글꼴/glyph/중첩·물리 Windows·저장/재열기·앱 인쇄·앱 전체 통합·신규5번 Open PR 조건은 미결이다. #8349 본문 65520자/남은16자를 유지하고 새 댓글 없이 이 문서 링크를 최종 알림/자동화에 남긴다.
