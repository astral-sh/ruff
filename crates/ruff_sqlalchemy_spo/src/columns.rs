//! Column-walker: `col = db.Column(db.TYPE(...), nullable=, primary_key=,
//! default=, db.ForeignKey('table.col'))` → a [`RawColumn`].
//!
//! A standalone module (`SoC` mandate — Ruling a): the relationship-walker
//! ([`crate::relationships`]) and the type-mapper ([`crate::types`]) are
//! separate concerns, not folded into one God-`walk.rs`.

use ruff_python_ast::{Arguments, Expr};

use crate::types::field_type_of;

/// One `db.Column(...)` declaration, harvested but not yet deduped against
/// any sibling `db.relationship` — that dedup is OGAR's
/// `project_sqlalchemy_fields` (SPEC-5 Part B), not this frontend's job. The
/// physical column and the declared association are BOTH emitted here,
/// mirroring the Rails design (`ruff_ruby_spo`).
pub(crate) struct RawColumn {
    /// Column / attribute name.
    pub name: String,
    /// The mapped `field_type` token (`"string"`, `"integer"`, …), or `None`
    /// for a constructor the closed type-mapper table doesn't recognise.
    pub field_type: Option<String>,
    /// `Some(true)` when the column is NOT NULL (`nullable=False`, or an
    /// implicit primary key); `None` when nullable (`nullable=True` or
    /// absent — SQLAlchemy's default, same absence-means-nullable
    /// convention as the Rails schema stratum).
    pub not_null: Option<bool>,
    /// The raw `'table.col'` string from a `db.ForeignKey(...)` argument, if
    /// this column declares one.
    pub foreign_key: Option<String>,
}

/// Build a [`RawColumn`] from `name = db.Column(...)`. Returns `None` for a
/// non-`db.Column` assignment (methods, `__tablename__`, plain constants,
/// `db.relationship(...)` — handled separately by
/// [`crate::relationships::relationship_from_call`]).
#[must_use]
pub(crate) fn column_from_call(name: &str, value: &Expr) -> Option<RawColumn> {
    let Expr::Call(call) = value else {
        return None;
    };
    if !is_db_attr(&call.func, "Column") {
        return None;
    }

    let field_type = call
        .arguments
        .args
        .first()
        .and_then(field_type_of)
        .map(str::to_string);
    let foreign_key = foreign_key_arg(&call.arguments);
    let primary_key = bool_keyword(&call.arguments, "primary_key").unwrap_or(false);
    let nullable = bool_keyword(&call.arguments, "nullable");
    // PKs are not-null regardless of an explicit `nullable=`; otherwise
    // `nullable=False` is the only positive fact (absence, or an explicit
    // `nullable=True`, means nullable — the SQLAlchemy default).
    let not_null = if primary_key || nullable == Some(false) {
        Some(true)
    } else {
        None
    };

    Some(RawColumn {
        name: name.to_string(),
        field_type,
        not_null,
        foreign_key,
    })
}

/// Scan a `db.Column(...)` call's positional arguments for a nested
/// `db.ForeignKey('table.col', ...)` and return its raw `'table.col'` string.
fn foreign_key_arg(args: &Arguments) -> Option<String> {
    args.args.iter().find_map(|arg| {
        let Expr::Call(call) = arg else {
            return None;
        };
        if !is_db_attr(&call.func, "ForeignKey") {
            return None;
        }
        call.arguments.args.first().and_then(crate::expr_str)
    })
}

/// The boolean value of a keyword argument (`nullable=False` →
/// `Some(false)`), or `None` if absent or not a literal bool.
fn bool_keyword(args: &Arguments, keyword: &str) -> Option<bool> {
    match args.find_keyword(keyword).map(|kw| &kw.value) {
        Some(Expr::BooleanLiteral(b)) => Some(b.value),
        _ => None,
    }
}

/// `true` if `expr` is `db.<attr>` (an `Expr::Attribute` whose base is the
/// `db` name — the Flask-SQLAlchemy convention this frontend targets).
pub(crate) fn is_db_attr(expr: &Expr, attr: &str) -> bool {
    let Expr::Attribute(a) = expr else {
        return false;
    };
    crate::name_id(&a.value) == Some("db") && a.attr.id.as_str() == attr
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_expression;

    fn column_call(src: &str) -> Expr {
        parse_expression(src).expect("parses").expr().clone()
    }

    #[test]
    fn plain_typed_column_with_nullable_false() {
        let col = column_from_call(
            "beschreibung",
            &column_call("db.Column(db.String(500), nullable=False)"),
        )
        .expect("is a Column");
        assert_eq!(col.field_type.as_deref(), Some("string"));
        assert_eq!(col.not_null, Some(true));
        assert_eq!(col.foreign_key, None);
    }

    #[test]
    fn nullable_column_defaults_to_no_positive_fact() {
        let col = column_from_call(
            "created_at",
            &column_call("db.Column(db.DateTime, default=datetime.now)"),
        )
        .expect("is a Column");
        assert_eq!(col.field_type.as_deref(), Some("datetime"));
        assert_eq!(col.not_null, None);
    }

    #[test]
    fn primary_key_is_implicitly_not_null() {
        let col = column_from_call(
            "id",
            &column_call("db.Column(db.Integer, primary_key=True)"),
        )
        .expect("is a Column");
        assert_eq!(col.field_type.as_deref(), Some("integer"));
        assert_eq!(col.not_null, Some(true));
    }

    #[test]
    fn foreign_key_column_is_captured_as_a_typed_field_plus_fk_string() {
        let col = column_from_call(
            "timesheet_id",
            &column_call(
                "db.Column(db.Integer, db.ForeignKey('timesheets.id', ondelete='CASCADE'), nullable=False)",
            ),
        )
        .expect("is a Column");
        assert_eq!(col.field_type.as_deref(), Some("integer"));
        assert_eq!(col.not_null, Some(true));
        assert_eq!(col.foreign_key.as_deref(), Some("timesheets.id"));
    }

    #[test]
    fn non_column_assignment_is_not_a_column() {
        assert!(
            column_from_call("timesheet", &column_call("db.relationship('TimeSheet')")).is_none()
        );
    }
}
