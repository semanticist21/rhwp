//! 번호와 누름틀의 저장 글자 축·원문 서식을 보존하고, 새 글자는 기존 원시 입력점 서식을 이어받는다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::parser::hwpx::section::parse_hwpx_section;
use serde_json::Value;

const PARA: usize = 1;
const NAME: &str = "번호와 누름틀";

#[derive(Clone, Copy, Debug)]
enum Number {
    Auto,
    New,
}

#[derive(Clone, Copy, Debug)]
enum Place {
    First,
    AfterEnd,
    Last,
}

#[derive(Clone, Copy, Debug)]
enum Edit {
    Interior,
    ActiveEnd,
    SetValue,
}

fn paragraph(core: &DocumentCore) -> &Paragraph {
    &core.document().sections[0].paragraphs[PARA]
}

fn properties(core: &DocumentCore, at: usize) -> Value {
    serde_json::from_str(&core.get_char_properties_at_native(0, PARA, at).unwrap()).unwrap()
}

fn styles(
    core: &DocumentCore,
    field: (usize, usize),
    number: usize,
    place: Place,
    italic_new: Option<(usize, usize)>,
) {
    let p = paragraph(core);
    for at in 0..p.text.chars().count() {
        let actual = properties(core, at);
        // 새 글자만 원시 입력점의 run을 이어받는다. 원문 A/B/尾의 기대는 그대로 둔다.
        let new_italic = italic_new.is_some_and(|(start, end)| (start..end).contains(&at));
        let in_field = (field.0..field.1).contains(&at) && !new_italic;
        let after_field = at >= field.1 || new_italic;
        assert_eq!(actual["bold"], in_field, "{place:?} at={at}: {actual}");
        assert_eq!(actual["italic"], after_field, "{place:?} at={at}: {actual}");
        assert_eq!(
            actual["textColor"],
            if in_field {
                "#cc0000"
            } else if after_field {
                "#0000cc"
            } else {
                "#000000"
            },
            "{place:?} at={at}: {actual}"
        );
    }
    assert_eq!(p.control_text_positions().len(), 2);
    let number_index = p
        .controls
        .iter()
        .position(|c| matches!(c, Control::AutoNumber(_) | Control::NewNumber(_)))
        .unwrap();
    assert_eq!(p.control_text_positions()[number_index], number);
}

fn assert_model(
    core: &DocumentCore,
    text: &str,
    value: &str,
    offsets: &[u32],
    count: u32,
    field: (usize, usize),
    number: usize,
    place: Place,
) {
    let p = paragraph(core);
    assert_eq!(p.text, text);
    assert_eq!(p.char_offsets, offsets);
    assert_eq!(p.char_count, count);
    assert_eq!(p.field_ranges.len(), 1);
    assert_eq!(
        (
            p.field_ranges[0].start_char_idx,
            p.field_ranges[0].end_char_idx
        ),
        field
    );
    let actual: Value = serde_json::from_str(&core.get_field_value_by_name(NAME).unwrap()).unwrap();
    assert_eq!(actual["value"], value);
    assert_eq!(
        p.control_text_positions()[p.field_ranges[0].control_idx],
        field.0
    );
}

fn source(number: Number, place: Place) -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    // 공식 서식 API가 만든 유효한 DocInfo를 원문 XML의 run 참조에 사용한다.
    core.insert_text_native(0, 0, 0, "XY").unwrap();
    core.apply_char_format_native(0, 0, 0, 1, r##"{"bold":true,"textColor":"#cc0000"}"##)
        .unwrap();
    core.apply_char_format_native(0, 0, 1, 2, r##"{"italic":true,"textColor":"#0000cc"}"##)
        .unwrap();
    let seed = &core.document().sections[0].paragraphs[0];
    let bold = seed.char_shape_id_at(0).unwrap();
    let italic = seed.char_shape_id_at(1).unwrap();
    core.delete_text_native(0, 0, 0, 2).unwrap();

    let number_xml = match number {
        Number::Auto => "<hp:ctrl><hp:autoNum num=\"7\" numType=\"PAGE\"/></hp:ctrl>",
        Number::New => "<hp:ctrl><hp:newNum num=\"7\" numType=\"PAGE\"/></hp:ctrl>",
    };
    let leading = if matches!(place, Place::First) {
        number_xml
    } else {
        ""
    };
    let trailing = if matches!(place, Place::First) {
        ""
    } else {
        number_xml
    };
    let suffix = if matches!(place, Place::Last) {
        ""
    } else {
        "尾"
    };
    let xml = format!(
        r#"<hs:sec xmlns:hp="http://www.hancom.co.kr/hwpml/2011/paragraph" xmlns:hs="http://www.hancom.co.kr/hwpml/2011/section">
<hp:p paraPrIDRef="0" styleIDRef="0">
<hp:run charPrIDRef="0">{leading}<hp:t>P</hp:t></hp:run>
<hp:run charPrIDRef="{bold}"><hp:ctrl><hp:fieldBegin id="71" type="CLICK_HERE" name="{NAME}" fieldid="71" editable="1" dirty="1"/></hp:ctrl><hp:t>AB</hp:t></hp:run>
<hp:run charPrIDRef="{italic}"><hp:ctrl><hp:fieldEnd beginIDRef="71" fieldid="71"/></hp:ctrl>{trailing}<hp:t>{suffix}</hp:t></hp:run>
</hp:p></hs:sec>"#
    );
    let parsed = parse_hwpx_section(&xml).unwrap();
    assert_eq!(parsed.paragraphs.len(), 1);
    let mut document = core.document().clone();
    document.sections[0].raw_stream = None;
    document.sections[0]
        .paragraphs
        .push(parsed.paragraphs[0].clone());
    core.set_document(document);
    assert_source(&core, number, place);
    core
}

fn assert_source(core: &DocumentCore, number: Number, place: Place) {
    // 8유닛 번호, BEGIN/END 각 8유닛, 보이는 P/A/B/尾 각 1유닛을 독립적으로 센다.
    match (number, place) {
        (Number::Auto, Place::First) => assert_model(
            core,
            " PAB尾",
            "AB",
            &[0, 8, 17, 18, 27],
            29,
            (2, 4),
            0,
            place,
        ),
        (Number::New, Place::First) => {
            assert_model(core, "PAB尾", "AB", &[8, 17, 18, 27], 29, (1, 3), 0, place)
        }
        (Number::Auto, Place::AfterEnd) => assert_model(
            core,
            "PAB 尾",
            "AB",
            &[0, 9, 10, 19, 27],
            29,
            (1, 3),
            3,
            place,
        ),
        (Number::New, Place::AfterEnd) => {
            assert_model(core, "PAB尾", "AB", &[0, 9, 10, 27], 29, (1, 3), 3, place)
        }
        (Number::Auto, Place::Last) => {
            assert_model(core, "PAB ", "AB", &[0, 9, 10, 19], 28, (1, 3), 3, place)
        }
        (Number::New, Place::Last) => {
            assert_model(core, "PAB", "AB", &[0, 9, 10], 28, (1, 3), 3, place)
        }
    }
    let field = if matches!((number, place), (Number::Auto, Place::First)) {
        (2, 4)
    } else {
        (1, 3)
    };
    let at = if matches!(place, Place::First) { 0 } else { 3 };
    styles(core, field, at, place, None);
}

fn save(core: &DocumentCore) -> [DocumentCore; 2] {
    [
        DocumentCore::from_bytes(&core.export_hwp_with_adapter().unwrap()).unwrap(),
        DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn assert_edited(core: &DocumentCore, number: Number, place: Place, edit: Edit) {
    let value = match edit {
        Edit::Interior => "AXB",
        Edit::ActiveEnd => "ABX",
        Edit::SetValue => "가🦦",
    };
    let (text, offsets, count, field, at): (String, &[u32], u32, (usize, usize), usize) =
        match (number, place, edit) {
            (Number::Auto, Place::First, Edit::SetValue) => {
                (format!(" P{value}尾"), &[0, 8, 17, 18, 28], 30, (2, 4), 0)
            }
            (Number::New, Place::First, Edit::SetValue) => {
                (format!("P{value}尾"), &[8, 17, 18, 28], 30, (1, 3), 0)
            }
            (Number::Auto, Place::First, _) => (
                format!(" P{value}尾"),
                &[0, 8, 17, 18, 19, 28],
                30,
                (2, 5),
                0,
            ),
            (Number::New, Place::First, _) => {
                (format!("P{value}尾"), &[8, 17, 18, 19, 28], 30, (1, 4), 0)
            }
            (Number::Auto, Place::AfterEnd, Edit::SetValue) => {
                (format!("P{value} 尾"), &[0, 9, 10, 20, 28], 30, (1, 3), 3)
            }
            (Number::New, Place::AfterEnd, Edit::SetValue) => {
                (format!("P{value}尾"), &[0, 9, 10, 28], 30, (1, 3), 3)
            }
            // 새 번호는 가시 자리표시자가 없어 기존 API가 번호 뒤 원시 슬롯에 입력한다.
            (Number::New, Place::AfterEnd, Edit::ActiveEnd) => {
                (format!("P{value}尾"), &[0, 9, 10, 19, 28], 30, (1, 4), 3)
            }
            (Number::Auto, Place::AfterEnd, _) => (
                format!("P{value} 尾"),
                &[0, 9, 10, 11, 20, 28],
                30,
                (1, 4),
                4,
            ),
            (Number::New, Place::AfterEnd, _) => {
                (format!("P{value}尾"), &[0, 9, 10, 11, 28], 30, (1, 4), 4)
            }
            (Number::Auto, Place::Last, Edit::SetValue) => {
                (format!("P{value} "), &[0, 9, 10, 20], 29, (1, 3), 3)
            }
            (Number::New, Place::Last, Edit::SetValue) => {
                (format!("P{value}"), &[0, 9, 10], 29, (1, 3), 3)
            }
            (Number::New, Place::Last, Edit::ActiveEnd) => {
                // 끝에는 다음 글자 위치가 없어 END·번호 앞 원시 슬롯에 넣는다.
                (format!("P{value}"), &[0, 9, 10, 11], 29, (1, 4), 4)
            }
            (Number::Auto, Place::Last, _) => {
                (format!("P{value} "), &[0, 9, 10, 11, 20], 29, (1, 4), 4)
            }
            (Number::New, Place::Last, _) => (format!("P{value}"), &[0, 9, 10, 11], 29, (1, 4), 4),
        };
    assert_model(core, &text, value, offsets, count, field, at, place);
    // 뒤 글자가 있고 번호를 먼저 건너뛰면 뒤 run, 번호·끝 갭 앞이면 필드 run이다.
    // 원문 필드 값과 다른 서식을 선택하는 앱 입력은 이 native primitive와 별도이다.
    let inherits_suffix =
        matches!(place, Place::First) || matches!((number, place), (Number::New, Place::AfterEnd));
    let italic_new = match edit {
        Edit::SetValue if inherits_suffix => Some(field),
        Edit::ActiveEnd if inherits_suffix => Some((field.1 - 1, field.1)),
        _ => None,
    };
    styles(core, field, at, place, italic_new);
}

fn exercise(number: Number, place: Place, edit: Edit) {
    let original = source(number, place);
    for mut core in save(&original) {
        // 두 원문 저장 형식의 슬롯과 서식을 먼저 증명해야 편집 후 실패가 의미를 갖는다.
        assert_source(&core, number, place);
        let field = paragraph(&core).field_ranges[0].clone();
        match edit {
            Edit::Interior => {
                core.insert_text_native(0, PARA, field.start_char_idx + 1, "X")
                    .unwrap();
            }
            Edit::ActiveEnd => {
                assert!(core.set_active_field(0, PARA, field.end_char_idx));
                core.insert_text_native(0, PARA, field.end_char_idx, "X")
                    .unwrap();
            }
            Edit::SetValue => {
                core.set_field_value_by_name(NAME, "가🦦").unwrap();
            }
        }
        assert_edited(&core, number, place, edit);
        for reopened in save(&core) {
            assert_edited(&reopened, number, place, edit);
        }
    }
}

#[test]
fn auto_number_placeholder_keeps_its_eight_units_during_field_input() {
    for place in [Place::First, Place::AfterEnd, Place::Last] {
        exercise(Number::Auto, place, Edit::Interior);
    }
}

#[test]
fn active_field_end_before_number_keeps_slots_and_character_styles() {
    for number in [Number::Auto, Number::New] {
        for place in [Place::First, Place::AfterEnd, Place::Last] {
            exercise(number, place, Edit::ActiveEnd);
        }
    }
}

#[test]
fn field_value_setter_preserves_number_placeholders_and_neighbor_styles() {
    for number in [Number::Auto, Number::New] {
        for place in [Place::First, Place::AfterEnd, Place::Last] {
            exercise(number, place, Edit::SetValue);
        }
    }
}

#[test]
fn hwpx_new_number_uses_a_pure_gap_and_does_not_create_a_visible_space() {
    for place in [Place::First, Place::AfterEnd, Place::Last] {
        exercise(Number::New, place, Edit::Interior);
    }
}
