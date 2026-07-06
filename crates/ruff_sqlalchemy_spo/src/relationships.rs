//! Relationship-walker: `rel = db.relationship('Target', backref=…,
//! uselist=…)` → a [`RawRelationship`], including the
//! to-one/to-many cardinality heuristic.
//!
//! A standalone module (`SoC` mandate — Ruling a), separate from the
//! column-walker ([`crate::columns`]) and the type-mapper ([`crate::types`]).

use ruff_python_ast::Expr;
use ruff_spo_triplet::{AssocDecl, AssocKind};

use crate::columns::{RawColumn, is_db_attr};
use crate::expr_str;

/// One `db.relationship(...)` declaration, before FK pairing (which
/// [`crate::walk`] performs once it has the sibling column list).
pub(crate) struct RawRelationship {
    /// The Python attribute name (`timesheet`).
    pub name: String,
    /// The related model's class name, as declared (`'TimeSheet'`).
    pub target: String,
    /// The association cardinality, per the backref/`uselist` heuristic
    /// (see [`classify`]).
    pub kind: AssocKind,
}

/// Build a [`RawRelationship`] from `name = db.relationship(...)`. Returns
/// `None` for a non-`db.relationship` assignment.
#[must_use]
pub(crate) fn relationship_from_call(name: &str, value: &Expr) -> Option<RawRelationship> {
    let Expr::Call(call) = value else {
        return None;
    };
    if !is_db_attr(&call.func, "relationship") {
        return None;
    }
    let target = call.arguments.args.first().and_then(expr_str)?;

    let uselist = call
        .arguments
        .find_keyword("uselist")
        .and_then(|kw| match &kw.value {
            Expr::BooleanLiteral(b) => Some(b.value),
            _ => None,
        });
    let backref_name = call
        .arguments
        .find_keyword("backref")
        .map(|kw| backref_name_of(&kw.value));

    Some(RawRelationship {
        name: name.to_string(),
        target,
        kind: classify(uselist, backref_name.flatten().as_deref()),
    })
}

/// The declared backref's name, from either SQLAlchemy form: a bare string
/// (`backref='activities'`) or `db.backref('activities', ...)` (whose first
/// positional argument is the name).
fn backref_name_of(expr: &Expr) -> Option<String> {
    match expr {
        Expr::StringLiteral(_) => expr_str(expr),
        Expr::Call(call) if is_db_attr(&call.func, "backref") => {
            call.arguments.args.first().and_then(expr_str)
        }
        _ => None,
    }
}

/// The to-one/to-many heuristic (SPEC-5 Part A, documented as best-effort):
///
/// - `uselist=False`, or a **plural** backref name (the reverse side
///   returns a collection, so this side is the single parent) → `BelongsTo`.
/// - `uselist=True`, or a **singular** backref name (the reverse side
///   returns one object, so this side is the collection) → `HasMany`.
/// - Neither signal present → `BelongsTo`, the more common
///   Flask-SQLAlchemy shape for an explicit `uselist`-less, backref-less
///   `relationship()` declared on the "many" side's FK-owning column.
///
/// Pluralisation is a simple trailing-`s` heuristic (not full English
/// inflection) — good enough for the corpus this frontend targets; a
/// genuinely irregular plural is a known, documented limitation.
#[must_use]
fn classify(uselist: Option<bool>, backref_name: Option<&str>) -> AssocKind {
    if uselist == Some(false) {
        return AssocKind::BelongsTo;
    }
    if uselist == Some(true) {
        return AssocKind::HasMany;
    }
    match backref_name.map(is_plural) {
        Some(true) => AssocKind::BelongsTo,
        Some(false) => AssocKind::HasMany,
        None => AssocKind::BelongsTo,
    }
}

/// Trailing-`s` plural heuristic (`"activities"` → plural via the `-y`→`-ies`
/// form; `"posts"` → plural; `"post"` → singular). Not full English
/// inflection — a best-effort heuristic, per the spec.
fn is_plural(word: &str) -> bool {
    word.ends_with("ies") || (word.ends_with('s') && !word.ends_with("ss"))
}

/// Build one [`AssocDecl`] per harvested relationship, opportunistically
/// pairing it with a sibling FK column (Rails-convention `<name>_id`) when
/// one exists — the FK column stays a plain typed [`ruff_spo_triplet::Field`]
/// **in addition to** the association (SPEC-5 Part A: "prefer to keep the
/// raw column ... and let OGAR's `project_sqlalchemy_fields` dedup"). A
/// `HasMany`-classified relationship (the FK lives on the *other* model, not
/// here) simply finds no local match and gets no `foreign_key` option.
#[must_use]
pub(crate) fn build_associations(
    columns: &[RawColumn],
    relationships: &[RawRelationship],
) -> Vec<AssocDecl> {
    relationships
        .iter()
        .map(|rel| {
            let fk_column_name = format!("{}_id", rel.name);
            let mut options = vec![("class_name".to_string(), rel.target.clone())];
            if let Some(col) = columns
                .iter()
                .find(|c| c.name == fk_column_name && c.foreign_key.is_some())
            {
                options.push(("foreign_key".to_string(), col.name.clone()));
            }
            AssocDecl {
                kind: rel.kind,
                name: rel.name.clone(),
                options,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_expression;

    fn rel_call(src: &str) -> Expr {
        parse_expression(src).expect("parses").expr().clone()
    }

    #[test]
    fn plural_backref_means_this_side_is_belongs_to() {
        let rel = relationship_from_call(
            "timesheet",
            &rel_call("db.relationship('TimeSheet', backref=db.backref('activities'))"),
        )
        .expect("is a relationship");
        assert_eq!(rel.target, "TimeSheet");
        assert_eq!(rel.kind, AssocKind::BelongsTo);
    }

    #[test]
    fn uselist_true_means_has_many() {
        let rel = relationship_from_call(
            "time_entries",
            &rel_call("db.relationship('TimeEntry', uselist=True)"),
        )
        .expect("is a relationship");
        assert_eq!(rel.kind, AssocKind::HasMany);
    }

    #[test]
    fn uselist_false_means_belongs_to() {
        let rel =
            relationship_from_call("user", &rel_call("db.relationship('User', uselist=False)"))
                .expect("is a relationship");
        assert_eq!(rel.kind, AssocKind::BelongsTo);
    }

    #[test]
    fn non_relationship_assignment_is_not_a_relationship() {
        assert!(relationship_from_call("id", &rel_call("db.Column(db.Integer)")).is_none());
    }

    #[test]
    fn build_associations_pairs_the_fk_column_by_convention() {
        let columns = vec![RawColumn {
            name: "timesheet_id".to_string(),
            field_type: Some("integer".to_string()),
            not_null: Some(true),
            foreign_key: Some("timesheets.id".to_string()),
        }];
        let relationships = vec![RawRelationship {
            name: "timesheet".to_string(),
            target: "TimeSheet".to_string(),
            kind: AssocKind::BelongsTo,
        }];
        let assocs = build_associations(&columns, &relationships);
        assert_eq!(assocs.len(), 1);
        assert_eq!(assocs[0].kind, AssocKind::BelongsTo);
        assert_eq!(assocs[0].name, "timesheet");
        assert!(
            assocs[0]
                .options
                .contains(&("class_name".to_string(), "TimeSheet".to_string()))
        );
        assert!(
            assocs[0]
                .options
                .contains(&("foreign_key".to_string(), "timesheet_id".to_string()))
        );
    }

    #[test]
    fn build_associations_has_many_side_gets_no_local_foreign_key() {
        // The FK lives on the OTHER model (TimeEntry.work_package_id), not
        // here — a HasMany relationship should not fabricate one.
        let relationships = vec![RawRelationship {
            name: "time_entries".to_string(),
            target: "TimeEntry".to_string(),
            kind: AssocKind::HasMany,
        }];
        let assocs = build_associations(&[], &relationships);
        assert_eq!(assocs.len(), 1);
        assert_eq!(assocs[0].kind, AssocKind::HasMany);
        assert!(!assocs[0].options.iter().any(|(k, _)| k == "foreign_key"));
    }
}
