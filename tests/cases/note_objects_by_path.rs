//! 관계형. 각주·미주(본문·표 칸 안 주석) 경로로 표·그림·수식·도형을 넣고, 주석 영역에 그리며,
//! 쪽 개체 목록·커서·히트 테스트가 본문 문단이 뿌리인 주석 경로를 주고, HWP·HWPX 저장 뒤에도 남는다.
//! 한/글 12.30(Mac)은 각주 안에서 표 만들기를 글자처럼 취급을 끈 채 각주 글 아래 줄에 두고 A1로
//! 들어가며, 수식은 캐럿 자리에 글자처럼 넣는다. 그 저장본(`fn173-table.hwpx`·`fn173-eq.hwpx`)도 연다.

use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

type Path = Vec<(usize, usize, usize)>;

fn png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn json_path(path: &[(usize, usize, usize)]) -> String {
    serde_json::json!(path
        .iter()
        .map(|&(c, cell, p)| serde_json::json!({
            "controlIndex": c, "cellIndex": cell, "cellParaIndex": p
        }))
        .collect::<Vec<_>>())
    .to_string()
}

fn parse_path(value: &Value) -> Path {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let n = |k: &str| e[k].as_u64().unwrap() as usize;
            (n("controlIndex"), n("cellIndex"), n("cellParaIndex"))
        })
        .collect()
}

/// 본문 문단 `para` 에서 셀 경로를 따라간 문단.
fn path_para<'a>(
    doc: &'a HwpDocument,
    para: usize,
    path: &[(usize, usize, usize)],
) -> &'a Paragraph {
    let mut p = &doc.document().sections[0].paragraphs[para];
    for &(c, cell, cp) in path {
        p = match &p.controls[c] {
            Control::Footnote(n) => &n.paragraphs[cp],
            Control::Endnote(n) => &n.paragraphs[cp],
            Control::Table(t) => &t.cells[cell].paragraphs[cp],
            other => panic!("경로 마디가 컨테이너가 아니다: {other:?}"),
        };
    }
    p
}

/// 주석 문단 목록(경로 마지막 마디가 주석).
fn note_paras<'a>(
    doc: &'a HwpDocument,
    para: usize,
    note: &[(usize, usize, usize)],
) -> &'a [Paragraph] {
    let (last, outer) = note.split_last().unwrap();
    let host = if outer.is_empty() {
        &doc.document().sections[0].paragraphs[para]
    } else {
        path_para(doc, para, outer)
    };
    match &host.controls[last.0] {
        Control::Footnote(n) => &n.paragraphs,
        Control::Endnote(n) => &n.paragraphs,
        other => panic!("주석이 아니다: {other:?}"),
    }
}

fn controls(doc: &HwpDocument, page: u32) -> Vec<Value> {
    let v: Value = serde_json::from_str(&doc.get_page_control_layout(page).unwrap()).unwrap();
    v["controls"].as_array().unwrap().clone()
}

/// 쪽 개체 목록에서 `kind` 개체 가운데 cellPath 가 `path` 이고 controlIdx 가 `control` 인 것.
fn find_control(
    list: &[Value],
    kind: &str,
    path: &[(usize, usize, usize)],
    control: usize,
) -> Value {
    list.iter()
        .find(|c| {
            c["type"] == kind
                && c["controlIdx"] == control
                && c.get("cellPath").is_some_and(|p| parse_path(p) == path)
        })
        .cloned()
        .unwrap_or_else(|| panic!("{kind} {path:?}#{control} 없음: {list:?}"))
}

struct Inserted {
    /// 표 첫 칸(A1) 경로.
    a1: Path,
    /// 표 문단 경로(마지막 마디가 표를 담은 주석 문단).
    table_para: Path,
    picture: usize,
    equation: usize,
    shape: usize,
}

/// 주석 첫 문단 글 끝에 표(한/글 12.30 각주 표 기본값: 글자처럼 취급 끔)를, 번호 뒤에 그림·수식·도형을
/// WASM 공개 API로 넣는다.
fn insert_objects(doc: &mut HwpDocument, para: usize, note: &Path) -> Inserted {
    let end = path_para(doc, para, note).text.chars().count() as u32;
    let table: Value = serde_json::from_str(
        &doc.create_table_in_cell_by_path(
            0,
            para as u32,
            &json_path(note),
            end,
            r#"{"rowCount":2,"colCount":2,"treatAsChar":false}"#,
        )
        .unwrap(),
    )
    .unwrap();
    let a1 = parse_path(&table["cellPath"]);
    doc.insert_text_in_cell_by_path_api(0, para as u32, &json_path(&a1), 0, "칸")
        .unwrap();

    let picture: Value = serde_json::from_str(
        &doc.insert_picture_in_cell(
            0,
            para as u32,
            2,
            &json_path(note),
            &png(),
            1500,
            1500,
            1,
            1,
            "png",
            "",
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    let equation: Value = serde_json::from_str(
        &doc.insert_equation_by_path(0, para as u32, &json_path(note), 3, "a over b", 1000, 0)
            .unwrap(),
    )
    .unwrap();
    let shape: Value = serde_json::from_str(
        &doc.create_shape_control(
            &serde_json::json!({
                "sectionIdx": 0, "paraIdx": para, "charOffset": 4,
                "width": 2000, "height": 1000, "shapeType": "rectangle",
                "cellPath": serde_json::from_str::<Value>(&json_path(note)).unwrap(),
            })
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut table_para = note.clone();
    table_para.last_mut().unwrap().2 = table["cellParaIdx"].as_u64().unwrap() as usize;
    Inserted {
        a1,
        table_para,
        picture: picture["controlIdx"].as_u64().unwrap() as usize,
        equation: equation["controlIdx"].as_u64().unwrap() as usize,
        shape: shape["controlIdx"].as_u64().unwrap() as usize,
    }
}

/// 넣은 개체가 주석 문단에 있고(그림·도형은 글자처럼 취급), 표는 한/글처럼 글 아래 줄의 자리 차지
/// 표이며 그 뒤에 빈 문단이 있다.
fn assert_model(doc: &HwpDocument, para: usize, note: &Path, got: &Inserted, text: &str) {
    let paras = note_paras(doc, para, note);
    assert_eq!(paras.len(), 3, "글·표·빈 문단");
    assert!(paras[0].text.contains(text), "{:?}", paras[0].text);
    assert!(matches!(
        &paras[0].controls[got.picture],
        Control::Picture(p) if p.common.treat_as_char
    ));
    assert!(matches!(
        &paras[0].controls[got.equation],
        Control::Equation(_)
    ));
    assert!(matches!(
        &paras[0].controls[got.shape],
        Control::Shape(s) if s.common().treat_as_char
    ));
    let Control::Table(table) = &paras[1].controls[0] else {
        panic!("표 문단");
    };
    assert!(!table.common.treat_as_char);
    let page = &doc.document().sections[0].section_def.page_def;
    let text_width = page.width - page.margin_left - page.margin_right;
    // 한/글 12.30 각주 표: 폭 = 주석 영역 폭 − 바깥 여백(283 × 2), 세로 문단·가로 단 기준.
    assert_eq!(table.common.width, text_width - 566);
    assert!(paras[2].text.is_empty() && paras[2].controls.is_empty());
    assert_eq!(table.cells[0].paragraphs[0].text, "칸");
}

/// 쪽 개체 목록이 주석 안 개체를 본문 문단이 뿌리인 주석 경로로 주고, 그 자리가 `area` 안이다.
fn assert_layout(
    doc: &HwpDocument,
    page: u32,
    para: usize,
    note: &Path,
    got: &Inserted,
    area: (f64, f64),
) {
    let list = controls(doc, page);
    let mut first = note.clone();
    first.last_mut().unwrap().2 = 0;
    for (kind, control, path) in [
        ("image", got.picture, &first),
        ("equation", got.equation, &first),
        ("shape", got.shape, &first),
        ("table", 0, &got.table_para),
    ] {
        let c = find_control(&list, kind, path, control);
        assert_eq!(c["secIdx"], 0, "{c}");
        // 본문 뿌리 문단: 그림·수식·표는 parentParaIdx, 도형은 paraIdx(칸 도형과 같은 꼴).
        // 미주 수식의 paraIdx 는 미주 가상 문단 번호(`noteRef` 와 함께)로 남는다.
        assert_eq!(c.get("parentParaIdx").unwrap_or(&c["paraIdx"]), para, "{c}");
        let y = c["y"].as_f64().unwrap();
        assert!(y > area.0 && y < area.1, "{kind} y={y} 영역 {area:?}");
    }
    // 수식 속성도 그 주소로 읽고 바꾼다.
    let eq = doc
        .get_equation_properties_by_path(0, para as u32, &json_path(&first), got.equation as u32)
        .unwrap();
    assert!(eq.contains("\"script\":\"a over b\""), "{eq}");
}

/// A1 캐럿은 주석 영역 안 표 칸에 서고, 그 자리를 누르면 같은 A1 경로가 돌아온다.
fn assert_a1_caret(doc: &HwpDocument, para: usize, got: &Inserted, footnote: bool) {
    let rect: Value = serde_json::from_str(
        &doc.get_cursor_rect_by_path(0, para as u32, &json_path(&got.a1), 1)
            .unwrap(),
    )
    .unwrap();
    let list = controls(doc, rect["pageIndex"].as_u64().unwrap() as u32);
    let table = find_control(&list, "table", &got.table_para, 0);
    let cell = &table["cells"][0];
    let (cx, cy, cw, ch) = (
        cell["x"].as_f64().unwrap(),
        cell["y"].as_f64().unwrap(),
        cell["w"].as_f64().unwrap(),
        cell["h"].as_f64().unwrap(),
    );
    let (x, y) = (rect["x"].as_f64().unwrap(), rect["y"].as_f64().unwrap());
    assert!(
        x >= cx && x <= cx + cw && y >= cy && y <= cy + ch,
        "{rect} 칸 {cell}"
    );
    let page = rect["pageIndex"].as_u64().unwrap() as u32;
    let hit: Value = serde_json::from_str(
        &if footnote {
            doc.hit_test_in_footnote(page, cx + 4.0, cy + ch / 2.0)
        } else {
            doc.hit_test(page, cx + 4.0, cy + ch / 2.0)
        }
        .unwrap(),
    )
    .unwrap();
    assert_eq!(parse_path(&hit["cellPath"]), got.a1, "{hit}");
    assert_eq!(hit["parentParaIndex"], para, "{hit}");
}

fn reopen_all(doc: &HwpDocument) -> Vec<HwpDocument> {
    vec![
        HwpDocument::from_bytes(&doc.export_hwp_native().unwrap()).unwrap(),
        HwpDocument::from_bytes(&doc.export_hwpx_native().unwrap()).unwrap(),
    ]
}

fn body_note(kind: &str) -> (HwpDocument, Path) {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc.insert_text_native(0, 0, 0, "본문").unwrap();
    if kind == "footnote" {
        doc.insert_footnote_native(0, 0, 1).unwrap();
    } else {
        doc.insert_endnote_native(0, 0, 1).unwrap();
    }
    let ci = doc.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .position(|c| matches!(c, Control::Footnote(_) | Control::Endnote(_)))
        .unwrap();
    // 본문 주석 글은 앱처럼 주석 API로 친다(번호 뒤 자리 2).
    doc.insert_text_in_footnote_native(0, 0, ci, 0, 2, "주석")
        .unwrap();
    (doc, vec![(ci, 0, 0)])
}

#[test]
fn hancom_note_object_samples_open_render_and_round_trip() {
    for (name, kind, note_para) in [
        ("fn173-table", "table", 1usize),
        ("fn173-eq", "equation", 0),
    ] {
        let bytes = std::fs::read(format!("tests/fixtures/{name}.hwpx")).unwrap();
        let doc = HwpDocument::from_bytes(&bytes).unwrap();
        let ci = doc.document().sections[0].paragraphs[0]
            .controls
            .iter()
            .position(|c| matches!(c, Control::Footnote(_)))
            .unwrap();
        let path = vec![(ci, 0, note_para)];
        for doc in std::iter::once(doc).chain(reopen_all(&HwpDocument::from_bytes(&bytes).unwrap()))
        {
            let note = note_paras(&doc, 0, &path);
            let control = note[note_para]
                .controls
                .iter()
                .position(|c| matches!(c, Control::Table(_) | Control::Equation(_)))
                .unwrap_or_else(|| panic!("{name}: 주석 안 개체가 없다"));
            let c = find_control(&controls(&doc, 0), kind, &path, control);
            // 각주 영역(쪽 아래쪽)에 그린다.
            assert!(c["y"].as_f64().unwrap() > 900.0, "{name} {c}");
            assert!(doc.render_page_svg_native(0).is_ok());
        }
    }
}

#[test]
fn body_footnote_objects_by_path() {
    let (mut doc, note) = body_note("footnote");
    let got = insert_objects(&mut doc, 0, &note);
    for doc in std::iter::once(&doc).chain(reopen_all(&doc).iter()) {
        assert_model(doc, 0, &note, &got, "주석");
        assert_layout(doc, 0, 0, &note, &got, (800.0, 1123.0));
        // 번호와 본문은 그대로다.
        let Control::Footnote(fnote) =
            &doc.document().sections[0].paragraphs[0].controls[note[0].0]
        else {
            panic!("각주");
        };
        assert_eq!(fnote.number, 1);
        assert_eq!(doc.document().sections[0].paragraphs[0].text, "본문");
        assert_eq!(doc.page_count(), 1);
    }
    assert_a1_caret(&doc, 0, &got, true);
}

#[test]
fn body_endnote_objects_by_path() {
    let (mut doc, note) = body_note("endnote");
    let got = insert_objects(&mut doc, 0, &note);
    for doc in std::iter::once(&doc).chain(reopen_all(&doc).iter()) {
        assert_model(doc, 0, &note, &got, "주석");
        // 미주는 본문 끝 뒤에 그린다.
        assert_layout(doc, 0, 0, &note, &got, (120.0, 600.0));
    }
    assert_a1_caret(&doc, 0, &got, false);
}

#[test]
fn table_cell_footnote_objects_by_path() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let t: Value = serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let (tp, tc) = (
        t["paraIdx"].as_u64().unwrap() as usize,
        t["controlIdx"].as_u64().unwrap() as usize,
    );
    doc.insert_text_in_cell_by_path(0, tp, &[(tc, 0, 0)], 0, "셀")
        .unwrap();
    doc.insert_footnote_by_path_native(0, tp, &[(tc, 0, 0)], 1)
        .unwrap();
    let note = vec![(tc, 0, 0), (0, 0, 0)];
    doc.insert_text_in_cell_by_path(0, tp, &note, 2, "칸주석")
        .unwrap();
    let got = insert_objects(&mut doc, tp, &note);
    assert_eq!(got.a1, vec![(tc, 0, 0), (0, 0, 1), (0, 0, 0)]);
    for doc in std::iter::once(&doc).chain(reopen_all(&doc).iter()) {
        assert_model(doc, tp, &note, &got, "칸주석");
        assert_layout(doc, 0, tp, &note, &got, (800.0, 1123.0));
    }
    assert_a1_caret(&doc, tp, &got, true);
}

/// 수식 속성은 주석 경로로 바꾸고, 도형 경로는 주석이 아닌 컨테이너를 받지 않는다.
#[test]
fn note_equation_properties_and_shape_path_guard() {
    let (mut doc, note) = body_note("footnote");
    let eq: Value = serde_json::from_str(
        &doc.insert_equation_by_path(0, 0, &json_path(&note), 2, "x", 1000, 0)
            .unwrap(),
    )
    .unwrap();
    let ci = eq["controlIdx"].as_u64().unwrap() as u32;
    doc.set_equation_properties_by_path(0, 0, &json_path(&note), ci, r#"{"script":"x over y"}"#)
        .unwrap();
    let props = doc
        .get_equation_properties_by_path(0, 0, &json_path(&note), ci)
        .unwrap();
    assert!(props.contains("\"script\":\"x over y\""), "{props}");

    let t: Value = serde_json::from_str(&doc.create_table_native(0, 0, 0, 1, 1).unwrap()).unwrap();
    let cell = [(t["controlIdx"].as_u64().unwrap() as usize, 0, 0)];
    let rejected = doc.create_shape_control_by_path_native(
        0,
        t["paraIdx"].as_u64().unwrap() as usize,
        &cell,
        0,
        2000,
        1000,
        "rectangle",
        (false, false),
        &[],
    );
    assert!(rejected.is_err());
}

/// 주석 글 가운데 넣은 그림 뒤 글도 곧바로 그린다(다시 열기 전에도). 그림은 문단 길이에 8칸을 더한다.
#[test]
fn note_picture_keeps_following_text_drawn() {
    for kind in ["footnote", "endnote"] {
        let (mut doc, note) = body_note(kind);
        let before = note_paras(&doc, 0, &note)[0].char_count;
        doc.insert_picture_in_cell_native(
            0,
            0,
            3,
            &note,
            &png(),
            1000,
            1000,
            1,
            1,
            "png",
            "그림",
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            note_paras(&doc, 0, &note)[0].char_count,
            before + 8,
            "{kind}"
        );
        let svg: String = (0..doc.page_count())
            .map(|page| doc.render_page_svg_native(page).unwrap())
            .collect();
        assert!(svg.contains(">석</text>"), "{kind}: 그림 뒤 글이 없다");
    }
}

/// 주석 안 표도 셀 경로로 셀 정보를 읽고 계산식을 다시 계산한다(주석 마디를 지나 표를 찾는다).
#[test]
fn note_table_cell_info_and_formulas_by_path() {
    for kind in ["footnote", "endnote"] {
        let (mut doc, note) = body_note(kind);
        let got = insert_objects(&mut doc, 0, &note);
        let info: Value = serde_json::from_str(
            &doc.get_cell_info_by_path(0, 0, &json_path(&got.a1))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (info["row"].as_u64(), info["col"].as_u64()),
            (Some(0), Some(0)),
            "{kind}"
        );
        let mut table = got.a1.clone();
        table.last_mut().unwrap().2 = 0;
        doc.recalculate_table_formulas_by_path(0, 0, &json_path(&table), 0, true)
            .unwrap();
    }
}
