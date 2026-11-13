//! 관계형. 한컴 12.30 스타일 편집은 머리말·꼬리말·각주·표 안의 표 문단에도 닿고, 글자 스타일은 그 글자에도
//! 준다. 스타일 넣기·지우기는 글자에 적힌 글자 스타일 번호도 옮긴다. 기준: 고친 스타일 모양이 본문 밖
//! 문단에 들어가고 글자 스타일이 본문 밖 글자에 남으며, HWP·HWPX 저장 뒤에도 같다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;

type Path = Vec<(usize, usize, usize)>;

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap()
}

fn reopened(core: &DocumentCore) -> Vec<(&'static str, DocumentCore)> {
    [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, DocumentCore::from_bytes(&bytes).unwrap()))
    .collect()
}

/// 본문 문단에서 셀 경로를 따라 문단을 찾는다.
fn para_at<'a>(
    core: &'a DocumentCore,
    para: usize,
    path: &[(usize, usize, usize)],
) -> &'a Paragraph {
    let mut p = &core.document().sections[0].paragraphs[para];
    for &(ctrl, cell, cell_para) in path {
        p = match &p.controls[ctrl] {
            Control::Table(t) => &t.cells[cell].paragraphs[cell_para],
            Control::Footnote(n) => &n.paragraphs[cell_para],
            c @ (Control::Header(_) | Control::Footer(_)) => {
                &c.header_footer_paragraphs().unwrap()[cell_para]
            }
            _ => panic!("경로 마디"),
        };
    }
    p
}

/// 머리말 문단 하나, 1×1 표 안의 1×1 표, 각주 하나를 가진 새 문서. 셋 다 글이 있다.
/// (문서, [(본문 문단, 경로); 3])
fn outside_body() -> (HwpDocument, [(usize, Path); 3]) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문").unwrap();
    // 표(새 문단) → 각주(본문 문단 가운데) → 머리말(본문 문단 끝 컨트롤) 차례라야 앞 주소가 밀리지 않는다.
    let table = json(&doc.create_table_native(0, 0, 2, 1, 1).unwrap());
    let (tp, tc) = (
        table["paraIdx"].as_u64().unwrap() as usize,
        table["controlIdx"].as_u64().unwrap() as usize,
    );
    let note = json(&doc.insert_footnote_native(0, 0, 1).unwrap());
    let (np, nc) = (
        note["paraIdx"].as_u64().unwrap() as usize,
        note["controlIdx"].as_u64().unwrap() as usize,
    );
    doc.insert_text_in_footnote_native(0, np, nc, 0, 0, "각주")
        .unwrap();
    // 한컴 각주 글은 각주 스타일의 글자 모양을 쓴다. rhwp가 넣은 글은 0번 모양이라 스타일 모양으로 맞춘다
    // (그렇지 않으면 스타일 적용이 직접 꾸민 글로 보고 남긴다).
    let note_style = para_at(&doc, np, &[(nc, 0, 0)]).style_id as usize;
    let note_shape = doc.document().doc_info.styles[note_style].char_shape_id as u32;
    doc.set_char_shape_id_in_cell_by_path(0, np, &[(nc, 0, 0)], 0, 2, note_shape)
        .unwrap();
    let hf = json(&doc.create_header_footer_native(0, true, 0).unwrap());
    let (hp, hc) = (
        hf["paraIndex"].as_u64().unwrap() as usize,
        hf["controlIndex"].as_u64().unwrap() as usize,
    );
    doc.insert_text_in_header_footer_native(0, true, 0, 0, 0, "머리말")
        .unwrap();
    let made = json(
        &doc.create_table_in_cell_by_path_native(0, tp, &[(tc, 0, 0)], 0, 1, 1, None, None, true)
            .unwrap(),
    );
    let nested: Path = made["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let n = |k: &str| e[k].as_u64().unwrap() as usize;
            (n("controlIndex"), n("cellIndex"), n("cellParaIndex"))
        })
        .collect();
    doc.insert_text_in_cell_by_path(0, tp, &nested, 0, "안쪽")
        .unwrap();
    (
        doc,
        [(hp, vec![(hc, 0, 0)]), (tp, nested), (np, vec![(nc, 0, 0)])],
    )
}

/// 122: 스타일 모양을 고치면 머리말·표 안의 표·각주에서 그 스타일을 쓴 문단도 새 문단·글자 모양이 된다.
/// 셀 경로 스타일 적용은 각주 문단도 받는다.
#[test]
fn style_edit_reaches_header_nested_cell_and_note_paragraphs() {
    let (mut doc, targets) = outside_body();
    let id = doc.create_style(r#"{"name":"밖 스타일","englishName":"Outside","type":0}"#);
    assert!(id > 0);
    let apply = |doc: &mut HwpDocument, (para, path): &(usize, Path)| {
        // HwpDocument의 같은 이름 wasm 메서드(경로 JSON)를 피해 엔진 메서드를 부른다.
        DocumentCore::apply_cell_style_by_path(doc, 0, *para, path, id as usize).unwrap();
        assert_eq!(para_at(doc, *para, path).style_id as i32, id);
    };
    let check = |core: &DocumentCore, label: &str, targets: &[(usize, Path)], size: i32| {
        for (i, (para, path)) in targets.iter().enumerate() {
            let p = para_at(core, *para, path);
            let style = &core.document().doc_info.styles[p.style_id as usize];
            assert_eq!(
                p.para_shape_id, style.para_shape_id,
                "{label} {i}: 문단 모양"
            );
            assert_eq!(
                p.char_shape_id_at(0),
                Some(u32::from(style.char_shape_id)),
                "{label} {i}: 글자 모양"
            );
            let cs = &core.document().doc_info.char_shapes[style.char_shape_id as usize];
            assert_eq!(cs.base_size, size, "{label} {i}: 글자 크기");
        }
    };
    // 머리말·표 안의 표 문단에 스타일을 주고 스타일을 고친다.
    for target in &targets[..2] {
        apply(&mut doc, target);
    }
    assert!(doc.update_style_shapes(
        id as u32,
        r#"{"fontSize":2000}"#,
        r#"{"alignment":"center"}"#
    ));
    check(&doc, "머리말·안쪽 표", &targets[..2], 2000);
    // 각주 문단도 셀 경로로 스타일을 받고, 다시 고치면 셋 다 바뀐다.
    apply(&mut doc, &targets[2]);
    assert!(doc.update_style_shapes(id as u32, r#"{"fontSize":2200}"#, ""));
    check(&doc, "편집 직후", &targets, 2200);
    for (name, core) in reopened(&doc) {
        check(&core, name, &targets, 2200);
    }
}

/// 스타일 넣기·지우기도 글자에 적힌 글자 스타일을 옮긴다. 지운 글자 스타일의 영역은 남지 않는다.
#[test]
fn inserting_and_deleting_styles_remap_char_style_ranges() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "가나다").unwrap();
    let emphasis = doc.create_style(r#"{"name":"강조","englishName":"Emphasis","type":1}"#) as u32;
    doc.set_char_style_range_by_path(0, 0, &[], 0, 2, Some(emphasis))
        .unwrap();
    assert_eq!(
        doc.create_style(r#"{"name":"새 스타일","type":0,"index":1}"#),
        1
    );
    let para = |doc: &HwpDocument| doc.document().sections[0].paragraphs[0].char_style_id_at(0);
    assert_eq!(para(&doc), Some(emphasis + 1));
    assert!(doc.delete_style(emphasis + 1));
    assert_eq!(para(&doc), None);
    assert!(doc.document().sections[0].paragraphs[0]
        .range_tags
        .iter()
        .all(|t| t.char_style_id().is_none()));
}

/// 434: 머리말·각주·표 안의 표 글자에도 글자 스타일을 주고 풀며 HWP·HWPX에 남는다.
#[test]
fn char_style_on_header_note_and_nested_cell_text() {
    let (mut doc, targets) = outside_body();
    let id = doc.create_style(r#"{"name":"강조","englishName":"Emphasis","type":1}"#) as u32;
    assert!(doc.update_style_shapes(id, r#"{"fontSize":1600}"#, ""));
    for (para, path) in &targets {
        let done = json(
            &doc.set_char_style_range_by_path(0, *para, path, 0, 1, Some(id))
                .unwrap(),
        );
        assert_eq!(done["changed"], 1);
    }
    let check = |core: &DocumentCore, label: &str| {
        let shape = core.document().doc_info.styles[id as usize].char_shape_id as u32;
        for (i, (para, path)) in targets.iter().enumerate() {
            let p = para_at(core, *para, path);
            assert_eq!(
                (p.char_style_id_at(0), p.char_style_id_at(1)),
                (Some(id), None),
                "{label} {i}"
            );
            assert_eq!(p.char_shape_id_at(0), Some(shape), "{label} {i}: 모양");
        }
    };
    check(&doc, "편집 직후");
    for (name, core) in reopened(&doc) {
        check(&core, name);
    }
    let (para, path) = &targets[0];
    doc.set_char_style_range_by_path(0, *para, path, 0, 3, None)
        .unwrap();
    assert_eq!(para_at(&doc, *para, path).char_style_id_at(0), None);
}
