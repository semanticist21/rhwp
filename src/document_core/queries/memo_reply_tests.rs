//! 메모 번호가 구역마다 겹치는 문서의 메모 본문 보존(한컴 12.30). 댓글 검사는 devel
//! `tests/cases/memo_shape_and_replies.rs`가 맡는다.

use crate::document_core::DocumentCore;

#[test]
fn memos_repeating_a_number_in_another_section_keep_their_text() {
    // 한컴 12.30 두 구역 문서에서 둘째 구역 메모의 번호(1)를 첫 구역 메모와 같은 2로 바꾼 HWPX.
    // HWP는 모든 메모 본문을 마지막 구역 끝에 번호로 적으므로, 겹친 번호는 다시 열 때 본문을 뒤바꿨다.
    let core = DocumentCore::from_bytes(include_bytes!(
        "../../../tests/fixtures/memo_hancom/repeated-number.hwpx"
    ))
    .unwrap();
    let texts = |core: &DocumentCore| -> Vec<(u64, u64, String)> {
        let list: serde_json::Value = serde_json::from_str(&core.get_memo_list_json()).unwrap();
        list.as_array()
            .unwrap()
            .iter()
            .map(|memo| {
                (
                    memo["sectionIndex"].as_u64().unwrap(),
                    memo["index"].as_u64().unwrap(),
                    memo["text"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let expected = vec![
        (0, 2, "첫 구역 메모".to_string()),
        (1, 3, "둘째 구역 메모".to_string()),
    ];
    assert_eq!(texts(&core), expected);
    for out in [
        core.export_hwp_native().unwrap(),
        core.export_hwpx_native().unwrap(),
    ] {
        assert_eq!(texts(&DocumentCore::from_bytes(&out).unwrap()), expected);
    }
}
