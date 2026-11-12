//! 병합 셀이 있는 표의 행/열 바꾸기 뒤 줄 높이. 저장했다 다시 연 문서에서 두 문단을 품은 병합 셀이
//! 한 줄 높이로 바뀌면, 다시 조판하지 않으면 그 줄이 옛 높이로 그려져 아래 줄 글자와 겹쳤다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;

fn text_y(svg: &str, ch: char) -> f64 {
    let needle = format!(">{ch}</text>");
    let end = svg.find(&needle).unwrap_or_else(|| panic!("{ch} 없음"));
    let start = svg[..end].rfind("<text").unwrap();
    let tag = &svg[start..end];
    let y = tag.find(" y=\"").unwrap() + 4;
    tag[y..y + tag[y..].find('"').unwrap()].parse().unwrap()
}

#[test]
fn transposed_merged_cell_grows_its_row_in_reopened_document() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let t: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 2, 3).unwrap()).unwrap();
    let (p, c) = (
        t["paraIdx"].as_u64().unwrap() as usize,
        t["controlIdx"].as_u64().unwrap() as usize,
    );
    for (i, ch) in "ABCDEF".chars().enumerate() {
        doc.insert_text_in_cell_native(0, p, c, i, 0, 0, &ch.to_string())
            .unwrap();
    }
    // 첫 줄 둘째·셋째 칸을 합치면 B·C 두 문단이 한 셀에 든다.
    doc.merge_table_cells_native(0, p, c, 0, 1, 0, 2).unwrap();
    doc.transpose_table_cells_in_place_native(0, p, c).unwrap();
    for bytes in [doc.export_hwpx().unwrap(), doc.export_hwp().unwrap()] {
        let mut reopened = HwpDocument::from_bytes(&bytes).unwrap();
        reopened
            .transpose_table_cells_in_place_native(0, p, c)
            .unwrap();
        let svg = reopened.render_page_svg_native(0).unwrap();
        assert!(
            text_y(&svg, 'C') < text_y(&svg, 'D'),
            "병합 셀의 둘째 문단(C)은 아래 줄(D)보다 위에 있어야 한다"
        );
    }
}
