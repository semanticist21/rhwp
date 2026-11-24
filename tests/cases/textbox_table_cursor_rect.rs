//! 글상자 글의 캐럿 좌표가 글상자 안 표 칸 상자에 갇힌다.
//!
//! 셀 번호 캐럿 API(`getCursorRectInCell`)는 본문 문단·컨트롤·셀 번호로 셀 상자를 찾아 캐럿을 그 안에
//! 가둔다. 글상자(본문 문단의 도형)의 글은 셀 0으로 고치는데, 그 글상자 안 표도 같은 본문 문단과
//! 같은 컨트롤 번호(글상자 문단의 0번)를 렌더 트리에 실어 그 표의 칸 상자가 잡혔다. 그래서 표 아래
//! 글상자 문단의 캐럿이 모두 표 첫 칸 안 한 자리로 끌려 올라갔다. 글상자 글에는 셀 상자가 없다.
#![cfg(not(target_arch = "wasm32"))]

use std::path::Path;

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;

// 글상자(본문 2번 문단 0번 도형) 다섯째 문단에 0번 컨트롤인 발신처 표가 있다.
const SAMPLE: &str = "samples/issue5721/2568129_textbox_float_tables.hwp";

#[test]
fn textbox_text_cursor_rect_ignores_cell_box_of_table_inside_textbox() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SAMPLE);
    let core = DocumentCore::from_bytes(&std::fs::read(path).expect("read sample")).expect("open");

    // 표를 품은 글상자와, 그 글상자 문단 가운데 표 뒤의 글 문단들을 찾는다.
    let (para_idx, ctrl_idx, after) = core.document().sections[0]
        .paragraphs
        .iter()
        .enumerate()
        .find_map(|(pi, para)| {
            para.controls.iter().enumerate().find_map(|(ci, ctrl)| {
                let Control::Shape(shape) = ctrl else {
                    return None;
                };
                let text_box = shape.drawing()?.text_box.as_ref()?;
                let table_at = text_box
                    .paragraphs
                    .iter()
                    .position(|p| matches!(p.controls.get(ci), Some(Control::Table(_))))?;
                let after: Vec<usize> = (table_at + 1..text_box.paragraphs.len())
                    .filter(|&p| text_box.paragraphs[p].controls.is_empty())
                    .collect();
                Some((pi, ci, after))
            })
        })
        .expect("같은 컨트롤 번호의 표를 품은 글상자");
    assert!(
        after.len() >= 2,
        "표 뒤 글상자 글 문단이 둘 이상이어야 한다"
    );

    let rects: Vec<serde_json::Value> = after
        .iter()
        .map(|&p| {
            serde_json::from_str(
                &core
                    .get_cursor_rect_in_cell_native(0, para_idx, ctrl_idx, 0, p, 0)
                    .expect("글상자 글 캐럿"),
            )
            .expect("캐럿 좌표 JSON")
        })
        .collect();
    for (p, r) in after.iter().zip(&rects) {
        assert!(
            r.get("cellBounds").is_none(),
            "글상자 {p}번 문단 캐럿이 글상자 안 표의 칸 상자에 갇혔다: {r}"
        );
    }
    let ys: Vec<f64> = rects.iter().map(|r| r["y"].as_f64().expect("y")).collect();
    assert!(
        ys.windows(2).all(|w| w[1] > w[0]),
        "글상자 글 문단 캐럿은 문단마다 내려가야 한다: {ys:?}"
    );
}
