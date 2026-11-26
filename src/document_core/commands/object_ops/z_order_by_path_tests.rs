//! 표 셀·본문 그림과 도형의 배치 순서(`change_object_z_order_by_path_native`)와 그 그리기 순서를 검사한다.

use crate::document_core::DocumentCore;
use crate::model::control::Control;
use crate::renderer::render_tree::{RenderNode, RenderNodeType};

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("JSON")
}

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

/// 쪽 0에 그려지는 그림의 BinData 번호(그리는 순서).
fn painted_pictures(core: &DocumentCore) -> Vec<u16> {
    fn walk(node: &RenderNode, out: &mut Vec<u16>) {
        if let RenderNodeType::Image(image) = &node.node_type {
            out.push(image.bin_data_id);
        }
        for child in &node.children {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(&core.build_page_tree(0).expect("쪽 트리").root, &mut out);
    out
}

/// 1×1 표 셀 문단에 겹친 글 앞 그림 둘을 넣는다. (표 문단, 표 컨트롤, 셀 경로 JSON, 그림 컨트롤 번호들)
fn cell_with_two_pictures() -> (DocumentCore, usize, String, [usize; 2]) {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("빈 문서");
    let table = json(&core.create_table_native(0, 0, 0, 1, 1).expect("표"));
    let table_para = table["paraIdx"].as_u64().unwrap() as usize;
    let table_ctrl = table["controlIdx"].as_u64().unwrap() as usize;
    let cell = [(table_ctrl, 0usize, 0usize)];
    let path = format!(r#"[{{"controlIndex":{table_ctrl},"cellIndex":0,"cellParaIndex":0}}]"#);
    for _ in 0..2 {
        core.insert_picture_in_cell_native(
            0,
            table_para,
            0,
            &cell,
            &png(),
            3000,
            3000,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .expect("셀 그림");
    }
    let mut pictures = Vec::new();
    if let Control::Table(t) =
        &core.document.sections[0].paragraphs[table_para].controls[table_ctrl]
    {
        for (ci, ctrl) in t.cells[0].paragraphs[0].controls.iter().enumerate() {
            if matches!(ctrl, Control::Picture(_)) {
                pictures.push(ci);
            }
        }
    }
    assert_eq!(pictures.len(), 2);
    for &ci in &pictures {
        core.set_cell_picture_properties_by_path_native(
            0,
            table_para,
            &path,
            ci,
            r#"{"textWrap":"InFrontOfText","horzOffset":0,"vertOffset":0}"#,
        )
        .expect("글 앞으로");
    }
    (core, table_para, path, [pictures[0], pictures[1]])
}

fn cell_z(core: &DocumentCore, table_para: usize, ci: usize) -> i32 {
    match &core.document.sections[0].paragraphs[table_para].controls[0] {
        Control::Table(t) => match &t.cells[0].paragraphs[0].controls[ci] {
            Control::Picture(p) => p.common.z_order,
            _ => panic!("그림 아님"),
        },
        _ => panic!("표 아님"),
    }
}

#[test]
fn cell_picture_z_order_changes_within_the_cell_and_paint_order() {
    let (mut core, table_para, path, [a, b]) = cell_with_two_pictures();
    let before = painted_pictures(&core);
    assert_eq!(before.len(), 2);
    assert_ne!(before[0], before[1], "두 그림의 BinData가 달라야 한다");
    // 먼저 그려지는(아래) 그림을 맨 앞으로 보내면 나중에 그려진다.
    let bottom = if cell_z(&core, table_para, a) <= cell_z(&core, table_para, b) {
        a
    } else {
        b
    };
    let top = if bottom == a { b } else { a };
    let result = json(
        &core
            .change_object_z_order_by_path_native(0, table_para, &path, bottom, "front")
            .expect("맨 앞으로"),
    );
    assert_eq!(result["moved"], true);
    assert!(cell_z(&core, table_para, bottom) > cell_z(&core, table_para, top));
    let after = painted_pictures(&core);
    assert_eq!(
        after,
        vec![before[1], before[0]],
        "맨 앞 그림을 나중에 그린다"
    );
    // 이미 맨 앞이면 바꾸지 않는다.
    let again = json(
        &core
            .change_object_z_order_by_path_native(0, table_para, &path, bottom, "forward")
            .expect("앞으로"),
    );
    assert_eq!(again["moved"], false);
    // 한 칸 뒤로는 이웃과 맞바꿔 처음 순서로 돌아간다.
    core.change_object_z_order_by_path_native(0, table_para, &path, bottom, "backward")
        .expect("뒤로");
    assert_eq!(painted_pictures(&core), before);
    // 순서가 없는 컨트롤과 모르는 갈래는 거절한다.
    assert!(core
        .change_object_z_order_by_path_native(0, table_para, &path, a, "up")
        .is_err());
    assert!(core
        .change_object_z_order_by_path_native(0, table_para, &path, 99, "front")
        .is_err());
}

#[test]
fn body_z_order_by_empty_path_includes_pictures_with_shapes() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("빈 문서");
    for x in [0, 1000] {
        core.create_shape_control_native(
            0,
            0,
            0,
            4000,
            4000,
            x,
            0,
            false,
            "InFrontOfText",
            "rectangle",
            false,
            false,
            &[],
        )
        .expect("도형");
    }
    let picture = json(
        &core
            .insert_picture_native(
                0,
                0,
                0,
                &[],
                &png(),
                3000,
                3000,
                1,
                1,
                "png",
                "",
                None,
                None,
            )
            .expect("그림"),
    );
    let (pi, ci) = (
        picture["paraIdx"].as_u64().unwrap() as usize,
        picture["controlIdx"].as_u64().unwrap() as usize,
    );
    core.set_picture_properties_native(
        0,
        pi,
        ci,
        r#"{"treatAsChar":false,"textWrap":"InFrontOfText"}"#,
    )
    .expect("떠 있는 그림");
    let z = |core: &DocumentCore| -> Vec<(bool, i32)> {
        core.document.sections[0]
            .paragraphs
            .iter()
            .flat_map(|p| p.controls.iter())
            .filter_map(|c| match c {
                Control::Shape(s) => Some((false, s.z_order())),
                Control::Picture(p) => Some((true, p.common.z_order)),
                _ => None,
            })
            .collect()
    };
    for (operation, picture_on_top) in [("back", false), ("front", true)] {
        core.change_object_z_order_by_path_native(0, pi, "", ci, operation)
            .expect("본문 순서");
        let values = z(&core);
        let picture_z = values.iter().find(|v| v.0).unwrap().1;
        let shapes: Vec<i32> = values.iter().filter(|v| !v.0).map(|v| v.1).collect();
        assert_eq!(
            shapes.iter().all(|&s| picture_z > s),
            picture_on_top,
            "{operation}: {values:?}"
        );
        assert_eq!(
            shapes.iter().all(|&s| picture_z < s),
            !picture_on_top,
            "{operation}: {values:?}"
        );
    }
}
