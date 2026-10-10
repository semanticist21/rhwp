//! HWP 양식의 목록 스크립트와 값은 HWPX 패키지 및 역변환에서 보존한다.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use flate2::read::DeflateDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;
use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::document::Document;
use rhwp::serializer::hwpx::package_check::check_package;
use rhwp::serializer::{serialize_hwp, serialize_hwpx, serialize_hwpx_with_report};
use serde_json::{json, Value};

const HWP: &[u8] = include_bytes!("../../samples/form-01.hwp");
const HWPX: &[u8] = include_bytes!("../../samples/hwpx/form-01.hwpx");
const DISTRIBUTION_HWP: &str = "samples/한글문서파일형식_5.0_revision1.3.hwp";
const SCRIPT: &str = "/Scripts/DefaultJScript";
const PARTS: [&str; 2] = ["Scripts/headerScripts", "Scripts/sourceScripts"];

fn entry(bytes: &[u8], path: &str) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    zip.by_name(path).unwrap().read_to_end(&mut out).unwrap();
    out
}

fn script(doc: &Document) -> &[u8] {
    &doc.extra_streams
        .iter()
        .find(|(path, _)| path == SCRIPT)
        .unwrap()
        .1
}

fn raw_script(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    DeflateDecoder::new(script(doc))
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn replace_script(doc: &mut Document, bytes: Vec<u8>) {
    doc.extra_streams
        .iter_mut()
        .find(|(path, _)| path == SCRIPT)
        .unwrap()
        .1 = bytes;
}

fn set_compression(doc: &mut Document, compressed: bool) {
    // 실제 HWP 입력을 만들 때 모델 플래그와 보존된 FileHeader 바이트를 함께 바꾼다.
    doc.header.compressed = compressed;
    doc.header.flags = (doc.header.flags & !1) | u32::from(compressed);
    if let Some(raw) = doc.header.raw_data.as_mut() {
        raw[36..40].copy_from_slice(&doc.header.flags.to_le_bytes());
    }
}

fn info(core: &DocumentCore, para: usize, control: usize) -> Value {
    serde_json::from_str(&core.get_form_object_info_native(0, para, control).unwrap()).unwrap()
}

fn values(core: &DocumentCore) -> Vec<Value> {
    [(2, 0), (4, 0), (6, 0), (6, 1), (6, 2), (8, 0)]
        .into_iter()
        .map(|(para, ci)| {
            let f = info(core, para, ci);
            json!({"type": f["formType"], "name": f["name"], "value": f["value"],
                "text": f["text"], "caption": f["caption"], "enabled": f["enabled"],
                "items": f["items"], "group": f["properties"]["GroupName"]})
        })
        .collect()
}

fn body_text(doc: &Document) -> Vec<String> {
    doc.sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.clone())
        .collect()
}

fn fixture() -> DocumentCore {
    let mut core = DocumentCore::from_bytes(HWP).unwrap();
    assert_eq!(
        info(&core, 4, 0)["items"],
        json!(["봄", "여름", "가을", "겨울"])
    );
    // 공개 컨트롤 복사로 같은 문단에 라디오를 세 개 만든다. 원본 확장 문자를 직접 합성하지 않는다.
    core.copy_control_native(0, 6, &[], 0).unwrap();
    core.paste_internal_native(0, 6, 0).unwrap();
    core.paste_internal_native(0, 6, 0).unwrap();
    for (ci, (name, group)) in [
        ("RadioButton", "choice"),
        ("second", "choice"),
        ("other", "other"),
    ]
    .into_iter()
    .enumerate()
    {
        let Control::Form(form) = &mut core.document_mut().sections[0].paragraphs[6].controls[ci]
        else {
            panic!("같은 문단 라디오 표본")
        };
        form.name = name.into();
        form.properties.insert("GroupName".into(), group.into());
    }
    for (para, ci, patch) in [
        (2, 0, json!({"value": 0})),
        (4, 0, json!({"text": "겨울"})),
        (6, 0, json!({"value": 0})),
        (6, 1, json!({"value": 1})),
        (6, 2, json!({"value": 1})),
        (8, 0, json!({"text": "한글 \"양식\" \\ 🦦"})),
    ] {
        let result: Value = serde_json::from_str(
            &core
                .set_form_value_native(0, para, ci, &patch.to_string())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["ok"], true);
    }
    core
}

fn assert_manifest_once(bytes: &[u8], expected_ids: Option<[&str; 2]>) {
    let hpf = entry(bytes, "Contents/content.hpf");
    let mut reader = Reader::from_reader(hpf.as_slice());
    let mut scripts = HashMap::<String, Vec<String>>::new();
    let mut refs = HashMap::<String, usize>::new();
    loop {
        match reader.read_event().unwrap() {
            Event::Empty(tag) | Event::Start(tag) => {
                let attrs: HashMap<String, String> = tag
                    .attributes()
                    .map(|a| {
                        let a = a.unwrap();
                        (
                            a.key.as_ref().to_string(),
                            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned(),
                        )
                    })
                    .collect();
                if tag.name().as_ref() == "opf:item" {
                    if let Some(href) = attrs
                        .get("href")
                        .filter(|href| href.starts_with("Scripts/"))
                    {
                        scripts
                            .entry(href.clone())
                            .or_default()
                            .push(attrs["id"].clone());
                    }
                } else if tag.name().as_ref() == "opf:itemref" {
                    *refs.entry(attrs["idref"].clone()).or_default() += 1;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    for (index, path) in PARTS.into_iter().enumerate() {
        let ids = &scripts[path];
        assert_eq!(ids.len(), 1, "{path} 매니페스트는 한 번만 등록");
        assert_eq!(refs[&ids[0]], 1, "{path} spine은 한 번만 등록");
        if let Some(expected) = expected_ids {
            assert_eq!(ids[0], expected[index], "원래 참조 id 보존");
        }
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let count = (0..zip.len())
            .filter(|i| zip.by_index(*i).unwrap().name() == path)
            .count();
        assert_eq!(count, 1, "{path} ZIP 엔트리 중복 없음");
    }
}

#[test]
fn hwp_scripts_items_groups_and_values_survive_both_formats() {
    for compressed in [false, true] {
        let mut core = fixture();
        set_compression(core.document_mut(), compressed);
        let expected = values(&core);
        let text = body_text(core.document());
        let raw = raw_script(core.document());
        // 문서 압축 플래그는 양쪽 모두 쓰되 스크립트의 자체 raw-deflate는 그대로 둔다.
        let source = DocumentCore::from_bytes(&core.export_hwp_native().unwrap()).unwrap();
        assert_eq!(source.document().header.compressed, compressed);
        assert_eq!(
            values(&source),
            expected,
            "HWP 입력 생성도 양식 내용을 보존"
        );
        assert_eq!(body_text(source.document()), text);
        assert_eq!(raw_script(source.document()), raw);
        let saved = serialize_hwpx_with_report(source.document()).unwrap();
        assert!(saved.content_loss().is_empty());
        assert_manifest_once(saved.bytes(), None);
        for path in PARTS {
            assert_eq!(
                entry(saved.bytes(), path),
                entry(HWPX, path),
                "스크립트 원문 바이트"
            );
        }
        let package = check_package(saved.bytes(), source.document());
        assert!(package.is_ok(), "{}", package.summary());
        for bytes in [source.export_hwp_native().unwrap(), saved.into_bytes()] {
            let reopened = DocumentCore::from_bytes(&bytes).unwrap();
            assert_eq!(values(&reopened), expected, "압축={compressed} 양식 내용");
            assert_eq!(body_text(reopened.document()), text, "본문 보존");
            assert_eq!(
                raw_script(reopened.document()),
                raw,
                "역변환의 길이·원문·꼬리 보존"
            );
            let back = DocumentCore::from_bytes(&reopened.export_hwp_native().unwrap()).unwrap();
            assert_eq!(values(&back), expected);
            assert_eq!(raw_script(back.document()), raw);
        }
    }
}

#[test]
fn plain_counted_hwp_script_converts_without_reencoding() {
    let core = fixture();
    let expected = values(&core);
    let raw = raw_script(core.document());
    for compressed in [false, true] {
        let mut doc = core.document().clone();
        set_compression(&mut doc, compressed);
        replace_script(&mut doc, raw.clone());
        // HWP 저장기는 추가 스트림을 그대로 쓰므로 실제 plain 스트림 입력부터 검사한다.
        let source = DocumentCore::from_bytes(&serialize_hwp(&doc).unwrap()).unwrap();
        assert_eq!(script(source.document()), raw);
        let bytes = source.export_hwpx_native().unwrap();
        assert_manifest_once(&bytes, None);
        for path in PARTS {
            assert_eq!(entry(&bytes, path), entry(HWPX, path));
        }
        let reopened = DocumentCore::from_bytes(&bytes).unwrap();
        assert_eq!(values(&reopened), expected);
        assert_eq!(raw_script(reopened.document()), raw);
        let back = DocumentCore::from_bytes(&reopened.export_hwp_native().unwrap()).unwrap();
        assert_eq!(values(&back), expected);
    }
}

#[test]
fn hwpx_aux_scripts_and_original_refs_win_without_duplicates() {
    let core = DocumentCore::from_bytes(HWPX).unwrap();
    let mut doc = core.document().clone();
    let hpf = doc
        .hwpx_aux_entries
        .iter_mut()
        .find(|(path, _)| path == "Contents/content.hpf")
        .unwrap();
    hpf.1 = String::from_utf8(hpf.1.clone())
        .unwrap()
        .replace("headersc", "kept-header")
        .replace("sourcesc", "kept-source")
        .into_bytes();
    // 변환용 HWP 스트림이 달라도 보조 파일의 바이트와 참조 id를 덮어쓰지 않는다.
    replace_script(&mut doc, vec![1, 2, 3]);
    let bytes = serialize_hwpx(&doc).unwrap();
    assert_manifest_once(&bytes, Some(["kept-header", "kept-source"]));
    for path in PARTS {
        assert_eq!(entry(&bytes, path), entry(HWPX, path));
    }
    let reopened = DocumentCore::from_bytes(&bytes).unwrap();
    assert_eq!(info(&reopened, 4, 0)["items"], info(&core, 4, 0)["items"]);
    assert_manifest_once(
        &reopened.export_hwpx_native().unwrap(),
        Some(["kept-header", "kept-source"]),
    );
}

#[test]
fn converted_hwp_scripts_reuse_existing_manifest_and_spine_refs() {
    let core = DocumentCore::from_bytes(HWP).unwrap();
    let mut doc = core.document().clone();
    // HWP 스크립트만 있으면서 기존 참조가 남은 경우에도 같은 파트를 두 번 등록하지 않는다.
    doc.hwpx_aux_entries.push((
        "Contents/content.hpf".into(),
        entry(HWPX, "Contents/content.hpf"),
    ));
    let bytes = serialize_hwpx(&doc).unwrap();
    assert_manifest_once(&bytes, Some(["headersc", "sourcesc"]));
    let reopened = DocumentCore::from_bytes(&bytes).unwrap();
    assert_eq!(info(&reopened, 4, 0)["items"], info(&core, 4, 0)["items"]);

    // 새 스크립트 식별자는 그림·바탕쪽 식별자와도 충돌하지 않는다.
    let hpf = rhwp::serializer::hwpx::content::write_content_hpf(
        &["Contents/section0.xml".into()],
        &[rhwp::serializer::hwpx::content::BinDataEntry {
            id: "headersc".into(),
            href: "BinData/image1.png".into(),
            media_type: "image/png".into(),
            is_embedded: true,
        }],
        &[(0, "sourcesc".into(), "Contents/masterpage0.xml".into())],
        None,
        true,
    )
    .unwrap();
    let xml = String::from_utf8(hpf).unwrap();
    for id in ["headersc", "sourcesc", "headersc1", "sourcesc1"] {
        assert_eq!(xml.matches(&format!("id=\"{id}\"")).count(), 1);
    }
    for id in ["headersc1", "sourcesc1"] {
        assert_eq!(xml.matches(&format!("idref=\"{id}\"")).count(), 1);
    }
}

#[test]
fn broken_nonempty_hwp_script_is_not_silently_omitted() {
    let core = DocumentCore::from_bytes(HWP).unwrap();
    let original = raw_script(core.document());
    for broken in [vec![1, 2, 3], original[..original.len() - 1].to_vec()] {
        let mut doc = core.document().clone();
        replace_script(&mut doc, broken.clone());
        assert!(
            serialize_hwpx_with_report(&doc).is_err(),
            "스크립트를 잃은 성공 저장을 금지"
        );
        assert_eq!(script(&doc), broken, "실패 후 원문 보존");
        assert_eq!(body_text(&doc), body_text(core.document()));
    }
}

#[test]
fn distribution_hwp_script_preserves_decrypted_counted_payload_and_original_stream() {
    let bytes =
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(DISTRIBUTION_HWP))
            .unwrap();
    let core = DocumentCore::from_bytes(&bytes).unwrap();
    assert!(core.document().header.distribution);
    assert!(core.document().header.compressed);
    let original = script(core.document()).to_vec();
    assert_eq!(original.len(), 420);
    assert_eq!(&original[..4], &[0x1c, 0x00, 0x00, 0x10]);
    // 실제 원본 스트림을 별도로 복호화해 확인한 원문이다. 변환 함수의 출력으로 기대값을 만들지 않는다.
    let header =
        "var Documents = XHwpDocuments;\r\nvar Document = Documents.Active_XHwpDocument;\r\n";
    let source = "function OnDocument_New()\r\n{\r\n\t//todo : \r\n}\r\n\r\n";
    let expected = [header, source].map(|text| {
        text.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    });
    assert_eq!(expected[0].len(), 158);
    assert_eq!(expected[1].len(), 94);
    let text: Vec<_> = core
        .document()
        .sections
        .iter()
        .flat_map(|section| section.paragraphs.iter().map(|para| para.text.clone()))
        .collect();
    let saved = serialize_hwpx_with_report(core.document()).unwrap();
    assert_manifest_once(saved.bytes(), None);
    for (path, expected_part) in PARTS.into_iter().zip(&expected) {
        assert_eq!(
            entry(saved.bytes(), path),
            *expected_part,
            "{path} 원문 바이트"
        );
    }
    let package = check_package(saved.bytes(), core.document());
    assert!(package.is_ok(), "{}", package.summary());
    assert_eq!(script(core.document()), original, "원본 암호화 스트림 보존");
    assert_eq!(
        core.document()
            .sections
            .iter()
            .flat_map(|section| section.paragraphs.iter().map(|para| para.text.clone()))
            .collect::<Vec<_>>(),
        text,
        "직렬화가 원본 본문을 바꾸지 않음"
    );
    let mut counted = Vec::new();
    for part in &expected {
        counted.extend_from_slice(&(part.len() as u32 / 2).to_le_bytes());
        counted.extend_from_slice(part);
    }
    counted.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255]);
    assert_eq!(counted.len(), 272);
    let reopened = DocumentCore::from_bytes(saved.bytes()).unwrap();
    assert_eq!(
        raw_script(reopened.document()),
        counted,
        "길이·원문·꼬리 보존"
    );
    let hwp = DocumentCore::from_bytes(&reopened.export_hwp_native().unwrap()).unwrap();
    assert_eq!(raw_script(hwp.document()), counted, "HWP 역변환도 보존");
    let resaved = reopened.export_hwpx_native().unwrap();
    for path in PARTS {
        assert_eq!(
            entry(&resaved, path),
            entry(saved.bytes(), path),
            "재직렬화는 복호화된 원문을 유지"
        );
    }

    // 배포 속성이 없거나 암호화 본문이 끊긴 입력은 실패시키고 원문을 보존한다.
    let mut ordinary = core.document().clone();
    ordinary.header.distribution = false;
    assert!(serialize_hwpx_with_report(&ordinary).is_err());
    assert_eq!(script(&ordinary), original);
    let mut truncated = core.document().clone();
    replace_script(&mut truncated, original[..260].to_vec());
    assert!(serialize_hwpx_with_report(&truncated).is_err());
    assert_eq!(script(&truncated), &original[..260]);
}
