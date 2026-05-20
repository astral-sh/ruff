//! Regression test for Codex review on PR #2 (P1 #1):
//! `run_harvest` was emitting bundles with `comparison_within_family = None`
//! because the observation pass was never invoked. This test runs the same
//! lib-level pipeline end-to-end (matcher then `attach_observations`) and
//! asserts that the comparison block is populated for every bundle.

use std::collections::BTreeMap;

use ruff_python_dto_check::bundle::EmittedBundle;
use ruff_python_dto_check::config::Config;
use ruff_python_dto_check::matcher::function_with_decorator::harvest_module_with_config;
use ruff_python_dto_check::observations::attach_observations;

const FLASK_CONFIG: &str = include_str!("../examples/flask.config.json");

const VIEWS_PY: &str = r#"
from flask import Blueprint
bp = Blueprint("views", __name__)

@bp.route("/orders")
def list_orders():
    return []

@bp.route("/orders/<int:order_id>")
def show_order(order_id):
    return {}

@bp.route("/orders", methods=["POST"])
def create_order():
    return {}
"#;

#[test]
fn attach_observations_populates_comparison_block() {
    let cfg = Config::from_json_str(FLASK_CONFIG).expect("parse config");
    let bundles = harvest_module_with_config("app/blueprints/orders.py", VIEWS_PY, &cfg);
    assert_eq!(bundles.len(), 3, "expect three route handlers");
    for b in &bundles {
        assert!(
            b.comparison_within_family.is_none(),
            "matcher should not pre-populate observations"
        );
    }

    let mut family_map: BTreeMap<String, Vec<EmittedBundle>> = BTreeMap::new();
    for b in bundles {
        family_map.entry(b.family.clone()).or_default().push(b);
    }
    let mut source_map: BTreeMap<String, String> = BTreeMap::new();
    source_map.insert("app/blueprints/orders.py".to_string(), VIEWS_PY.to_string());

    attach_observations(&mut family_map, &source_map);

    let family = family_map
        .values()
        .next()
        .expect("at least one family")
        .clone();
    assert_eq!(family.len(), 3);
    for b in &family {
        let cwf = b
            .comparison_within_family
            .as_ref()
            .expect("attach_observations should populate comparison_within_family");
        assert_eq!(cwf.family_size, 3);
        assert!(
            cwf.ast_hash_self.starts_with("sha256:"),
            "ast_hash_self should be the structural hash, got {:?}",
            cwf.ast_hash_self
        );
    }
}
