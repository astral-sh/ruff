//! Phase-0 golden test: parse the wo_list fixture, assert the identity
//! fields are extracted correctly. Extractor depth grows with later
//! phases — this test grows with it.

use ruff_python_dto_check::{bundle::DecoratorKind, harvest_module};

#[test]
fn wo_list_identity() {
    let source = include_str!("golden/wo_list.input.py");
    let harvest = harvest_module("woa/blueprints/vorgaenge_ops.py", source).expect("parses");
    assert_eq!(harvest.bundles.len(), 1, "expect exactly one route");
    let b = &harvest.bundles[0];
    assert_eq!(b.endpoint, "bp.wo_list");
    assert_eq!(b.path, "/vorgaenge");
    assert_eq!(b.methods, vec!["GET".to_string()]);
    assert_eq!(b.function, "wo_list");
    assert_eq!(b.action, "read");
    assert_eq!(b.source.file, "woa/blueprints/vorgaenge_ops.py");
    assert!(b.body.contains("def wo_list()"));
    assert!(b.body.starts_with("@bp.route("));
    assert_eq!(b.decorators.len(), 2);
    assert_eq!(b.decorators[0].kind, DecoratorKind::Route);
    assert_eq!(b.decorators[1].kind, DecoratorKind::Auth);
}
