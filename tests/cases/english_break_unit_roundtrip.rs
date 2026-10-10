//! 영어 줄나눔 값의 HWP/HWPX 저장 계약. 실제 줄 배치 알고리즘은 이 회귀의 범위 밖이다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::style::ParaShape;
use serde_json::Value;

const TEXTS: [&str; 3] = [
    "Alpha-hyphenated-word",
    "같은 모양을 공유하는 보호 문단",
    "마지막 본문 보존",
];

fn source() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    for (index, text) in TEXTS.iter().enumerate() {
        if index > 0 {
            core.insert_paragraph_native(0, index).unwrap();
        }
        core.insert_text_native(0, index, 0, text).unwrap();
    }
    for paragraph in 0..2 {
        core.apply_para_format_native(
            0,
            paragraph,
            r#"{"alignment":"left","marginLeft":240,"marginRight":720,"indent":-120,"spacingBefore":30,"spacingAfter":60,"lineSpacing":175,"koreanBreakUnit":1,"keepWithNext":true}"#,
        )
        .unwrap();
    }
    core.apply_char_format_native(0, 0, 0, 5, r##"{"italic":true,"textColor":"#803090"}"##)
        .unwrap();
    // HWP 원본은 lexical 값 없이 attr1만 가진다. 시작 문서의 출처를 고정한다.
    DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap()
}

fn props(core: &DocumentCore, paragraph: usize) -> Value {
    serde_json::from_str(&core.get_para_properties_at_native(0, paragraph).unwrap()).unwrap()
}

fn unchanged_props(core: &DocumentCore, paragraph: usize) -> Value {
    let mut value = props(core, paragraph);
    let object = value.as_object_mut().unwrap();
    // 편집은 새 문단모양을 만들며 저장 형식은 ID를 다시 매길 수 있다.
    object.remove("paraShapeId");
    object.remove("englishBreakUnit");
    value
}

fn shape(core: &DocumentCore, paragraph: usize) -> &ParaShape {
    let id = core.document().sections[0].paragraphs[paragraph].para_shape_id;
    &core.document().doc_info.para_shapes[id as usize]
}

fn token(value: u8) -> &'static str {
    match value {
        0 => "KEEP_WORD",
        1 => "HYPHENATION",
        2 => "BREAK_WORD",
        _ => panic!("회귀에 없는 값"),
    }
}

fn assert_content(core: &DocumentCore) {
    assert_eq!(
        core.document().sections[0]
            .paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>(),
        TEXTS
    );
    let char_props: Value =
        serde_json::from_str(&core.get_char_properties_at_native(0, 0, 1).unwrap()).unwrap();
    assert_eq!(char_props["italic"], true);
    assert_eq!(char_props["textColor"], "#803090");
}

fn assert_value(core: &DocumentCore, value: u8) {
    assert_eq!(props(core, 0)["englishBreakUnit"], value);
    assert_eq!((shape(core, 0).attr1 >> 5) & 3, value as u32);
    assert_content(core);
}

#[test]
fn known_values_survive_hwp_hwpx_hwp_and_public_properties() {
    for value in 0..=2 {
        let mut core = source();
        let protected = (unchanged_props(&core, 0), unchanged_props(&core, 1));
        // 영어 값이 0인 별도 저장본도 같은 형식 경로를 거친다. 채우기 없는
        // 배경의 미사용 무늬 기본값 등 형식 정규화를 편집 손실과 구별한다.
        let baseline_hwpx = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
        let baseline_hwp =
            DocumentCore::from_bytes(&baseline_hwpx.export_hwp_native().unwrap()).unwrap();
        let base_shape = shape(&core, 0).clone();
        core.apply_para_format_native(0, 0, &format!(r#"{{"englishBreakUnit":{value}}}"#))
            .unwrap();
        assert_value(&core, value);
        let mut restored = shape(&core, 0).clone();
        restored.attr1 = (restored.attr1 & !(3 << 5)) | (base_shape.attr1 & (3 << 5));
        restored.raw_data = base_shape.raw_data.clone();
        assert_eq!(
            serde_json::to_value(restored).unwrap(),
            serde_json::to_value(base_shape).unwrap()
        );
        let hwpx = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
        assert_value(&hwpx, value);
        assert_eq!(
            shape(&hwpx, 0).break_latin_word.as_deref(),
            Some(token(value))
        );
        let hwp = DocumentCore::from_bytes(&hwpx.export_hwp_native().unwrap()).unwrap();
        assert_value(&hwp, value);
        assert!(shape(&hwp, 0).break_latin_word.is_none());
        assert_eq!(unchanged_props(&core, 0), protected.0);
        assert_eq!(unchanged_props(&core, 1), protected.1);
        for (document, baseline) in [(&hwpx, &baseline_hwpx), (&hwp, &baseline_hwp)] {
            assert_eq!(unchanged_props(document, 0), unchanged_props(baseline, 0));
            assert_eq!(unchanged_props(document, 1), unchanged_props(baseline, 1));
            assert_eq!(props(document, 1)["englishBreakUnit"], 0);
        }
    }
}

#[test]
fn explicit_edits_of_hwpx_origin_replace_stale_lexical_in_both_formats() {
    for original in 0..=2 {
        let mut original_core = source();
        original_core
            .apply_para_format_native(0, 0, &format!(r#"{{"englishBreakUnit":{original}}}"#))
            .unwrap();
        let bytes = original_core.export_hwpx_native().unwrap();
        for value in 0..=2 {
            let mut core = DocumentCore::from_bytes(&bytes).unwrap();
            assert_eq!(
                shape(&core, 0).break_latin_word.as_deref(),
                Some(token(original))
            );
            let base = shape(&core, 0).clone();
            let sibling = serde_json::to_value(shape(&core, 1)).unwrap();
            let protected = (unchanged_props(&core, 0), unchanged_props(&core, 1));
            core.apply_para_format_native(0, 0, &format!(r#"{{"englishBreakUnit":{value}}}"#))
                .unwrap();
            assert_value(&core, value);
            assert!(shape(&core, 0).break_latin_word.is_none());
            assert_eq!(serde_json::to_value(shape(&core, 1)).unwrap(), sibling);
            // 공통 변경 함수가 줄나눔·원본 바이트 이외의 ParaShape 필드를 바꾸지 않아야 한다.
            let mut restored = shape(&core, 0).clone();
            restored.attr1 = (restored.attr1 & !(3 << 5)) | (base.attr1 & (3 << 5));
            restored.break_latin_word = base.break_latin_word.clone();
            restored.raw_data = base.raw_data.clone();
            assert_eq!(
                serde_json::to_value(restored).unwrap(),
                serde_json::to_value(base).unwrap()
            );
            for saved in [
                core.export_hwp_native().unwrap(),
                core.export_hwpx_native().unwrap(),
            ] {
                let reopened = DocumentCore::from_bytes(&saved).unwrap();
                assert_value(&reopened, value);
                assert_eq!(unchanged_props(&reopened, 0), protected.0);
                assert_eq!(unchanged_props(&reopened, 1), protected.1);
                assert_eq!(props(&reopened, 1)["englishBreakUnit"], 0);
            }
        }
    }
}

#[test]
fn unknown_lexical_survives_unedited_and_unrelated_hwpx_edits() {
    let mut core = source();
    // 미지 lexical 값 이외에는 같은 HWP 원본을 독립적으로 HWPX 왕복한다.
    let baseline = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    let mut document = core.document().clone();
    let shape_id = document.sections[0].paragraphs[0].para_shape_id as usize;
    assert_eq!(
        shape_id,
        document.sections[0].paragraphs[1].para_shape_id as usize
    );
    document.doc_info.para_shapes[shape_id].break_latin_word = Some("FUTURE_UNIT".into());
    core.set_document(document);
    let original = unchanged_props(&baseline, 0);
    let mut reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    assert_eq!(
        shape(&reopened, 0).break_latin_word.as_deref(),
        Some("FUTURE_UNIT")
    );
    assert_eq!(unchanged_props(&reopened, 0), original);
    assert_value(&reopened, 0);
    reopened
        .apply_para_format_native(0, 0, r#"{"spacingAfter":120}"#)
        .unwrap();
    let protected = (unchanged_props(&reopened, 0), unchanged_props(&reopened, 1));
    let edited = DocumentCore::from_bytes(&reopened.export_hwpx_native().unwrap()).unwrap();
    assert_eq!(
        shape(&edited, 0).break_latin_word.as_deref(),
        Some("FUTURE_UNIT")
    );
    assert_eq!(
        shape(&edited, 1).break_latin_word.as_deref(),
        Some("FUTURE_UNIT")
    );
    assert_eq!(shape(&edited, 0).spacing_after, 120);
    assert_eq!(shape(&edited, 1).spacing_after, 60);
    assert_eq!(unchanged_props(&edited, 0), protected.0);
    assert_eq!(unchanged_props(&edited, 1), protected.1);
    assert_content(&edited);
}
