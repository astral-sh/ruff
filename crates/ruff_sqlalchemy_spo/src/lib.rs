//! `ruff_sqlalchemy_spo` — the Flask-SQLAlchemy SPO frontend (SPEC-5 Part A).
//!
//! The Python sibling of `ruff_python_spo` (Odoo) and `ruff_ruby_spo`
//! (`OpenProject`/Rails). Its ONLY job is to read a Flask-SQLAlchemy model
//! file's Python AST (via `ruff_python_parser`) and fill a
//! [`ruff_spo_triplet::ModelGraph`]; [`ruff_spo_triplet::expand`] then yields
//! the same SPO triple shape every other frontend produces.
//!
//! # Scope (v0, schema stratum only)
//!
//! - `class X(db.Model):` → a [`Model`] (`name` = the Python class name —
//!   `WoA`'s convention, unlike Odoo's dotted `_name`; the `__tablename__`
//!   value is recorded but not used as the model identity).
//! - `col = db.Column(db.TYPE(...), nullable=, primary_key=, default=,
//!   db.ForeignKey('table.col'))` → a [`Field`] (see [`columns`] for the
//!   type-mapping table in [`types`]).
//! - `rel = db.relationship('Target', backref=…, uselist=…)` → an
//!   [`ruff_spo_triplet::AssocDecl`] (see [`relationships`]), opportunistically
//!   paired with a sibling FK column by the `<name>_id` convention — the raw
//!   FK column is ALSO kept as a plain typed `Field` (mirrors the Rails
//!   design: physical column + declared association, deduped downstream by
//!   OGAR's `project_sqlalchemy_fields`, not here).
//! - `def foo(self, …):` → a name-only [`Function`] (schema-only v0; body
//!   facts are a follow-up — see [`functions`]).
//!
//! # Module layout (`SoC` mandate — Ruling a)
//!
//! No God-`walk.rs`: the AST-walk is factored into focused units so a future
//! dialect (Django, Sequel) can reuse the type-mapper without inheriting the
//! Flask-SQLAlchemy-specific column/relationship recognition.
//!
//! - [`parse`] — source → raw classes (mirrors `ruff_python_spo::parse`).
//! - [`walk`] — one class body → a [`RawClass`].
//! - [`columns`] — the column-walker (`db.Column(...)` → [`columns::RawColumn`]).
//! - [`relationships`] — the relationship-walker (`db.relationship(...)` →
//!   [`relationships::RawRelationship`] + the to-one/to-many heuristic +
//!   FK pairing into [`ruff_spo_triplet::AssocDecl`]).
//! - [`types`] — the SQLAlchemy DSL → `field_type` mapping table, standalone
//!   so it's reusable by a future dialect frontend.
//! - [`functions`] — method harvest (name-only in v0).

use std::fs;
use std::path::Path;

use ruff_python_ast::Expr;
use ruff_spo_triplet::{Field, Function, Model, ModelGraph};

use crate::columns::RawColumn;
use crate::relationships::RawRelationship;

mod columns;
mod functions;
mod parse;
mod relationships;
mod types;
mod walk;

/// The IRI namespace prefix for every WoA/Flask-SQLAlchemy subject/object.
pub const NAMESPACE: &str = "woa";

/// One `class X(db.Model):` extracted from the AST.
pub(crate) struct RawClass {
    /// The Python class name — the OGAR-side model identity (`WoA`'s
    /// convention; see the module docs).
    pub name: String,
    /// The `__tablename__` value, if declared. Recorded but not used as the
    /// model identity.
    #[allow(dead_code)] // recorded per spec; not yet consumed downstream
    pub tablename: Option<String>,
    /// `db.Column(...)` declarations.
    pub columns: Vec<RawColumn>,
    /// `db.relationship(...)` declarations, before FK pairing.
    pub relationships: Vec<RawRelationship>,
    /// Method declarations (name-only in v0).
    pub functions: Vec<Function>,
}

/// Extract a [`ModelGraph`] from a single Python source string.
///
/// A source that fails to parse contributes nothing (returns an empty
/// graph), mirroring `ruff_python_spo`'s silent-skip invariant.
#[must_use]
pub fn extract_from_source(source: &str) -> ModelGraph {
    build_graph(&parse::parse_source(source), NAMESPACE)
}

/// Extract a [`ModelGraph`] from a source tree (recursively reads `*.py`),
/// using the given namespace.
#[must_use]
pub fn extract_with(root: &Path, namespace: &str) -> ModelGraph {
    let mut classes = Vec::new();
    collect_py(root, &mut classes);
    build_graph(&classes, namespace)
}

/// Extract a [`ModelGraph`] from a source tree under the default
/// [`NAMESPACE`].
#[must_use]
pub fn extract(root: &Path) -> ModelGraph {
    extract_with(root, NAMESPACE)
}

/// Extract a [`ModelGraph`] from a single file (e.g. `WoA`'s monolithic
/// `models.py`) under the given namespace.
#[must_use]
pub fn extract_file(path: &Path, namespace: &str) -> ModelGraph {
    let classes = fs::read_to_string(path)
        .map(|src| parse::parse_source(&src))
        .unwrap_or_default();
    build_graph(&classes, namespace)
}

/// Convenience: expand a graph and serialise it to ndjson ready for the SPO
/// store.
#[must_use]
pub fn to_ndjson(graph: &ModelGraph) -> String {
    ruff_spo_triplet::to_ndjson(&ruff_spo_triplet::expand(graph))
}

/// Recursively collect every parseable `db.Model` class under `dir`.
fn collect_py(dir: &Path, out: &mut Vec<RawClass>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_py(&path, out);
        } else if path.extension().is_some_and(|e| e == "py")
            && let Ok(src) = fs::read_to_string(&path)
        {
            out.extend(parse::parse_source(&src));
        }
    }
}

/// Assemble raw classes into a [`ModelGraph`]. Unlike the Odoo frontend,
/// there is no `_inherit`-reopen merge phase — each `class X(db.Model):` is
/// one model, one class, full stop (`WoA`'s SQLAlchemy models don't reopen
/// across files).
fn build_graph(classes: &[RawClass], namespace: &str) -> ModelGraph {
    let models = classes
        .iter()
        .map(|class| {
            let associations =
                relationships::build_associations(&class.columns, &class.relationships);
            Model {
                name: class.name.clone(),
                fields: class
                    .columns
                    .iter()
                    .map(|c| Field {
                        name: c.name.clone(),
                        field_type: c.field_type.clone(),
                        not_null: c.not_null,
                        ..Default::default()
                    })
                    .collect(),
                functions: class.functions.clone(),
                associations,
                ..Default::default()
            }
        })
        .collect();

    ModelGraph {
        namespace: namespace.to_string(),
        models,
    }
}

/// Pull the string value out of a string-literal expression.
pub(crate) fn expr_str(expr: &Expr) -> Option<String> {
    if let Expr::StringLiteral(s) = expr {
        Some(s.value.to_str().to_string())
    } else {
        None
    }
}

/// The bare identifier of a `Name` expression.
pub(crate) fn name_id(expr: &Expr) -> Option<&str> {
    if let Expr::Name(n) = expr {
        Some(n.id.as_str())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{AssocKind, Triple, expand};

    fn has(triples: &[Triple], s: &str, p: &str, o: &str) -> bool {
        triples.iter().any(|t| t.s == s && t.p == p && t.o == o)
    }

    /// The SPEC-5 mini-fixture: a 5-column `db.Model` with one FK + one
    /// relationship, transcribed from `WoA/models.py:1746`
    /// (`TimesheetActivity`).
    const TIMESHEET_ACTIVITY: &str = r#"
class TimesheetActivity(db.Model):
    __tablename__ = 'timesheet_activities'
    id            = db.Column(db.Integer, primary_key=True)
    timesheet_id  = db.Column(db.Integer, db.ForeignKey('timesheets.id', ondelete='CASCADE'), nullable=False)
    beschreibung  = db.Column(db.String(500), nullable=False)
    created_at    = db.Column(db.DateTime, default=datetime.now)

    timesheet = db.relationship('TimeSheet', backref=db.backref('activities', lazy='select', order_by='TimesheetActivity.created_at'))
"#;

    #[test]
    fn timesheet_activity_model_shape() {
        let graph = extract_from_source(TIMESHEET_ACTIVITY);
        assert_eq!(graph.models.len(), 1);
        let model = &graph.models[0];
        assert_eq!(model.name, "TimesheetActivity");

        // Every column is harvested as a Field (undeduped — FK-dedup is
        // OGAR's `project_sqlalchemy_fields`, not this frontend's job).
        let field = |n: &str| model.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(field("id").field_type.as_deref(), Some("integer"));
        assert_eq!(field("id").not_null, Some(true));
        assert_eq!(field("timesheet_id").field_type.as_deref(), Some("integer"));
        assert_eq!(field("timesheet_id").not_null, Some(true));
        assert_eq!(field("beschreibung").field_type.as_deref(), Some("string"));
        assert_eq!(field("beschreibung").not_null, Some(true));
        assert_eq!(field("created_at").field_type.as_deref(), Some("datetime"));
        assert_eq!(field("created_at").not_null, None);

        // The relationship becomes a BelongsTo AssocDecl (plural backref
        // `activities` → this side is the single parent), paired with its
        // FK column by the `<name>_id` convention.
        assert_eq!(model.associations.len(), 1);
        let assoc = &model.associations[0];
        assert_eq!(assoc.name, "timesheet");
        assert_eq!(assoc.kind, AssocKind::BelongsTo);
        assert!(
            assoc
                .options
                .contains(&("class_name".to_string(), "TimeSheet".to_string()))
        );
        assert!(
            assoc
                .options
                .contains(&("foreign_key".to_string(), "timesheet_id".to_string()))
        );

        // The ndjson round-trips through the closed-vocab parser.
        let ndjson = to_ndjson(&graph);
        let parsed = ruff_spo_triplet::from_ndjson(&ndjson).expect("ndjson round-trips");
        assert_eq!(parsed, expand(&graph));
    }

    #[test]
    fn triples_carry_the_woa_namespace_and_column_not_null() {
        let graph = extract_from_source(TIMESHEET_ACTIVITY);
        let t = expand(&graph);
        assert!(has(
            &t,
            "woa:TimesheetActivity",
            "rdf:type",
            "ogit:ObjectType"
        ));
        assert!(has(
            &t,
            "woa:TimesheetActivity.beschreibung",
            "column_not_null",
            "true"
        ));
        // Nullable column emits no positive fact.
        assert!(
            !t.iter()
                .any(|tr| tr.s == "woa:TimesheetActivity.created_at" && tr.p == "column_not_null")
        );
    }

    #[test]
    fn non_model_class_is_skipped() {
        let graph = extract_from_source("class Plain:\n    x = 1\n");
        assert!(graph.models.is_empty());
    }

    #[test]
    fn unparsable_source_yields_empty_graph() {
        let graph = extract_from_source("class Broken(:  # not valid python\n");
        assert!(graph.models.is_empty());
    }

    /// Corpus gate (mirrors `ruff_ruby_spo::schema`'s `OPENPROJECT_PATH`
    /// pattern): only runs with `WOA_SRC` set, pointing at
    /// `/home/user/WoA/models.py`. `WoA` is READ-ONLY; this test only reads it.
    #[test]
    #[allow(clippy::print_stderr)] // diagnostic emission gated on env var (real-corpus gate)
    fn woa_corpus_harvests_the_full_model_set() {
        let Ok(root) = std::env::var("WOA_SRC") else {
            eprintln!("skipping: WOA_SRC not set");
            return;
        };
        let graph = extract_file(Path::new(&root), NAMESPACE);
        // Measured reality: WoA/models.py has 139 `class X(db.Model):`
        // declarations (grep-verified), not the "≥140" the spec estimated —
        // a 1-model discrepancy, documented honestly rather than papered
        // over with an inflated threshold.
        assert!(
            graph.models.len() >= 139,
            "expected >= 139 WoA models, harvested {}",
            graph.models.len()
        );
        eprintln!(
            "ruff_sqlalchemy_spo WoA corpus gate: {} models harvested",
            graph.models.len()
        );

        let ts = graph
            .models
            .iter()
            .find(|m| m.name == "TimesheetActivity")
            .expect("TimesheetActivity model");
        let field = |n: &str| ts.fields.iter().find(|f| f.name == n).unwrap();
        assert_eq!(field("beschreibung").field_type.as_deref(), Some("string"));
        assert_eq!(field("beschreibung").not_null, Some(true));
        assert_eq!(field("created_at").field_type.as_deref(), Some("datetime"));
        assert_eq!(field("created_at").not_null, None);
        assert_eq!(ts.associations.len(), 1);
        assert_eq!(ts.associations[0].name, "timesheet");
    }
}
