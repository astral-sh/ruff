//! Phase-0 golden test: parse the Flask view fixture and assert the
//! identity fields are extracted correctly.

use ruff_python_dto_check::{bundle::DecoratorKind, harvest_module};

#[test]
fn flask_view_identity() {
    let source = include_str!("golden/flask_view.input.py");
    let harvest = harvest_module("app/blueprints/orders.py", source).expect("parses");
    assert_eq!(harvest.bundles.len(), 1, "expect exactly one route");
    let b = &harvest.bundles[0];
    assert_eq!(b.endpoint, "bp.order_list");
    assert_eq!(b.path, "/orders");
    assert_eq!(b.methods, vec!["GET".to_string()]);
    assert_eq!(b.function, "order_list");
    assert_eq!(b.action, "read");
    assert_eq!(b.source.file, "app/blueprints/orders.py");
    assert!(b.body.contains("def order_list()"));
    assert!(b.body.starts_with("@bp.route("));
    assert_eq!(b.decorators.len(), 2);
    assert_eq!(b.decorators[0].kind, DecoratorKind::Route);
    assert_eq!(b.decorators[1].kind, DecoratorKind::Auth);
}
