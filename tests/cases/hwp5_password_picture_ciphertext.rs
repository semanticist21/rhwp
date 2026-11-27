//! 관계형: 같은 비밀번호로 다시 저장한 HWP5 암호 문서의 그림 스트림과 저장 결과의 관계.
//! 주석 갱신: 2026-11-28T06:10:00+09:00. 시각 측정 대상이 아니다(저장 바이트·그림 보존만 본다).
//!
//! HWP5 EncryptVersion 4(영 IV 1비트 CFB)의 암호문은 같은 키·평문이면 늘 같다. 같은 비밀번호로
//! 연 문서를 다시 저장할 때 바뀌지 않은 그림은 원본 암호문을 그대로 옮기고, 다른 비밀번호로
//! 저장하면 새 키로 다시 암호화해야 한다.
#![cfg(not(target_arch = "wasm32"))]

use rhwp::document_core::DocumentCore;

const PICTURE: &[u8] = include_bytes!("../../assets/logo/logo-512.png");

fn picture_bytes(core: &DocumentCore) -> Vec<Vec<u8>> {
    core.document()
        .bin_data_content
        .iter()
        .map(|content| content.data.load())
        .collect()
}

#[test]
fn resaving_with_the_same_password_keeps_bytes_and_a_new_password_reencrypts_pictures() {
    let mut core = DocumentCore::new_empty();
    core.create_blank_document_native().unwrap();
    core.insert_picture_native(
        0,
        0,
        0,
        &[],
        PICTURE,
        9600,
        9600,
        512,
        512,
        "png",
        "그림",
        None,
        None,
    )
    .unwrap();
    let first = core
        .export_hwp_with_adapter_with_password(b"first")
        .unwrap();

    let opened = DocumentCore::from_bytes_with_password(&first, b"first").unwrap();
    assert_eq!(picture_bytes(&opened), [PICTURE.to_vec()]);
    // 같은 비밀번호: 그림 암호문을 그대로 옮겨도 다시 암호화한 결과와 같은 바이트다.
    let again = opened
        .export_hwp_with_adapter_with_password(b"first")
        .unwrap();
    assert_eq!(again, first);

    // 다른 비밀번호: 그림을 새 키로 암호화해 새 비밀번호로만 읽힌다.
    let renamed = opened
        .export_hwp_with_adapter_with_password(b"second")
        .unwrap();
    assert!(DocumentCore::from_bytes_with_password(&renamed, b"first").is_err());
    let reopened = DocumentCore::from_bytes_with_password(&renamed, b"second").unwrap();
    assert_eq!(picture_bytes(&reopened), [PICTURE.to_vec()]);
}
