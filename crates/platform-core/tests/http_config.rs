use platform_core::parse_cors_allowed_origins;

#[test]
fn parses_comma_separated_origins() {
    assert_eq!(
        parse_cors_allowed_origins("http://localhost:5173,http://localhost:5174"),
        vec![
            "http://localhost:5173".to_owned(),
            "http://localhost:5174".to_owned(),
        ]
    );
}

#[test]
fn trims_whitespace_and_skips_empty_entries() {
    assert_eq!(
        parse_cors_allowed_origins(" http://a.test , ,http://b.test ,"),
        vec!["http://a.test".to_owned(), "http://b.test".to_owned()]
    );
}

#[test]
fn empty_value_yields_no_origins() {
    assert!(parse_cors_allowed_origins("").is_empty());
}
