//! Phase A: tests for config loading, validation, and error messages.

use ruff_python_dto_check::config::{Config, ConfigError, MatchKind};

const FLASK_CONFIG: &str = include_str!("../examples/flask.config.json");

#[test]
fn parse_flask_config_succeeds() {
    let cfg = Config::from_json_str(FLASK_CONFIG).expect("flask config parses");
    assert_eq!(cfg.match_rules.len(), 1);
    let rule = &cfg.match_rules[0];
    assert_eq!(rule.id, "flask_route");
    assert_eq!(rule.kind, MatchKind::FunctionWithDecorator);
    let dec = rule.decorator.as_ref().expect("has decorator");
    assert_eq!(dec.attribute.as_deref(), Some("route"));
    assert_eq!(dec.min_positional_args, Some(1));
    assert!(rule.emit.contains_key("url"));
    assert!(rule.emit.contains_key("function_name"));
    let group = &cfg.group;
    let fff = group
        .family_from_filename
        .as_ref()
        .expect("has family_from_filename");
    assert!(fff.regex.contains("(?P<family>"));
}

#[test]
fn unknown_top_level_key_gives_did_you_mean() {
    let json = r#"{
        "mach": [{"id": "x", "kind": "function_with_decorator", "emit": {}}]
    }"#;
    let err = Config::from_json_str(json).expect_err("must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("did you mean"),
        "error should contain did_you_mean hint, got: {msg}"
    );
    assert!(
        msg.contains("match"),
        "hint should suggest 'match', got: {msg}"
    );
}

#[test]
fn missing_family_capture_rejected() {
    let json = r#"{
        "match": [{"id": "x", "kind": "function_with_decorator", "emit": {}}],
        "group": {"family_from_filename": {"regex": "^([a-z]+)\\.py$"}}
    }"#;
    let err = Config::from_json_str(json).expect_err("must fail");
    match err {
        ConfigError::Validation(msg) => {
            assert!(
                msg.contains("family"),
                "error should mention 'family' named capture, got: {msg}"
            );
        }
        other => panic!("expected Validation error, got: {other}"),
    }
}

#[test]
fn unknown_kind_rejected() {
    let json = r#"{
        "match": [{"id": "x", "kind": "class_with_base", "emit": {}}]
    }"#;
    let err = Config::from_json_str(json).expect_err("must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("class_with_base"),
        "error should mention unknown kind, got: {msg}"
    );
}

#[test]
fn empty_match_array_rejected() {
    let json = r#"{"match": []}"#;
    let err = Config::from_json_str(json).expect_err("must fail");
    let msg = err.to_string();
    assert!(msg.contains("at least one"), "got: {msg}");
}

#[test]
fn missing_match_key_rejected() {
    // "match" is required per schema; our validator requires at least one rule.
    // An empty object should produce a parse error (no "match" key → serde gives default empty vec).
    let json = r#"{"root": "."}"#;
    // serde gives empty vec for match, so our validator catches it.
    let err = Config::from_json_str(json).expect_err("must fail");
    let msg = err.to_string();
    assert!(msg.contains("at least one"), "got: {msg}");
}
