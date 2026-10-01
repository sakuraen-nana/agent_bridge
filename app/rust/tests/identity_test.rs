//! 短名规则边界（design D7 口径）。

use agent_bridge::identity::{SHORT_NAME_MAX_CHARS, short_name_compare_key, validate_short_name};

#[test]
fn trims_surrounding_whitespace() {
    assert_eq!(validate_short_name("  abc  ").unwrap(), "abc");
    assert_eq!(validate_short_name("\t中文名字\n").unwrap(), "中文名字");
}

#[test]
fn rejects_empty_or_whitespace_only() {
    assert!(validate_short_name("").is_err());
    assert!(validate_short_name("   ").is_err());
}

#[test]
fn length_boundary_is_32_chars() {
    let ok = "字".repeat(SHORT_NAME_MAX_CHARS);
    assert!(validate_short_name(&ok).is_ok(), "32 字符应通过");

    let too_long = "字".repeat(SHORT_NAME_MAX_CHARS + 1);
    assert!(validate_short_name(&too_long).is_err(), "33 字符应被拒");

    let ascii_ok = "a".repeat(SHORT_NAME_MAX_CHARS);
    assert!(validate_short_name(&ascii_ok).is_ok());
}

#[test]
fn rejects_inner_whitespace_and_control_chars() {
    assert!(validate_short_name("a b").is_err());
    assert!(validate_short_name("a\u{3000}b").is_err(), "全角空格也应被拒");
    assert!(validate_short_name("a\tb").is_err());
    assert!(validate_short_name("a\u{7}b").is_err(), "控制字符应被拒");
}

#[test]
fn compare_key_is_case_insensitive_and_trimmed() {
    assert_eq!(short_name_compare_key(" Dev "), short_name_compare_key("dev"));
    assert_eq!(short_name_compare_key("ΑΒΓ"), "αβγ", "Unicode 小写折叠");
    assert_ne!(short_name_compare_key("dev"), short_name_compare_key("dev2"));
}
