//! 차트 넣기(`insertChart`·`insertChartByPath`) 계약.
//!
//! 기준은 한컴 12.30(Mac)이 같은 조작으로 저장한 파일이다(`tests/fixtures/hancom_chart_insert`).
//! - `rchart-body.hwp/.hwpx`: 빈 문서에서 표 > 차트 만들기 → 확인(기본 데이터).
//! - `rchart-col/line/area/pie/scatter/radar.hwpx`: 같은 차트를 차트 종류 변경으로 바꾼 저장본.
//! - `rchart-cell.hwp/.hwpx`: 2×2 표의 A1 캐럿에서 차트 만들기.
//! - `rchart-fn.hwp/.hwpx`: 각주 안 캐럿에서 차트 만들기.
//!
//! 한컴은 세 자리 모두 캐럿 문단에 떠 있는 차트(32250×18750, 문단·단 기준 0, 어울림·양쪽)를
//! 넣고, 같은 OLE 저장소(레거시 `Contents` + `OOXMLChartContents`)를 쓴다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::shape::{HorzRelTo, OleShape, ShapeObject, TextWrap, VertRelTo};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/hancom_chart_insert/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn open(bytes: &[u8]) -> DocumentCore {
    DocumentCore::from_bytes(bytes).expect("문서 열기")
}

fn blank() -> DocumentCore {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().expect("빈 문서");
    core
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("JSON {e}: {text}"))
}

fn charts(core: &DocumentCore) -> Vec<serde_json::Value> {
    json(&core.list_charts_native().unwrap())
        .as_array()
        .unwrap()
        .clone()
}

fn data(core: &DocumentCore, index: usize) -> serde_json::Value {
    let value = json(&core.get_chart_data_by_index_native(index).unwrap());
    assert_eq!(value["ok"], true, "{value}");
    value
}

/// 한컴 기본 데이터: 항목 1~4 × 계열 1~3.
fn assert_default_data(value: &serde_json::Value) {
    assert_eq!(
        value["labels"],
        serde_json::json!(["항목 1", "항목 2", "항목 3", "항목 4"])
    );
    let series = value["series"].as_array().unwrap();
    assert_eq!(series.len(), 3, "{value}");
    assert_eq!(series[0]["name"], "계열 1");
    assert_eq!(
        series[0]["values"],
        serde_json::json!(["4.3", "2.5", "3.5", "4.5"])
    );
    assert_eq!(series[2]["values"], serde_json::json!(["2", "2", "3", "5"]));
}

fn oles(paragraphs: &[Paragraph]) -> Vec<&OleShape> {
    paragraphs
        .iter()
        .flat_map(|p| p.controls.iter())
        .filter_map(|c| match c {
            Control::Shape(s) => match s.as_ref() {
                ShapeObject::Ole(o) => Some(o.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// 한컴 차트 만들기의 개체 배치.
fn assert_hancom_frame(ole: &OleShape) {
    let c = &ole.common;
    assert!(!c.treat_as_char);
    assert_eq!((c.width, c.height), (32250, 18750));
    assert_eq!(
        (c.vert_rel_to, c.horz_rel_to),
        (VertRelTo::Para, HorzRelTo::Column)
    );
    assert_eq!((c.vertical_offset, c.horizontal_offset), (0, 0));
    assert_eq!(c.text_wrap, TextWrap::Square);
    assert!(c.flow_with_text);
}

/// 앱과 같은 저장 경로(HWPX 출처의 HWP 변환 포함)로 저장해 다시 연다.
fn reopen_hwp(core: &DocumentCore) -> DocumentCore {
    open(&core.export_hwp_with_adapter_snapshot().expect("HWP 저장"))
}

fn reopen_hwpx(core: &DocumentCore) -> DocumentCore {
    open(&core.export_hwpx_native().expect("HWPX 저장"))
}

#[test]
fn hancom_chart_insert_fixtures_open_with_default_data() {
    for (name, plot) in [
        ("rchart-body.hwp", "bar"),
        ("rchart-body.hwpx", "bar"),
        ("rchart-col.hwpx", "bar"),
        ("rchart-line.hwpx", "line"),
        ("rchart-area.hwpx", "area"),
        ("rchart-pie.hwpx", "pie"),
        ("rchart-scatter.hwpx", "scatter"),
        ("rchart-radar.hwpx", "radar"),
        ("rchart-cell.hwp", "bar"),
        ("rchart-cell.hwpx", "bar"),
        ("rchart-fn.hwp", "bar"),
        ("rchart-fn.hwpx", "bar"),
    ] {
        let core = open(&fixture(name));
        assert_eq!(charts(&core).len(), 1, "{name}");
        let value = data(&core, 0);
        assert_eq!(value["plot"], plot, "{name}");
        if plot != "scatter" {
            assert_default_data(&value);
        }
        // 다른 형식으로 저장해 다시 열어도 차트와 값이 남는다.
        for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
            assert_eq!(charts(&reopened).len(), 1, "{name} 재열기");
            assert_eq!(data(&reopened, 0)["plot"], plot, "{name} 재열기");
        }
    }
}

#[test]
fn insert_chart_in_blank_hwp_matches_hancom_object() {
    let hancom = open(&fixture("rchart-body.hwp"));
    let expected = oles(&hancom.document().sections[0].paragraphs)[0].clone();

    let mut core = blank();
    let result = json(&core.insert_chart_native(0, 0, 0, "", "", "").unwrap());
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["chartIndex"], 0);
    let ole = oles(&core.document().sections[0].paragraphs)[0].clone();
    assert_hancom_frame(&ole);
    assert_eq!(ole.common.attr, expected.common.attr);
    assert_eq!(ole.common.instance_id, expected.common.instance_id);
    let (ours, theirs) = (&ole.drawing.shape_attr, &expected.drawing.shape_attr);
    assert_eq!(
        (ours.original_width, ours.current_width, ours.flip),
        (theirs.original_width, theirs.current_width, theirs.flip)
    );
    assert_eq!(ole.chart_id_ref, None);
    assert_default_data(&data(&core, 0));
    // 렌더 경로가 차트를 그린다.
    let svg = core.render_page_svg_native(0).unwrap();
    assert!(
        svg.contains("항목 1") && svg.contains("계열 3"),
        "차트가 그려져야 한다"
    );

    for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
        assert_eq!(charts(&reopened).len(), 1);
        assert_default_data(&data(&reopened, 0));
        let again = oles(&reopened.document().sections[0].paragraphs)[0].clone();
        assert_hancom_frame(&again);
    }
    // HWP 저장본의 OLE 개체 레코드와 BinData 는 한컴 저장본과 같은 모양이다.
    let hwp = reopen_hwp(&core);
    let again = oles(&hwp.document().sections[0].paragraphs)[0].clone();
    assert_eq!(again.common.attr, expected.common.attr);
    assert_eq!(
        (again.extent_x, again.extent_y),
        (expected.extent_x, expected.extent_y)
    );
    assert_eq!(
        hwp.document().doc_info.bin_data_list[0].data_type,
        hancom.document().doc_info.bin_data_list[0].data_type
    );
}

#[test]
fn insert_chart_with_type_and_data_round_trips() {
    let edits = serde_json::json!({
        "structure": true,
        "labels": ["1분기", "2분기"],
        "series": [
            {"name": "매출", "values": ["10", "20.5"]},
            {"name": "비용", "values": ["7", "8"]}
        ]
    })
    .to_string();
    for (kind, plot) in [
        ("column", "bar"),
        ("line", "line"),
        ("area", "area"),
        ("pie", "pie"),
        ("radar", "radar"),
    ] {
        let mut core = blank();
        let result = json(&core.insert_chart_native(0, 0, 0, kind, &edits, "").unwrap());
        assert_eq!(result["ok"], true, "{kind}: {result}");
        for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
            let value = data(&reopened, 0);
            assert_eq!(value["plot"], plot, "{kind}");
            assert_eq!(
                value["labels"],
                serde_json::json!(["1분기", "2분기"]),
                "{kind}"
            );
            assert_eq!(value["series"][0]["name"], "매출", "{kind}");
            assert_eq!(
                value["series"][1]["values"],
                serde_json::json!(["7", "8"]),
                "{kind}"
            );
        }
    }
    // 크기 옵션.
    let mut core = blank();
    core.insert_chart_native(0, 0, 0, "scatter", "", r#"{"width":20000,"height":10000}"#)
        .unwrap();
    let ole = oles(&core.document().sections[0].paragraphs)[0].clone();
    assert_eq!((ole.common.width, ole.common.height), (20000, 10000));
    assert_eq!(data(&reopen_hwpx(&core), 0)["plot"], "scatter");
}

/// 한컴 12.30 차트 데이터 편집 창의 "차트 제목": 한컴 기본 차트는 자동 제목 "차트 제목"을 보이고,
/// 고친 제목은 `c:tx/c:rich` 로 써서 HWP·HWPX 에 남는다. 같은 제목은 쓰지 않고, 빈 제목은 지운다.
#[test]
fn chart_data_title_reads_and_writes_like_hancom_data_dialog() {
    let mut core = blank();
    core.insert_chart_native(0, 0, 0, "bar", "", "").unwrap();
    assert_eq!(data(&core, 0)["title"], "차트 제목");
    let defaults = data(&core, 0);
    let edits = |title: &str| {
        serde_json::json!({
            "series": defaults["series"].as_array().unwrap().iter().map(|s| {
                serde_json::json!({ "values": s["values"].clone() })
            }).collect::<Vec<_>>(),
            "title": title,
        })
        .to_string()
    };
    let same = json(
        &core
            .set_chart_data_by_index_native(0, &edits("차트 제목"))
            .unwrap(),
    );
    assert_eq!(same["changedCount"], 0, "{same}");
    let wrote = json(
        &core
            .set_chart_data_by_index_native(0, &edits("성적표"))
            .unwrap(),
    );
    assert_eq!(wrote["ok"], true, "{wrote}");
    assert_eq!(data(&core, 0)["title"], "성적표");
    for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
        assert_eq!(data(&reopened, 0)["title"], "성적표");
        assert_default_data(&data(&reopened, 0));
    }
    let refused = json(
        &core
            .set_chart_data_by_index_native(0, &edits("a<b"))
            .unwrap(),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(data(&core, 0)["title"], "성적표");
    core.set_chart_data_by_index_native(0, &edits("")).unwrap();
    assert!(data(&core, 0)["title"].is_null());
    assert!(data(&reopen_hwpx(&core), 0)["title"].is_null());
}

#[test]
fn insert_chart_refuses_bad_input_without_changing_the_document() {
    let mut core = blank();
    let bad =
        r#"{"structure":true,"labels":["가"],"series":[{"name":"a","values":["숫자 아님"]}]}"#;
    let result = json(&core.insert_chart_native(0, 0, 0, "", bad, "").unwrap());
    assert_eq!(result["ok"], false, "{result}");
    assert!(charts(&core).is_empty());
    assert!(core.insert_chart_native(0, 0, 0, "bubble", "", "").is_err());
    assert!(core
        .insert_chart_native(0, 0, 0, "", "", r#"{"width":1}"#)
        .is_err());
    assert!(core.insert_chart_native(0, 9, 0, "", "", "").is_err());
    assert!(charts(&core).is_empty());
    assert!(core.document().doc_info.bin_data_list.is_empty());
}

#[test]
fn insert_chart_into_hwpx_document_writes_hancom_switch() {
    let mut core = open(&fixture("rchart-body.hwpx"));
    let result = json(&core.insert_chart_native(0, 0, 0, "line", "", "").unwrap());
    assert_eq!(result["ok"], true, "{result}");
    let list = charts(&core);
    assert_eq!(list.len(), 2);
    let new = &list[result["chartIndex"].as_u64().unwrap() as usize];
    assert!(
        new["zipPart"].is_number() && new["nestedCopy"].is_number(),
        "{new}"
    );
    let added = oles(&core.document().sections[0].paragraphs)
        .into_iter()
        .find(|o| o.chart_id_ref.as_deref() == Some("Chart/chart2.xml"))
        .expect("새 차트는 Chart/chart2.xml 을 가리킨다")
        .clone();
    assert_hancom_frame(&added);
    assert!(added.chart_switch_fallback.is_some());

    let bytes = core.export_hwpx_native().unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    assert!(zip.by_name("Chart/chart2.xml").is_ok(), "새 차트 파트");
    let reopened = open(&bytes);
    assert_eq!(charts(&reopened).len(), 2);
    let plots: Vec<_> = (0..2).map(|i| data(&reopened, i)["plot"].clone()).collect();
    assert!(
        plots.contains(&serde_json::json!("line")) && plots.contains(&serde_json::json!("bar"))
    );

    let hwp = reopen_hwp(&core);
    assert_eq!(charts(&hwp).len(), 2);
    for i in 0..2 {
        assert_default_data(&data(&hwp, i));
    }
}

#[test]
fn insert_chart_by_path_in_table_cell_and_footnote() {
    for name in ["rchart-cell.hwpx", "rchart-cell.hwp"] {
        let mut core = open(&fixture(name));
        // 2×2 표의 B1(셀 1) 첫 문단.
        let (para, ctrl) = core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(p, para)| {
                para.controls
                    .iter()
                    .position(|c| matches!(c, Control::Table(_)))
                    .map(|c| (p, c))
            })
            .expect("표");
        let path = [(ctrl, 1, 0)];
        let result = json(
            &core
                .insert_chart_by_path_native(0, para, &path, 0, "pie", "", "")
                .unwrap(),
        );
        assert_eq!(result["ok"], true, "{name}: {result}");
        let index = result["chartIndex"].as_u64().unwrap() as usize;
        let chart = &charts(&core)[index];
        assert_eq!(
            chart["container"][0]["kind"], "tableCell",
            "{name}: {chart}"
        );
        assert_eq!(chart["container"][0]["cell"], 1, "{name}: {chart}");
        for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
            assert_eq!(charts(&reopened).len(), 2, "{name}");
            let plots: Vec<_> = (0..2).map(|i| data(&reopened, i)["plot"].clone()).collect();
            assert!(
                plots.contains(&serde_json::json!("pie")),
                "{name}: {plots:?}"
            );
        }
    }
    for name in ["rchart-fn.hwpx", "rchart-fn.hwp"] {
        let mut core = open(&fixture(name));
        let (para, ctrl) = core.document().sections[0]
            .paragraphs
            .iter()
            .enumerate()
            .find_map(|(p, para)| {
                para.controls
                    .iter()
                    .position(|c| matches!(c, Control::Footnote(_)))
                    .map(|c| (p, c))
            })
            .expect("각주");
        let path = [(ctrl, 0, 0)];
        let result = json(
            &core
                .insert_chart_by_path_native(0, para, &path, 0, "column", "", "")
                .unwrap(),
        );
        assert_eq!(result["ok"], true, "{name}: {result}");
        let index = result["chartIndex"].as_u64().unwrap() as usize;
        assert_eq!(
            charts(&core)[index]["container"][0]["kind"],
            "footnote",
            "{name}"
        );
        for reopened in [reopen_hwp(&core), reopen_hwpx(&core)] {
            assert_eq!(charts(&reopened).len(), 2, "{name}");
            for i in 0..2 {
                assert_default_data(&data(&reopened, i));
            }
        }
    }
}

/// 주석 안 차트는 쪽 개체 목록에 본문 문단을 뿌리로 한 주석 경로로 나와야 두 번 눌러 고칠 차트를
/// 하나로 정할 수 있다. 주석 문단 주소(문단 0·컨트롤 0)만 내면 두 각주의 차트가 같은 주소가 되고,
/// 미주 차트는 미주 글줄의 가상 문단 번호로 나와 문서의 어느 차트와도 맞지 않는다.
#[test]
fn note_charts_report_note_path_in_page_controls() {
    let mut core = blank();
    core.insert_text_native(0, 0, 0, "ab").unwrap();
    core.insert_footnote_native(0, 0, 0).unwrap();
    core.insert_footnote_native(0, 0, 0).unwrap();
    core.insert_endnote_native(0, 0, 0).unwrap();
    let notes: Vec<(usize, &str)> = core.document().sections[0].paragraphs[0]
        .controls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            Control::Footnote(_) => Some((i, "footnote")),
            Control::Endnote(_) => Some((i, "endnote")),
            _ => None,
        })
        .collect();
    assert_eq!(notes.len(), 3);
    for &(ctrl, _) in &notes {
        let result = json(
            &core
                .insert_chart_by_path_native(0, 0, &[(ctrl, 0, 0)], 0, "bar", "", "")
                .unwrap(),
        );
        assert_eq!(result["ok"], true, "{result}");
    }
    let listed: Vec<(u64, String)> = charts(&core)
        .iter()
        .map(|c| {
            let level = &c["container"][0];
            (
                level["control"].as_u64().unwrap(),
                level["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let mut want: Vec<(u64, String)> = notes
        .iter()
        .map(|&(ctrl, kind)| (ctrl as u64, kind.to_string()))
        .collect();
    let mut listed = listed;
    listed.sort();
    want.sort();
    assert_eq!(listed, want);

    let mut reported: Vec<u64> = Vec::new();
    for page in 0..core.page_count() {
        let layout = json(&core.get_page_control_layout_native(page).unwrap());
        for control in layout["controls"].as_array().unwrap() {
            if control["type"] != "ole" {
                continue;
            }
            assert_eq!(control["secIdx"], 0, "{control}");
            assert_eq!(control["parentParaIdx"], 0, "{control}");
            let path = control["cellPath"].as_array().expect("주석 경로");
            assert_eq!(path.len(), 1, "{control}");
            assert_eq!(path[0]["cellIndex"], 0, "{control}");
            assert_eq!(path[0]["cellParaIndex"], 0, "{control}");
            reported.push(path[0]["controlIndex"].as_u64().unwrap());
        }
    }
    reported.sort();
    let mut controls: Vec<u64> = notes.iter().map(|&(c, _)| c as u64).collect();
    controls.sort();
    assert_eq!(reported, controls);
}

/// 표 칸 안 각주·미주의 차트도 쪽 개체 목록에 [표 칸, (주석, 0, 주석 문단)] 경로로 나온다.
#[test]
fn table_cell_note_charts_report_cell_and_note_path() {
    let mut core = blank();
    let table = json(&core.create_table_native(0, 0, 0, 1, 2).unwrap());
    let host = table["paraIdx"].as_u64().unwrap() as usize;
    let ctrl = table["controlIdx"].as_u64().unwrap() as usize;
    let footnote = json(
        &core
            .insert_footnote_by_path_native(0, host, &[(ctrl, 0, 0)], 0)
            .unwrap(),
    );
    let endnote = json(
        &core
            .insert_endnote_by_path_native(0, host, &[(ctrl, 1, 0)], 0)
            .unwrap(),
    );
    let mut want = Vec::new();
    for (cell, note) in [(0usize, &footnote), (1, &endnote)] {
        let note_ctrl = note["controlIdx"].as_u64().unwrap() as usize;
        let path = [(ctrl, cell, 0), (note_ctrl, 0, 0)];
        let result = json(
            &core
                .insert_chart_by_path_native(0, host, &path, 0, "bar", "", "")
                .unwrap(),
        );
        assert_eq!(result["ok"], true, "{result}");
        want.push(serde_json::json!([
            {"controlIndex": ctrl, "cellIndex": cell, "cellParaIndex": 0},
            {"controlIndex": note_ctrl, "cellIndex": 0, "cellParaIndex": 0},
        ]));
    }
    let mut reported = Vec::new();
    for page in 0..core.page_count() {
        let layout = json(&core.get_page_control_layout_native(page).unwrap());
        for control in layout["controls"].as_array().unwrap() {
            if control["type"] == "ole" {
                assert_eq!(control["parentParaIdx"], host as u64, "{control}");
                reported.push(control["cellPath"].clone());
            }
        }
    }
    reported.sort_by_key(|path| path.to_string());
    want.sort_by_key(|path| path.to_string());
    assert_eq!(reported, want);
}

/// 한컴 기본 원형은 점(`c:dPt`)마다 테마 색을 적는다. 첫 점 색을 계열 색으로 올리면 모든 조각이
/// 한 색으로 그려진다 — 조각마다 다른 색이어야 한다.
#[test]
fn hancom_pie_slices_render_in_distinct_colors() {
    let colors = |core: &DocumentCore| {
        let svg = core.render_page_svg_native(0).unwrap();
        let mut set = std::collections::BTreeSet::new();
        for part in svg.split("<path ").skip(1) {
            if !part.contains(" A") {
                continue;
            }
            if let Some(rest) = part.split("fill=\"").nth(1) {
                set.insert(rest[..7].to_string());
            }
        }
        set
    };
    let hancom = open(&fixture("rchart-pie.hwpx"));
    assert!(colors(&hancom).len() >= 4, "{:?}", colors(&hancom));
    let mut core = blank();
    core.insert_chart_native(0, 0, 0, "pie", "", "").unwrap();
    assert!(colors(&core).len() >= 4, "{:?}", colors(&core));
}
