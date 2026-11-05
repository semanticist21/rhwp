//! 메모 댓글(한컴 12.30 Mac 메모 풍선의 댓글) 읽기·쓰기·지우기.

use crate::document_core::DocumentCore;

const HWP: &[u8] = include_bytes!("../../../tests/fixtures/memo_hancom/reply.hwp");
const HWPX: &[u8] = include_bytes!("../../../tests/fixtures/memo_hancom/reply.hwpx");

/// (번호, 부모 번호, 글)을 문서 차례로.
fn memos(core: &DocumentCore) -> Vec<(u64, Option<u64>, String)> {
    let list: serde_json::Value = serde_json::from_str(&core.get_memo_list_json()).unwrap();
    list.as_array()
        .unwrap()
        .iter()
        .map(|memo| {
            (
                memo["id"].as_u64().unwrap(),
                memo["parentId"].as_u64(),
                memo["text"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn hancom_replies() -> Vec<(u64, Option<u64>, String)> {
    vec![
        (1238731703, None, "첫 메모 내용입니다.".into()),
        (1238731704, Some(1238731703), "댓글입니다.".into()),
        (1238731719, Some(1238731703), "첫 메모의 둘째 댓글".into()),
        (1238731705, None, "캐럿 자리 메모입니다.".into()),
        (1238731718, Some(1238731705), "셋째 메모의 댓글".into()),
    ]
}

#[test]
fn hancom_replies_survive_both_formats() {
    for bytes in [HWP, HWPX] {
        let core = DocumentCore::from_bytes(bytes).unwrap();
        assert_eq!(memos(&core), hancom_replies());
        for out in [
            core.export_hwp_native().unwrap(),
            core.export_hwpx_native().unwrap(),
        ] {
            assert_eq!(
                memos(&DocumentCore::from_bytes(&out).unwrap()),
                hancom_replies()
            );
        }
    }
}

#[test]
fn reply_shares_the_anchor_and_goes_with_its_parent() {
    let mut core = DocumentCore::from_bytes(HWPX).unwrap();
    // 댓글의 댓글은 맨 위 메모의 댓글이 된다.
    let result = core
        .insert_memo_reply_json_native(1238731718, r#"{"author":"t","text":"새 댓글"}"#)
        .unwrap();
    let id: serde_json::Value = serde_json::from_str(&result).unwrap();
    let id = id["id"].as_u64().unwrap();
    let list: serde_json::Value = serde_json::from_str(&core.get_memo_list_json()).unwrap();
    let find = |id: u64| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|memo| memo["id"].as_u64() == Some(id))
            .unwrap()
            .clone()
    };
    let (reply, parent) = (find(id), find(1238731705));
    assert_eq!(reply["parentId"].as_u64(), Some(1238731705));
    for key in ["paraIndex", "startCharIdx", "endCharIdx"] {
        assert_eq!(reply[key], parent[key], "{key}");
    }
    // 저장하고 다시 열어도 관계가 남는다.
    let reopened = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
    assert!(memos(&reopened).contains(&(id, Some(1238731705), "새 댓글".into())));

    // 댓글을 지우면 관계만 사라지고, 부모를 지우면 댓글도 지운다.
    core.remove_memo(1238731704).unwrap();
    assert!(!core
        .document
        .memo_parents
        .iter()
        .any(|(r, _)| *r == 1238731704));
    core.remove_memo(1238731705).unwrap();
    let left: Vec<u64> = memos(&core).into_iter().map(|(id, _, _)| id).collect();
    assert_eq!(left, [1238731703, 1238731719]);
    let out = core.export_hwpx_native().unwrap();
    assert_eq!(
        memos(&DocumentCore::from_bytes(&out).unwrap()),
        [
            (1238731703, None, "첫 메모 내용입니다.".into()),
            (1238731719, Some(1238731703), "첫 메모의 둘째 댓글".into()),
        ]
    );
}
