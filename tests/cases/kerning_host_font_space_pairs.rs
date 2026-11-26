//! 호스트가 준 설치 글꼴 파일로 커닝하고, 빈칸이 낀 글자 쌍은 커닝하지 않는다.
//!
//! 한컴 12.30(Mac)은 26pt Arial `To AV`에서 `To`와 `AV`만 당기고 `빈칸·A`, `빈칸·T` 쌍은 당기지 않는다
//! (PDF 글자 x: T 85.08, o 98.04, A 125.52, V 141.00pt).
//! 글꼴은 저장소의 합성 글꼴이다. `RHWPExactKerningSmoke.ttf`는 `pos A V -80`, `pos T o -40`,
//! `RHWPExactKerningSpaceSmoke.ttf`는 거기에 `pos space A -80`, `pos V space -80`을 더했다.
//! 1000 em, advance 600, 26pt(34.67px)에서 80 단위는 2.77px이다.

#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;

const KERNING_FONT: &[u8] = include_bytes!("../fixtures/fonts/RHWPExactKerningSmoke.ttf");
const SPACE_FONT: &[u8] = include_bytes!("../fixtures/fonts/RHWPExactKerningSpaceSmoke.ttf");
const PAIR: f64 = 80.0 * 2_600.0 / 7_200.0 * 96.0 / 1_000.0;

/// 글자모양을 모두 커닝·26pt로 바꾼 새 문서에 text를 넣는다.
fn kerning_doc(text: &str) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("새 문서 템플릿");
    let mut document = core.document().clone();
    for char_shape in &mut document.doc_info.char_shapes {
        char_shape.raw_data = None;
        char_shape.kerning = true;
        char_shape.base_size = 2_600;
    }
    core.set_document(document);
    core.insert_text_native(0, 0, 0, text).expect("입력");
    core
}

/// 첫 쪽 SVG의 한 글자 `<text>` x를 차례로 돌려준다.
fn glyph_xs(core: &DocumentCore) -> Vec<(char, f64)> {
    let svg = core.render_page_svg_native(0).expect("SVG");
    let mut glyphs = Vec::new();
    for chunk in svg.split("<text ").skip(1) {
        let Some(tag_end) = chunk.find('>') else {
            continue;
        };
        let tag = &chunk[..tag_end];
        let Some(content_end) = chunk[tag_end + 1..].find("</text>") else {
            continue;
        };
        let mut chars = chunk[tag_end + 1..tag_end + 1 + content_end].chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            continue;
        };
        let Some(start) = tag.find(" x=\"").map(|at| at + 4) else {
            continue;
        };
        let end = start + tag[start..].find('"').expect("x 끝");
        glyphs.push((ch, tag[start..end].parse().expect("x 값")));
    }
    glyphs
}

/// 모든 글자모양의 영문 slot에 글꼴을 등록한다.
fn register_all(core: &mut DocumentCore, font: &[u8]) {
    for char_shape_id in 0..core.document().doc_info.char_shapes.len() as u32 {
        core.register_exact_font_source_native(char_shape_id, 1, font, 0)
            .expect("글꼴 등록");
    }
}

/// 첫 글자모양의 영문 글꼴 이름.
fn english_font(core: &DocumentCore) -> String {
    let info = &core.document().doc_info;
    let id = info.char_shapes[0].font_ids[1] as usize;
    info.font_faces[1][id].name.clone()
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 0.01,
        "{what}: {actual} != {expected}"
    );
}

#[test]
fn kerning_skips_pairs_with_a_space_like_hancom() {
    let plain = glyph_xs(&kerning_doc("AV AV"));
    let mut kerned = kerning_doc("AV AV");
    register_all(&mut kerned, SPACE_FONT);
    let kerned = glyph_xs(&kerned);
    assert_eq!(plain.len(), 4, "{plain:?}");
    assert_eq!(kerned.len(), 4, "{kerned:?}");
    // A·V만 당긴다. V·빈칸, 빈칸·A 쌍은 그대로 둔다.
    assert_close(kerned[1].1, plain[1].1 - PAIR, "첫 V");
    assert_close(kerned[2].1, plain[2].1 - PAIR, "둘째 A");
    assert_close(kerned[3].1, plain[3].1 - 2.0 * PAIR, "둘째 V");
}

#[test]
fn host_font_kerns_every_kerning_char_shape_and_survives_rebuilds() {
    let mut core = kerning_doc("AVTo");
    let plain = glyph_xs(&core);
    let name = english_font(&core);
    assert!(
        core.kerning_font_requests()
            .contains(&(name.clone(), false)),
        "{:?}",
        core.kerning_font_requests()
    );
    assert!(core.register_kerning_font_native(&name, false, KERNING_FONT));
    assert!(!core
        .kerning_font_requests()
        .contains(&(name.clone(), false)));
    let kerned = glyph_xs(&core);
    assert_close(kerned[1].1, plain[1].1 - PAIR, "V");

    // 편집으로 새 글자모양이 생겨도 커닝한다.
    core.apply_char_format_native(0, 0, 0, 4, r##"{"textColor":"#ff0000"}"##)
        .expect("글자 색");
    assert_close(glyph_xs(&core)[1].1, plain[1].1 - PAIR, "새 글자모양의 V");

    // 문서를 통째로 다시 세워도(스냅샷 되돌리기 등) 호스트 글꼴로 다시 등록한다.
    let document = core.document().clone();
    core.set_document(document);
    assert_close(glyph_xs(&core)[1].1, plain[1].1 - PAIR, "다시 세운 뒤의 V");
}

#[test]
fn host_font_ignores_shapes_without_kerning_and_empty_files() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("새 문서 템플릿");
    assert!(core.kerning_font_requests().is_empty());

    let mut core = kerning_doc("AVTo");
    let name = english_font(&core);
    // 빈 파일은 없는 글꼴로 기억해 다시 묻지 않고, 아무것도 등록하지 않는다.
    assert!(!core.register_kerning_font_native(&name, false, &[]));
    assert!(!core.kerning_font_requests().contains(&(name, false)));
}
