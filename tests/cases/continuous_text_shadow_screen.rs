//! 화면(글꼴 파일 없음)에서도 연속 그림자는 글자와 그림자 사이 띠를 채운다.
//!
//! 한컴 12.30 Mac은 글자 모양의 연속 그림자를 화면에서 글자부터 그림자 끝까지 이어 그린다. 인쇄 장치 계획은
//! 실제 글꼴 파일이 있어야 세우므로, 계획이 없을 때는 비연속 그림자와 같은 끝점까지 같은 간격으로 잇는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;

/// 36pt `그림자 ABC`에 빨간 그림자(가로·세로 20%)를 건 쪽 SVG에서 그림자 색으로 그린 글자 수.
fn shadow_passes(kind: u8) -> usize {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "그림자 ABC").unwrap();
    doc.apply_char_format_native(
        0,
        0,
        0,
        7,
        &format!(
            r##"{{"fontSize":3600,"shadowType":{kind},"shadowColor":"#ff0000","shadowOffsetX":20,"shadowOffsetY":20}}"##
        ),
    )
    .unwrap();
    let svg = doc.render_page_svg_native(0).unwrap();
    svg.matches("#ff0000").count()
}

#[test]
fn continuous_shadow_fills_the_band_on_screen() {
    let discrete = shadow_passes(1);
    let continuous = shadow_passes(2);
    assert!(discrete > 0, "비연속 그림자는 그린다");
    // 36pt의 20%는 약 9.6px이라 반 픽셀 간격이면 끝점까지 20번 그린다.
    assert!(
        continuous >= discrete * 15,
        "연속 그림자 {continuous}, 비연속 {discrete}"
    );
}
