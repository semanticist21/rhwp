//! 관계형·고정형. rhwp 가 낸 줄뿐인 문단도 문단 기준 어울림(Square) 그림 옆으로 접는다 — 본문과
//! 각주 모두, 열 때와 개체를 붙일 때, HWP·HWPX 저장 뒤에도.
//!
//! `square-float-reflow/rhwp-saved.hwp`·`.hwpx` 는 rhwp 가 저장한 문서다. 첫 문단(구역 정의)에
//! 9600×9600 어울림 그림(문단·단 기준 0, 양쪽), 둘째 문단 「본문 문단」에 각주 하나, 각주 문단에도 같은
//! 어울림 그림이 있다. HWP 는 구현 태그 줄을 싣는데 host 만 깎이고 뒤 문단·각주는 전폭이었다. HWPX 는
//! 줄이 없다.
//!
//! 정본은 한/글 12.30(Mac)이다. 같은 두 파일의 PDF(`pdftocairo -svg`)에서 그림은 x=85.08pt, 본문 「본문」
//! 과 각주 「1) 주석…」의 첫 글자는 모두 x=181.08pt(그림 오른쪽 끝)에서 시작한다. 한/글로 다시 저장한
//! HWP·HWPX 는 첫 문단·「본문 문단」·각주 문단의 줄을 모두 horzpos=9600 horzsize=32920 으로 쓴다.

use rhwp::model::control::Control;
use rhwp::model::paragraph::Paragraph;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

const RHWP_HWP: &[u8] = include_bytes!("../fixtures/square-float-reflow/rhwp-saved.hwp");
const RHWP_HWPX: &[u8] = include_bytes!("../fixtures/square-float-reflow/rhwp-saved.hwpx");
/// 한/글 다시 저장본의 줄 조각(horzpos, horzsize).
const BESIDE: (i32, i32) = (9600, 32920);
/// 한/글 PDF 의 본문 「본」·각주 「주」 x(pt). 각주 번호 「1)」 은 181.08pt 에서 시작한다.
const BODY_X_PT: f64 = 181.08;
const NOTE_X_PT: f64 = 194.76;

fn pt(v: f64) -> f64 {
    v * 96.0 / 72.0
}

fn first_row(para: &Paragraph) -> Option<(i32, i32)> {
    para.line_segs
        .first()
        .map(|row| (row.column_start, row.segment_width))
}

fn note_paragraphs(para: &Paragraph) -> &[Paragraph] {
    para.controls
        .iter()
        .find_map(|c| match c {
            Control::Footnote(n) => Some(n.paragraphs.as_slice()),
            _ => None,
        })
        .expect("각주")
}

/// 첫 문단·본문 글 문단·각주 문단의 첫 줄 조각.
fn rows(doc: &HwpDocument) -> Vec<Option<(i32, i32)>> {
    let paras = &doc.document().sections[0].paragraphs;
    let body = paras
        .iter()
        .position(|p| p.text.contains("본문"))
        .expect("본문 문단");
    vec![
        first_row(&paras[0]),
        first_row(&paras[body]),
        first_row(&note_paragraphs(&paras[body])[0]),
    ]
}

/// 1쪽에서 글자 `ch` 를 담은 첫 run 의 x(px).
fn run_x(doc: &HwpDocument, ch: char) -> f64 {
    let text: Value = serde_json::from_str(&doc.get_page_text_layout(0).unwrap()).unwrap();
    text["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["text"].as_str().is_some_and(|t| t.contains(ch)))
        .and_then(|r| r["x"].as_f64())
        .unwrap_or_else(|| panic!("「{ch}」 run"))
}

fn assert_beside(doc: &HwpDocument, label: &str) {
    for (ch, what, expected) in [('본', "본문", BODY_X_PT), ('주', "각주", NOTE_X_PT)] {
        let x = run_x(doc, ch);
        assert!(
            (x - pt(expected)).abs() < 1.0,
            "{label}: {what} 「{ch}」 은 한/글처럼 어울림 그림 옆 {:.1}px 에 놓인다: {x:.1}",
            pt(expected)
        );
    }
    assert_eq!(
        rows(doc),
        vec![Some(BESIDE); 3],
        "{label}: 첫 문단·본문·각주 줄은 한/글 다시 저장본처럼 그림 옆 조각이다"
    );
}

#[test]
fn rhwp_rows_wrap_beside_square_pictures_in_body_and_footnote() {
    for (bytes, label) in [(RHWP_HWP, "HWP"), (RHWP_HWPX, "HWPX")] {
        let doc = HwpDocument::from_bytes(bytes).expect("open");
        assert_beside(&doc, label);
        let hwp = HwpDocument::from_bytes(&doc.export_hwp().unwrap()).expect("HWP 다시 열기");
        assert_beside(&hwp, &format!("{label}→HWP"));
        let hwpx = HwpDocument::from_bytes(&doc.export_hwpx().unwrap()).expect("HWPX 다시 열기");
        assert_beside(&hwpx, &format!("{label}→HWPX"));
    }
}

/// 어울림 그림을 붙이면 그 옆에 놓이는 뒤 문단의 줄도 그림 옆으로 접는다.
#[test]
fn pasted_square_picture_carves_following_paragraph_rows() {
    let mut doc = HwpDocument::from_bytes(RHWP_HWPX).expect("open");
    doc.copy_control(0, 0, "[]", 2).expect("그림 복사");
    doc.delete_picture_control(0, 0, 2).expect("그림 지우기");
    let pasted: Value = serde_json::from_str(&doc.paste_control(0, 0, 0).unwrap()).unwrap();
    let host = pasted["paraIdx"].as_u64().unwrap() as usize;
    let paras = &doc.document().sections[0].paragraphs;
    let Control::Picture(picture) = &paras[host].controls[0] else {
        panic!("붙인 그림");
    };
    let right = picture.common.horizontal_offset as i32 + picture.common.width as i32;
    let body = paras
        .iter()
        .position(|p| p.text.contains("본문"))
        .expect("본문 문단");
    assert!(body > host, "전제: 본문 문단은 붙인 그림 문단 뒤다");
    assert_eq!(
        first_row(&paras[body]),
        Some((right, 42520 - right)),
        "그림 옆 뒤 문단은 그림 오른쪽 끝에서 시작하는 조각이다"
    );
}
