# PR #8380 리뷰 — fix(pdf-import): 본문을 가리지 않는 그림 때문에 문단을 위치 고정 낱말로 남기지 않는다 (#8362, #8375 위)

## 최종 판정

**머지 보류 — 재체리픽 후보의 검증 진행 중.** 최신 base의 기능 보존, 관련 검증과 기존 정밀 TSV 하락 해소를 완료한 뒤 다시 판정한다.

## 접수와 적용

- 원 PR: [#8380](https://github.com/edwardkim/rhwp/pull/8380), 작성자 `planet6897`, reviewer `jangster77`.
- 원 PR head 조회값: `0d0e9e88de6fa9bf66f17f25963e6b9cf7f28ba9`. 조회 시 원 PR head와 마지막 체리픽 원본이 일치한다.
- 마지막 적용 원본: `0d0e9e88de6fa9bf66f17f25963e6b9cf7f28ba9`. 검증 시작 당시 코드 후보: `abea5b028f8f135bdf000ca700e1993ebf0b1973`, 기준 `upstream/devel`: `bc9dfdd057258de59afebeb5121c4bee7398e07f`.
- 관련 이슈: [#8362](https://github.com/edwardkim/rhwp/issues/8362).
- 선행 PR: #8365·#8375.

후속 `e1f03ffbb3`은 Python 테스트 준비, `af175315e7`과 `e920cba6b0`은 회귀 검사 주석, `a63bb719de`는 검증 경과를 기록했다. `5d1294811b`은 전체 그림의 잘림 오판을 보정했다. `e1b8ab0f54`, `d6fe30c3ad`, `0897582f6c`, `8c4eca265e`에 Native 진단, #8410 집중 6건 통과와 글꼴 대조를 기록했으며 새 코드의 전쪽 출력 검증이 필요하다.

## 변경 주장과 검토 관점

본문을 가리지 않는 PDF 그림을 이유로 문단을 위치 고정 낱말로 남기지 않는다.
선행 #8365·#8375와 그림 겹침·paint 순서 및 뒤 본문을 함께 확인한다.

## 체리픽과 보정

- 원본 `a19fbc662055adb6ee864141efa48db6b022e310` → 통합 `a63b516027e9f0e396b1f31bac1cd4c173a3ae90`.
- 원본 `7f9ae2bebc3ff66648512e42899702320b2e0cac` → 통합 `686fcc33f0ff9484235b44844521c0bf10b439bb`.
- 원본 `f05ecea19b1490c91179d9738204b896c6fe5b86` → 통합 `3d909c41953c40a3f5941c5f2103d889b2236f31`.
- 원본 `735304f879d269f9c86b3283a17aa73f3cd981c0` → 통합 `d8f611ce4d3929342a82475afbbf6e9795fa7a1f`.
- 원본 `dca4739bcf59b4a96e0633d485a878ffcb4283c0` → 통합 `31e627bfb96860cd68a7dccc059a436e3e01142a`.
- 원본 `5e42900f52786cf6d3b075499d660fe02ca2f7e5` → 통합 `64f6faa89c91496b00e0f352e25300fa373e29f6`.
- 원본 `c463bbc89201ec2b8541cec9665f1a2cc3601815` → 통합 `251d271f7fa15d7a25df958ccf7fa065454e1dfb`.
- 원본 `908093912d5dec9eb4b01ae1d7c268666a0b02ae` → 통합 `0f8751e97a2ac50be7f0f367fe30ac9b1fdc846e`.
- 원본 `0d0e9e88de6fa9bf66f17f25963e6b9cf7f28ba9` → 통합 `0efd5e97f5dbdbec16cdec3718f0761244dbdf60`.

이 PR의 원본 커밋 적용 중 별도로 기록된 충돌은 없다.

## 검증과 증거

이전 통합 후보의 최소 글꼴 전체 회귀 로그에서 이 PR 관련 다음 모듈의 실행을 확인했다. 새 base 후보의 재실행 결과는 이 값으로 대신하지 않는다.
- [`issue_8362_pdf_editable_hancom_rows.rs`](../../../tests/cases/issue_8362_pdf_editable_hancom_rows.rs): 이전 실행 2건 통과, 0건 실패, 0건 건너뜀.
- [`issue_8362_pdf_editable_paint_order.rs`](../../../tests/cases/issue_8362_pdf_editable_paint_order.rs): 이전 실행 2건 통과, 0건 실패, 0건 건너뜀.

검토 후보에 포함된 입력 바이트를 Git blob과 대조했다. 아래 자료는 원 PR의 제출 증거이며 새 후보의 실행 결과를 뜻하지 않는다.
- [`editable-import-2024-hancom-rows.pdf`](../../../pdf/issue8362/editable-import-2024-hancom-rows.pdf) — SHA-256 `139db8fbff158f31dc5a3ee6c4a36cf256268185a3fa1a7d0a2e24f48ab0b57c`.
- [`editable-import-2024.pdf`](../../../pdf/issue8362/editable-import-2024.pdf) — SHA-256 `ecfebf930da0c63e0f389f46405ee8053f6adcb82353189c766be5f1c1bd01a1`.
- [`21_언어_기출_편집가능본-2010.pdf`](../../../samples/21_언어_기출_편집가능본-2010.pdf) — SHA-256 `609d56421009ccab5a50cb2a8f4e78af4d0961de2e7ae85d811905e8434ac251`.

기존 생성 목록으로 최소 글꼴 회귀 12,320건 통과·45건 건너뜀을 확인했다. 생성 목록에서 누락된 #8410 두 모듈의 6건을 목록 재작성 후 같은 환경에서 별도로 실행해 모두 통과했다. 두 실행을 구분하며 한 번의 전체 실행으로 기록하지 않는다.

2026-11-01 고정 후보의 #8389·#8397 전쪽 정밀 대조에서 2,153행 중 Native 12행, WASM 8행이 공개 기준보다 낮았다. 이 수치는 이후 GIF·Canvas 보정 전 결과이며 최종 후보의 재검증 결과가 아니다. #7905 3쪽은 GIF 오른쪽 경계 누락, sample16 35쪽 WASM은 메타파일 내부의 미대응 3픽셀이 확인됐다. #8040 두 원본은 Native·WASM 전쪽이 90% 이상이나 Native 정밀 기준 하락이 남는다. 진행과 근거는 [작업 기록](../../working/task_m100_planet6897_system_fonts_20261031.md)을 따른다. 하락 보정과 전쪽 대조, 각 PR의 독립 Print·Native/fresh WASM 및 직접 판독을 완료해야 한다. #8410·#8412의 새 base 기능도 별도로 보존 검증한다.

원 PR의 최신 head·required check와 통합 후보의 새 검증 결과를 확인하기 전에는 승인이나 merge를 진행하지 않는다.
