use super::*;

#[test]
fn parse_pieces_literals_escapes_and_placeholders() {
    assert_eq!(parse_pieces(""), Ok(vec![]));
    assert_eq!(
        parse_pieces("hello {{world}}!"),
        Ok(vec![
            Piece::Literal("hello "),
            Piece::Literal("{"),
            Piece::Literal("world"),
            Piece::Literal("}"),
            Piece::Literal("!"),
        ])
    );
    assert_eq!(
        parse_pieces("User {} ({nick}) scored {0:04}!"),
        Ok(vec![
            Piece::Literal("User "),
            Piece::Placeholder { arg: "", spec: "" },
            Piece::Literal(" ("),
            Piece::Placeholder {
                arg: "nick",
                spec: "",
            },
            Piece::Literal(") scored "),
            Piece::Placeholder {
                arg: "0",
                spec: "04",
            },
            Piece::Literal("!"),
        ])
    );
}

#[test]
fn parse_pieces_rejects_unmatched_braces() {
    assert!(parse_pieces("unclosed {foo").is_err());
    assert!(parse_pieces("unmatched } here").is_err());
    assert!(parse_pieces("trailing }").is_err());
}

#[test]
fn dynamic_count_detection_ignores_fill_chars() {
    assert!(!has_dynamic_count(""));
    assert!(!has_dynamic_count("04"));
    assert!(!has_dynamic_count(">6.2"));
    assert!(!has_dynamic_count("$<4"));
    assert!(!has_dynamic_count("*>4"));
    assert!(!has_dynamic_count("*^10"));

    assert!(has_dynamic_count(">w$"));
    assert!(has_dynamic_count(">0$"));
    assert!(has_dynamic_count(".*"));
    assert!(has_dynamic_count(">6.*"));
}

#[test]
fn format_call_parses_positional_named_raw_idents_and_comparisons() {
    let call: FormatCall =
        syn::parse_str(r#""{0} {match} {}", count == 3, "ok", r#match = 42,"#).unwrap();
    assert_eq!(call.positional.len(), 2);
    assert_eq!(call.named.len(), 1);
    assert_eq!(call.named[0].name, "match");

    // Positional after named and duplicate named arguments are rejected.
    assert!(syn::parse_str::<FormatCall>(r#""{a} {}", a = 1, 2"#).is_err());
    assert!(syn::parse_str::<FormatCall>(r#""{a}", a = 1, r#a = 2"#).is_err());
}
