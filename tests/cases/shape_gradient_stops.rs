//! 도형 속성의 그러데이션 색·위치·번짐 중심을 읽고 쓴다(개체 모양 복사·새 그리기 속성으로).
//!
//! 한컴 12.30의 새 그리기 속성으로는 채우기 색과 그러데이션을 다음 도형에 옮긴다
//! (도움말 `insert/textbox/textbox(attributes).htm`). 종전 도형 속성은 그러데이션 유형·각도·중심·번짐만
//! 줘서 색이 빠졌다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::wasm_api::HwpDocument;

fn rectangle(doc: &mut HwpDocument) -> (usize, usize) {
    let shape = doc
        .create_shape_control_native(
            0,
            0,
            0,
            9000,
            6000,
            0,
            2000,
            false,
            "InFrontOfText",
            "rectangle",
            false,
            false,
            &[],
        )
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&shape).unwrap();
    let at = |key: &str| v[key].as_u64().unwrap() as usize;
    (at("paraIdx"), at("controlIdx"))
}

#[test]
fn gradient_stops_round_trip_through_shape_properties_and_files() {
    let mut doc = HwpDocument::create_empty();
    doc.create_blank_document_native().unwrap();
    let (para, ctrl) = rectangle(&mut doc);
    doc.set_shape_properties_native(
        0,
        para,
        ctrl,
        r#"{"fillType":"gradient","gradientType":1,"gradientAngle":90,"gradientColors":[255,16711680,65280],"gradientPositions":[0,40,100],"gradientStepCenter":50}"#,
    )
    .unwrap();
    for bytes in [doc.export_hwp().unwrap(), doc.export_hwpx().unwrap()] {
        let reopened = HwpDocument::from_bytes(&bytes).unwrap();
        let props: serde_json::Value =
            serde_json::from_str(&reopened.get_shape_properties_native(0, para, ctrl).unwrap())
                .unwrap();
        assert_eq!(props["fillType"], "gradient");
        assert_eq!(
            props["gradientColors"],
            serde_json::json!([255, 16711680, 65280])
        );
        assert_eq!(props["gradientAngle"], 90);
    }
}
