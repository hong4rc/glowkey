//! UniKey's "bỏ dấu" tool: strip Vietnamese diacritics down to plain ASCII.

use glowkey_engine::remove_tones;

#[test]
fn strips_every_vowel_family_and_d_bar() {
    assert_eq!(remove_tones("Tiếng Việt"), "Tieng Viet");
    assert_eq!(remove_tones("đường"), "duong");
    assert_eq!(remove_tones("Nguyễn Huệ"), "Nguyen Hue");
    assert_eq!(remove_tones("cửa hàng ăn uống"), "cua hang an uong");
    assert_eq!(remove_tones("ỷ lại"), "y lai");
}

#[test]
fn preserves_case() {
    assert_eq!(remove_tones("ĐƯỜNG"), "DUONG");
    assert_eq!(remove_tones("Đường"), "Duong");
    assert_eq!(remove_tones("HÀ NỘI"), "HA NOI");
}

#[test]
fn leaves_everything_else_alone() {
    assert_eq!(remove_tones("hello world 123!"), "hello world 123!");
    assert_eq!(remove_tones(""), "");
    // A borrowed word is stripped too: é is an ordinary Vietnamese tone form,
    // and nothing here knows the word is French. UniKey's tool behaves the same.
    assert_eq!(remove_tones("café"), "cafe");
    // An accent Vietnamese never uses is left alone.
    assert_eq!(remove_tones("Zürich"), "Zürich");
}

#[test]
fn covers_all_five_tones_on_one_vowel() {
    assert_eq!(remove_tones("à á ả ã ạ"), "a a a a a");
    assert_eq!(remove_tones("ừ ứ ử ữ ự"), "u u u u u");
}
