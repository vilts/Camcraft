//! `braces::code` blanks each literal and comment, so a `mod` token inside one is not code (#3711).
//! Provenance: the Rust Reference's lexical structure chapter (literals, comments, lifetimes).

use super::{Lex, code};

/// The `mod` words of the code of `line` (one line, read from the code state).
fn mods(line: &str) -> Vec<String> {
    let mut lex = Lex::default();
    let text = code(line, &mut lex);
    assert_eq!(lex, Lex::Code, "{line:?} must end in code");
    text.split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0] == "mod")
        .map(|w| w[1].to_string())
        .collect()
}

#[test]
fn a_mod_token_in_a_literal_or_comment_is_blanked_and_the_real_one_stays() {
    for line in [
        r#"const S: &str = "mod y"; mod x;"#,
        r#"const S: &str = "a \" mod y"; mod x;"#,
        r#"const S: &str = r"mod y \"; mod x;"#,
        r##"const S: &str = r#"a " mod y"#; mod x;"##,
        r#"const S: &[u8] = br"mod y \"; mod x;"#,
        r#"const S: &[u8] = b"mod y"; mod x;"#,
        r#"const S: &core::ffi::CStr = cr"mod y \"; mod x;"#,
        r##"const S: &core::ffi::CStr = cr#"a " mod y"#; mod x;"##,
        r#"const S: &core::ffi::CStr = c"mod y"; mod x;"#,
        "/* a /* mod y */ mod z */ mod x;",
        r#"const Q: char = '"'; const S: &str = "mod y"; mod x;"#,
        r#"const Q: char = '\''; const S: &str = "mod y"; mod x;"#,
        r#"const Q: char = '\\'; const S: &str = "mod y"; mod x;"#,
        r#"const Q: u8 = b'"'; const S: &str = "mod y"; mod x;"#,
        r#"fn f<'a>(s: &'a str) -> &'a str { s } const S: &str = "mod y"; mod x;"#,
        "mod x; // mod y;",
    ] {
        assert_eq!(mods(line), ["x;"], "{line}");
    }
}

#[test]
fn a_block_comment_and_a_string_carry_over_to_the_next_line() {
    let mut lex = Lex::default();
    assert!(!code("/* mod a;", &mut lex).contains("mod"));
    assert_eq!(lex, Lex::Block(1));
    assert!(code("mod b; */ mod c;", &mut lex).trim() == "mod c;");
    let mut lex = Lex::default();
    assert!(!code(r#"let s = "mod a;"#, &mut lex).contains("mod"));
    assert_eq!(lex, Lex::Str);
    assert_eq!(code(r#"mod b;" ; mod c;"#, &mut lex).trim(), "; mod c;");
}

#[test]
fn a_test_after_an_attributed_fn_on_one_line_is_found_and_the_scan_ends() {
    // #3711: the attribute walk took the fn line above the test, and `scan::test_fns` never ended.
    let text = "#[allow(dead_code)] pub fn helper() {}\n#[test]\nfn a1() {}\n";
    let tests = crate::scan::test_fns(text).expect("scan");
    let names: Vec<(&str, usize, usize)> = tests.iter().map(|t| (t.name.as_str(), t.start, t.end)).collect();
    assert_eq!(names, [("a1", 2, 3)]);
    // A scoped `pub` fn line with attributes is a fn, not an attribute of the test below it.
    for head in ["#[inline] pub(crate) fn h() {}", "#[inline] pub(in crate::x) const unsafe fn h() {}"] {
        let text = format!("{head}\n#[test]\nfn a1() {{}}\n");
        let tests = crate::scan::test_fns(&text).expect("scan");
        let names: Vec<(&str, usize)> = tests.iter().map(|t| (t.name.as_str(), t.start)).collect();
        assert_eq!(names, [("a1", 2)], "{head}");
    }
}
