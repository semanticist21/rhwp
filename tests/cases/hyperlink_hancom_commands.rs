//! 관계형. 하이퍼링크 Command를 한컴 12.30이 저장하는 모양으로 만든다(2026-11-11 한컴 HWPX 저장본):
//! 메일은 `mailto:a@b;2;0;0`(콜론 그대로·종류 2·끝 세미콜론 없음), 웹은 `?`까지 escape한
//! `https\://a.example.org/x\?y=1;1;0;0;`이다. 웹 링크를 메일로 고치면 한컴 HWPX의 Category도 따라 바뀐다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::document_core::hyperlink::HyperlinkTarget;
use rhwp::document_core::queries::field_query::NestedEntry;
use rhwp::document_core::DocumentCore;
use rhwp::model::control::{FieldType, Parameter};
use rhwp::model::hyperlink::{command_uri, destination_command};

const MAIL: &str = "mailto:test@example.com";
const WEB: &str = "https://a.example.org/x?y=1";

#[test]
fn commands_match_hancom_spelling() {
    assert_eq!(
        destination_command(MAIL).unwrap(),
        "mailto:test@example.com;2;0;0"
    );
    assert_eq!(
        destination_command(WEB).unwrap(),
        r"https\://a.example.org/x\?y=1;1;0;0;"
    );
    assert_eq!(
        destination_command("http://www.example.com").unwrap(),
        r"http\://www.example.com;1;0;0;"
    );
    for uri in [MAIL, WEB, "mailto:a;b@example.com"] {
        assert_eq!(command_uri(&destination_command(uri).unwrap()), uri);
    }
}

#[test]
fn inserted_links_keep_hancom_commands_in_hwp_and_hwpx() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_text_native(0, 0, 0, "메일 웹").unwrap();
    let target = HyperlinkTarget::body(0, 0);
    core.insert_hyperlink_native(&target, 0, 2, MAIL).unwrap();
    core.insert_hyperlink_native(&target, 3, 4, WEB).unwrap();
    for (name, bytes) in [
        ("HWP", core.export_hwp_native().unwrap()),
        ("HWPX", core.export_hwpx_native().unwrap()),
    ] {
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        let mut commands: Vec<String> = reopened
            .collect_all_fields()
            .into_iter()
            .filter(|f| f.field.field_type == FieldType::Hyperlink)
            .map(|f| f.field.command)
            .collect();
        commands.sort();
        assert_eq!(
            commands,
            [
                r"https\://a.example.org/x\?y=1;1;0;0;",
                "mailto:test@example.com;2;0;0"
            ],
            "{name}"
        );
    }
}

#[test]
fn changing_hancom_web_link_to_mail_updates_category() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/samples/hwpx_sample2.hwpx"
    ))
    .unwrap();
    let mut core = DocumentCore::from_bytes(&bytes).unwrap();
    let info = core
        .collect_all_fields()
        .into_iter()
        .find(|info| {
            info.field.field_type == FieldType::Hyperlink
                && command_uri(&info.field.command)
                    == "https://apply.lh.or.kr/LH/index.html#MN::CLCC_MN_0010:"
        })
        .expect("한컴의 웹 링크");
    let target = HyperlinkTarget {
        section: info.location.section_index,
        para: info.location.para_index,
        cell_path: info
            .location
            .nested_path
            .iter()
            .map(|entry| match entry {
                NestedEntry::TableCell {
                    control_index,
                    cell_index,
                    para_index,
                } => (*control_index, *cell_index, *para_index),
                NestedEntry::TextBox {
                    control_index,
                    para_index,
                } => (*control_index, 0, *para_index),
            })
            .collect(),
    };
    let id = info.field.field_id;
    core.update_hyperlink_native(&target, id, MAIL).unwrap();
    let reopened = DocumentCore::from_bytes(&core.export_hwpx_native().unwrap()).unwrap();
    let field = reopened
        .collect_all_fields()
        .into_iter()
        .find(|info| info.field.field_id == id)
        .unwrap()
        .field;
    assert_eq!(field.command, "mailto:test@example.com;2;0;0");
    let string = |key: &str| {
        field.parameters.items.iter().find_map(|item| match item {
            Parameter::String {
                name: Some(name),
                value,
                ..
            } if name == key => Some(value.clone()),
            _ => None,
        })
    };
    assert_eq!(
        string("Category").as_deref(),
        Some("HWPHYPERLINK_TYPE_EMAIL")
    );
    assert_eq!(string("Path").as_deref(), Some(MAIL));
}
