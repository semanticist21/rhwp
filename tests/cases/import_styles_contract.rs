//! 관계형. 한컴 12.30 스타일 가져오기·내보내기는 다른 문서에서 고른 스타일을 그 글자·문단 모양째 옮긴다
//! (도움말 `format/style/style(open).htm`, `style(new_style).htm`). 기준: 가져온 스타일이 원본과 같은 언어별
//! 글꼴 이름·테두리/배경·탭·문단 번호/글머리표 정의를 가리키고, 다음 스타일은 이름으로 이어지며, 같은 이름을
//! 덮어 쓰면 그 스타일을 쓴 문단이 새 모양을 따른다. HWP·HWPX 저장 뒤에도 같다.
#![cfg(not(target_arch = "wasm32"))]
use rhwp::model::document::Document;
use rhwp::model::style::{Bullet, HeadType, TabItem};
use rhwp::wasm_api::HwpDocument;

const HANJA_FONT: &str = "가져온한자체";
const BORDER_COLOR: u32 = 0x0012_3456;
const TAB_AT: u32 = 12_345;

fn blank() -> HwpDocument {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    doc
}

fn style_named(doc: &Document, name: &str) -> Option<usize> {
    doc.doc_info
        .styles
        .iter()
        .position(|s| s.local_name == name)
}

fn reopened(doc: &HwpDocument) -> Vec<(&'static str, HwpDocument)> {
    [
        ("HWP", doc.export_hwp().unwrap()),
        ("HWPX", doc.export_hwpx().unwrap()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, HwpDocument::from_bytes(&bytes).unwrap()))
    .collect()
}

/// 원본: 한자 글꼴·테두리·탭·번호가 모두 새 정의인 "가져올 제목"(다음: 가져올 본문),
/// 글머리표를 쓰는 "가져올 본문"(다음: 본문), 그리고 굵고 가운데 정렬로 바꾼 "본문".
fn source() -> HwpDocument {
    let mut doc = blank();
    let title =
        doc.create_style(r#"{"name":"가져올 제목","englishName":"Imported Title","type":0}"#);
    let item = doc.create_style(r#"{"name":"가져올 본문","type":0}"#);
    assert!(title > 0 && item > 0);
    let body = style_named(doc.document(), "본문").unwrap();
    let d = doc.document_mut();
    let info = &mut d.doc_info;

    let mut font = info.font_faces[2][0].clone();
    font.name = HANJA_FONT.to_string();
    font.raw_data = None;
    info.font_faces[2].push(font);
    let hanja = (info.font_faces[2].len() - 1) as u16;

    let mut border = info.border_fills[0].clone();
    border.raw_data = None;
    for line in &mut border.borders {
        line.color = BORDER_COLOR;
    }
    info.border_fills.push(border);
    let border_ref = info.border_fills.len() as u16;

    let mut tabs = info.tab_defs[0].clone();
    tabs.raw_data = None;
    tabs.tabs = vec![TabItem {
        position: TAB_AT,
        tab_type: 1,
        fill_type: 0,
    }];
    info.tab_defs.push(tabs);
    let tab_def = (info.tab_defs.len() - 1) as u16;

    let mut numbering = info.numberings.first().cloned().unwrap_or_default();
    numbering.raw_data = None;
    numbering.start_number = 7;
    info.numberings.push(numbering);
    let numbering_ref = info.numberings.len() as u16;

    info.bullets.push(Bullet {
        bullet_char: '◆',
        char_shape_id: u32::MAX,
        ..Bullet::default()
    });
    info.bullet_count = info.bullets.len() as u32;
    let bullet_ref = info.bullets.len() as u16;

    let base_cs = info.styles[0].char_shape_id as usize;
    let base_ps = info.styles[0].para_shape_id as usize;
    let mut cs = info.char_shapes[base_cs].clone();
    cs.raw_data = None;
    cs.font_ids[2] = hanja;
    cs.border_fill_id = border_ref;
    info.char_shapes.push(cs);
    let title_cs = (info.char_shapes.len() - 1) as u16;

    let mut ps = info.para_shapes[base_ps].clone();
    ps.raw_data = None;
    ps.tab_def_id = tab_def;
    ps.head_type = HeadType::Number;
    ps.numbering_id = numbering_ref;
    ps.border_fill_id = border_ref;
    info.para_shapes.push(ps.clone());
    let title_ps = (info.para_shapes.len() - 1) as u16;
    ps.head_type = HeadType::Bullet;
    ps.numbering_id = bullet_ref;
    info.para_shapes.push(ps);
    let item_ps = (info.para_shapes.len() - 1) as u16;

    let mut bold = info.char_shapes[base_cs].clone();
    bold.raw_data = None;
    bold.bold = true;
    bold.base_size = 2000;
    info.char_shapes.push(bold);
    let bold_cs = (info.char_shapes.len() - 1) as u16;

    let styles = &mut info.styles;
    let (title, item) = (title as usize, item as usize);
    styles[title].char_shape_id = title_cs;
    styles[title].para_shape_id = title_ps;
    styles[title].next_style_id = item as u8;
    styles[item].para_shape_id = item_ps;
    styles[item].next_style_id = body as u8;
    styles[body].char_shape_id = bold_cs;
    for style in styles.iter_mut() {
        style.raw_data = None;
    }
    info.raw_stream_dirty = true;
    doc
}

/// 439: 고른 스타일이 그 모양 정의를 모두 데리고 오고 다음 스타일이 이름으로 이어진다.
#[test]
fn imported_styles_carry_their_shapes_and_next_style() {
    let src = source();
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "가나다").unwrap();
    let body = style_named(doc.document(), "본문").unwrap();
    doc.apply_style_native(0, 0, body).unwrap();
    let count = doc.document().doc_info.styles.len();
    let ids = doc
        .import_styles_native(
            src.document(),
            &["가져올 제목".into(), "가져올 본문".into()],
            false,
        )
        .unwrap();
    // 한컴 12.30처럼 바탕글 바로 뒤에 고른 차례로 넣고, 뒤 스타일을 쓰던 문단은 그 스타일을 따라간다.
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(doc.document().doc_info.styles.len(), count + 2);
    let para_style = doc.document().sections[0].paragraphs[0].style_id as usize;
    assert_eq!(
        doc.document().doc_info.styles[para_style].local_name,
        "본문"
    );

    let check = |doc: &HwpDocument, label: &str| {
        let d = doc.document();
        let info = &d.doc_info;
        let title = style_named(d, "가져올 제목").expect(label);
        let item = style_named(d, "가져올 본문").expect(label);
        let body = style_named(d, "본문").expect(label);
        let ts = &info.styles[title];
        assert_eq!(ts.english_name, "Imported Title", "{label}");
        assert_eq!(
            usize::from(ts.next_style_id),
            item,
            "{label}: 제목 → 본문 항목"
        );
        assert_eq!(
            usize::from(info.styles[item].next_style_id),
            body,
            "{label}: 항목 → 본문"
        );

        let cs = &info.char_shapes[ts.char_shape_id as usize];
        assert_eq!(
            info.font_faces[2][cs.font_ids[2] as usize].name, HANJA_FONT,
            "{label}: 한자 글꼴"
        );
        let base = &info.char_shapes[info.styles[0].char_shape_id as usize];
        assert_eq!(
            info.font_faces[0][cs.font_ids[0] as usize].name,
            info.font_faces[0][base.font_ids[0] as usize].name,
            "{label}: 한글 글꼴은 이름이 같은 기존 글꼴"
        );
        let char_border = &info.border_fills[cs.border_fill_id as usize - 1];
        assert!(
            char_border.borders.iter().all(|l| l.color == BORDER_COLOR),
            "{label}: 글자 테두리"
        );

        let ps = &info.para_shapes[ts.para_shape_id as usize];
        assert_eq!(
            info.tab_defs[ps.tab_def_id as usize].tabs[0].position, TAB_AT,
            "{label}: 탭"
        );
        assert_eq!(ps.head_type, HeadType::Number, "{label}");
        assert_eq!(
            info.numberings[ps.numbering_id as usize - 1].start_number,
            7,
            "{label}: 번호"
        );
        let para_border = &info.border_fills[ps.border_fill_id as usize - 1];
        assert!(
            para_border.borders.iter().all(|l| l.color == BORDER_COLOR),
            "{label}: 문단 테두리"
        );

        let ips = &info.para_shapes[info.styles[item].para_shape_id as usize];
        assert_eq!(ips.head_type, HeadType::Bullet, "{label}");
        assert_eq!(
            info.bullets[ips.numbering_id as usize - 1].bullet_char,
            '◆',
            "{label}: 글머리표"
        );
    };
    check(&doc, "가져온 직후");
    for (name, reopened) in reopened(&doc) {
        check(&reopened, name);
    }

    // 다음 스타일이 이 문서에 없으면 새 스타일은 자기 자신을 다음 스타일로 둔다.
    let mut alone = blank();
    let [id] = alone
        .import_styles_native(src.document(), &["가져올 제목".into()], false)
        .unwrap()[..]
    else {
        panic!("하나만 가져온다");
    };
    assert_eq!(id, 1);
    assert_eq!(alone.document().doc_info.styles[1].next_style_id, 1);

    assert!(doc
        .import_styles_native(src.document(), &["없는 스타일".into()], false)
        .is_err());
}

/// 439: 같은 이름은 덮어 쓸 때만 바꾸고, 덮어 쓰면 그 스타일을 쓴 문단이 새 모양을 따른다.
#[test]
fn overwriting_a_style_updates_its_paragraphs() {
    let src = source();
    let mut doc = blank();
    doc.insert_text_native(0, 0, 0, "가나다").unwrap();
    let body = style_named(doc.document(), "본문").unwrap();
    doc.apply_style_native(0, 0, body).unwrap();
    let bold_at = |doc: &HwpDocument| {
        let d = doc.document();
        let para = &d.sections[0].paragraphs[0];
        let cs = &d.doc_info.char_shapes[para.char_shapes[0].char_shape_id as usize];
        (cs.bold, cs.base_size)
    };
    assert!(!bold_at(&doc).0);
    let count = doc.document().doc_info.styles.len();

    let skipped = doc
        .import_styles_native(src.document(), &["본문".into()], false)
        .unwrap();
    assert_eq!(skipped, vec![-1]);
    assert!(!bold_at(&doc).0, "덮어 쓰지 않으면 그대로");

    let ids = doc
        .import_styles_native(src.document(), &["본문".into()], true)
        .unwrap();
    assert_eq!(ids, vec![body as i32]);
    assert_eq!(
        doc.document().doc_info.styles.len(),
        count,
        "새 스타일을 더하지 않는다"
    );
    assert_eq!(bold_at(&doc), (true, 2000), "문단이 덮어 쓴 모양을 따른다");
    for (name, reopened) in reopened(&doc) {
        assert_eq!(bold_at(&reopened), (true, 2000), "{name}");
        assert_eq!(
            reopened.document().sections[0].paragraphs[0].style_id as usize,
            body,
            "{name}"
        );
    }
}
