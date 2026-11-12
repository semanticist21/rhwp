//! 라벨 문서의 라벨 표: 크기 고정, 개체 보호의 HWP 저장, 라벨 표 정보(`hp:label`).
//!
//! 한/글 12.30 라벨 문서는 쪽마다 크기 고정·개체 보호 표를 두고 `<hp:tbl>` 끝에
//! `<hp:label .../>`를 적는다. 크기 고정 표는 칸에 글이 넘쳐도 행이 자라지 않는다.
//! 종전 rhwp는 크기 고정을 쓰지도 지키지도 못했고, HWPX에서 연 잠긴 표를 HWP로 저장하면
//! 잠금이 풀렸으며, `hp:label`을 읽지 않아 저장하면 사라졌다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::model::control::Control;
use rhwp::wasm_api::HwpDocument;

const LABEL: &str = r#"{"label":{"topmargin":"6804","leftmargin":"1644","boxwidth":"10743","boxlength":"5444","boxmarginhor":"540","boxmarginver":"595","labelcols":"5","labelrows":"12","landscape":"WIDELY","pagewidth":"59528","pageheight":"84188"}}"#;

/// 빈 문서에 1×1 표를 만들고 (문단, 컨트롤) 차례를 준다.
fn doc_with_table() -> (HwpDocument, u32, u32) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let result: serde_json::Value =
        serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let para = result["paraIdx"].as_u64().unwrap() as u32;
    let control = result["controlIdx"].as_u64().unwrap() as u32;
    (doc, para, control)
}

fn table_height(doc: &HwpDocument, para: u32, control: u32) -> f64 {
    let bbox: serde_json::Value =
        serde_json::from_str(&doc.get_table_bbox(0, para, control).unwrap()).unwrap();
    bbox["height"].as_f64().unwrap()
}

fn overflow_cell(doc: &mut HwpDocument, para: u32, control: u32) {
    doc.insert_text_in_cell_native(
        0,
        para as usize,
        control as usize,
        0,
        0,
        0,
        &"가나다라 ".repeat(80),
    )
    .unwrap();
}

#[test]
fn size_protected_table_rows_do_not_grow() {
    let (mut free, para, control) = doc_with_table();
    let declared = table_height(&free, para, control);
    overflow_cell(&mut free, para, control);
    assert!(
        table_height(&free, para, control) > declared + 20.0,
        "크기 고정이 아닌 표는 넘친 글만큼 자란다"
    );

    // 라벨 문서처럼 칸 높이를 정해 두고 크기를 고정한다.
    let (mut fixed, para, control) = doc_with_table();
    fixed
        .set_cell_properties(0, para, control, 0, r#"{"height":3000}"#)
        .unwrap();
    let declared = table_height(&fixed, para, control);
    fixed
        .set_table_properties(0, para, control, r#"{"sizeProtect":true}"#)
        .unwrap();
    let props = fixed.get_table_properties(0, para, control).unwrap();
    assert!(props.contains("\"sizeProtect\":true"), "{props}");
    overflow_cell(&mut fixed, para, control);
    let height = table_height(&fixed, para, control);
    assert!(
        (height - declared).abs() < 1.0,
        "크기 고정 표는 선언 높이를 지킨다: {height} vs {declared}; cells {}",
        fixed
            .get_table_cell_bboxes(0, para, control, None)
            .unwrap_or_default()
    );
    for bytes in [fixed.export_hwp().unwrap(), fixed.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        assert!(reopened
            .get_table_properties(0, para, control)
            .unwrap()
            .contains("\"sizeProtect\":true"));
    }
}

fn table_locked(doc: &HwpDocument, para: usize, control: usize) -> bool {
    match &doc.document().sections[0].paragraphs[para].controls[control] {
        Control::Table(table) => table.common.locked,
        _ => panic!("표가 아니다"),
    }
}

#[test]
fn locked_table_from_hwpx_stays_locked_in_hwp() {
    let (mut doc, para, control) = doc_with_table();
    doc.set_control_lock_api(para, control, true).unwrap();
    let hwpx = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).unwrap();
    assert!(table_locked(&hwpx, para as usize, control as usize));
    let hwp = HwpDocument::from_bytes(&hwpx.export_hwp().unwrap()).unwrap();
    assert!(
        table_locked(&hwp, para as usize, control as usize),
        "HWPX에서 연 잠긴 표는 HWP에서도 잠겨 있어야 한다"
    );
}

#[test]
fn label_metadata_round_trips_through_hwpx() {
    let (mut doc, para, control) = doc_with_table();
    doc.set_table_properties(0, para, control, LABEL).unwrap();
    let hwpx = doc.export_hwpx().unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&hwpx)).unwrap();
    let mut xml = String::new();
    std::io::Read::read_to_string(
        &mut archive.by_name("Contents/section0.xml").unwrap(),
        &mut xml,
    )
    .unwrap();
    assert!(
        xml.contains(r#"<hp:label topmargin="6804" leftmargin="1644" boxwidth="10743" boxlength="5444" boxmarginhor="540" boxmarginver="595" labelcols="5" labelrows="12" landscape="WIDELY" pagewidth="59528" pageheight="84188"/></hp:tbl>"#),
        "한컴 저장본처럼 표 끝에 라벨 정보를 적는다"
    );
    let reopened = HwpDocument::from_bytes(&hwpx).unwrap();
    let props: serde_json::Value =
        serde_json::from_str(&reopened.get_table_properties(0, para, control).unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(LABEL).unwrap();
    assert_eq!(props["label"], expected["label"]);
    let again = HwpDocument::from_bytes(&reopened.export_hwpx().unwrap()).unwrap();
    let props: serde_json::Value =
        serde_json::from_str(&again.get_table_properties(0, para, control).unwrap()).unwrap();
    assert_eq!(props["label"], expected["label"], "다시 저장해도 남는다");
}

/// 한/글 12.30이 폼텍 3101 라벨 문서를 HWP로 저장할 때 표 CTRL_DATA에 적은 바이트.
const HANCOM_3101_CTRL_DATA: &str = "1b02010000004202008042020b0000000a400400dc4801000940040088e800000840040000000000074004000c00000006400400050000000540040053020000044004001c020000034004004415000002400400f7290000014004006c06000000400400941a0000";

#[test]
fn label_metadata_is_hancom_ctrl_data_in_hwp() {
    let expected: serde_json::Value = serde_json::from_str(LABEL).unwrap();
    let label: Vec<(String, String)> = expected["label"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_string()))
        .collect();
    let bytes = rhwp::model::table::label_ctrl_data(&label);
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(hex, HANCOM_3101_CTRL_DATA);
    let parsed = rhwp::model::table::parse_label_ctrl_data(&bytes).unwrap();
    let parsed: serde_json::Value = serde_json::Value::Object(
        parsed
            .into_iter()
            .map(|(key, value)| (key, serde_json::Value::String(value)))
            .collect(),
    );
    assert_eq!(parsed, expected["label"]);

    let (mut doc, para, control) = doc_with_table();
    doc.set_table_properties(0, para, control, LABEL).unwrap();
    let reopened = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).unwrap();
    let props: serde_json::Value =
        serde_json::from_str(&reopened.get_table_properties(0, para, control).unwrap()).unwrap();
    assert_eq!(
        props["label"], expected["label"],
        "HWP 저장본도 라벨 정보를 지킨다"
    );
}
