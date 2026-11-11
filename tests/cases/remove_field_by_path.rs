//! 관계형. 머리말·꼬리말 문단과 중첩 표 칸 문단의 누름틀을 셀 경로로 지운다. 기준: 누름틀 컨트롤과
//! 그 자리만 사라지고 둘레 글은 그대로이며, HWP·HWPX로 저장해 다시 열어도 누름틀이 없다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::DocumentCore;
use rhwp::model::control::{Control, FieldType};
use rhwp::model::paragraph::Paragraph;

type Path = Vec<(usize, usize, usize)>;

fn paragraph<'a>(core: &'a DocumentCore, para: usize, path: &Path) -> &'a Paragraph {
    let mut p = &core.document().sections[0].paragraphs[para];
    for &(ctrl, cell, cell_para) in path {
        let c = &p.controls[ctrl];
        p = match c {
            Control::Table(t) => &t.cells[cell].paragraphs[cell_para],
            Control::Header(_) | Control::Footer(_) => {
                &c.header_footer_paragraphs().unwrap()[cell_para]
            }
            _ => panic!("경로 마디"),
        };
    }
    p
}

fn click_here_count(p: &Paragraph) -> usize {
    p.controls
        .iter()
        .filter(|c| matches!(c, Control::Field(f) if f.field_type == FieldType::ClickHere))
        .count()
}

fn check(mut core: DocumentCore, para: usize, path: Path) {
    core.insert_click_here_field_at_by_path(0, para, &path, 1, "여기에 입력", "", "", false)
        .unwrap();
    assert_eq!(click_here_count(paragraph(&core, para, &path)), 1);
    core.remove_field_at_by_path(0, para, &path, 1).unwrap();
    let p = paragraph(&core, para, &path);
    assert_eq!(click_here_count(p), 0);
    assert_eq!(p.text, "앞뒤");
    assert!(core.remove_field_at_by_path(0, para, &path, 1).is_err());
    for (name, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let p = paragraph(&reopened, para, &path);
        assert_eq!(click_here_count(p), 0, "{name}");
        assert_eq!(p.text, "앞뒤", "{name}");
    }
}

#[test]
fn removes_click_here_field_in_header_paragraph() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    let hf: serde_json::Value =
        serde_json::from_str(&core.create_header_footer_native(0, true, 0).unwrap()).unwrap();
    let (pi, ci) = (
        hf["paraIndex"].as_u64().unwrap() as usize,
        hf["controlIndex"].as_u64().unwrap() as usize,
    );
    core.insert_text_in_header_footer_native(0, true, 0, 0, 0, "앞뒤")
        .unwrap();
    check(core, pi, vec![(ci, 0, 0)]);
}

#[test]
fn removes_click_here_field_in_nested_table_cell() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.create_table_native(0, 0, 0, 1, 1).unwrap();
    let (para, ctrl) = core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, p)| {
            p.controls
                .iter()
                .position(|c| matches!(c, Control::Table(_)))
                .map(|ci| (pi, ci))
        })
        .unwrap();
    let made: serde_json::Value = serde_json::from_str(
        &core
            .create_table_in_cell_by_path_native(
                0,
                para,
                &[(ctrl, 0, 0)],
                0,
                1,
                1,
                None,
                None,
                true,
            )
            .unwrap(),
    )
    .unwrap();
    let path: Path = made["cellPath"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let n = |k: &str| e[k].as_u64().unwrap() as usize;
            (n("controlIndex"), n("cellIndex"), n("cellParaIndex"))
        })
        .collect();
    core.insert_text_in_cell_by_path(0, para, &path, 0, "앞뒤")
        .unwrap();
    check(core, para, path);
}
