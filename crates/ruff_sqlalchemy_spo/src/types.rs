//! The SQLAlchemy DSL → [`ruff_spo_triplet::Field::field_type`] mapping
//! table (SPEC-5 Part A). A standalone module (`SoC` mandate — Ruling a) so a
//! future dialect frontend (Django, Sequel) can reuse the same table rather
//! than re-deriving it inside a walker.
//!
//! Only the type *name* is kept; constructor arguments (`String(500)`'s
//! length, `Numeric(10, 2)`'s precision/scale) are dropped — the mapping
//! table records structure (which SQL type), not the parameterisation.

use ruff_python_ast::Expr;

/// The column-type constructors this frontend recognises, mapped to the
/// `field_type` token it emits. Closed vocabulary (mirrors the discipline of
/// `ruff_spo_triplet::Predicate::ALL` / the Rails `COLUMN_TYPES` list in
/// `ruff_ruby_spo::schema`): an unrecognised `db.<Ctor>` is not silently
/// guessed at, it simply yields no `field_type` (the column is still
/// harvested as a [`ruff_spo_triplet::Field`], just untyped).
const COLUMN_TYPES: &[(&str, &str)] = &[
    ("String", "string"),
    ("Text", "text"),
    ("Integer", "integer"),
    ("BigInteger", "bigint"),
    ("Boolean", "boolean"),
    ("DateTime", "datetime"),
    ("Date", "date"),
    ("Numeric", "decimal"),
    ("Float", "float"),
];

/// Resolve a `db.Column(...)`'s type argument to the `field_type` token.
///
/// Handles both SQLAlchemy forms: a bare constructor reference (`db.Integer`,
/// an `Expr::Attribute`) and a parameterised call (`db.String(500)`,
/// `db.Numeric(10, 2)`, an `Expr::Call` whose callee is the same attribute
/// access) — the constructor arguments are read only to recognise the call
/// shape, never to keep the length/precision (dropped per the mapping table).
#[must_use]
pub(crate) fn field_type_of(type_expr: &Expr) -> Option<&'static str> {
    let ctor_name = match type_expr {
        Expr::Attribute(attr) => Some(attr.attr.id.as_str()),
        Expr::Call(call) => match &*call.func {
            Expr::Attribute(attr) => Some(attr.attr.id.as_str()),
            Expr::Name(name) => Some(name.id.as_str()),
            _ => None,
        },
        Expr::Name(name) => Some(name.id.as_str()),
        _ => None,
    }?;
    COLUMN_TYPES
        .iter()
        .find(|(ctor, _)| *ctor == ctor_name)
        .map(|(_, field_type)| *field_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_expression;

    fn type_expr_of(src: &str) -> Expr {
        // Parse `db.Column(<src>)`'s sole argument back out — reuses the
        // real parser rather than hand-building AST nodes.
        let wrapped = format!("f({src})");
        let parsed = parse_expression(&wrapped).expect("parses");
        let Expr::Call(call) = parsed.expr() else {
            panic!("expected a call expression");
        };
        call.arguments.args[0].clone()
    }

    #[test]
    fn bare_constructor_maps_by_name() {
        assert_eq!(field_type_of(&type_expr_of("db.Integer")), Some("integer"));
        assert_eq!(field_type_of(&type_expr_of("db.Boolean")), Some("boolean"));
    }

    #[test]
    fn parameterised_constructor_drops_args_but_keeps_the_type() {
        assert_eq!(
            field_type_of(&type_expr_of("db.String(500)")),
            Some("string")
        );
        assert_eq!(
            field_type_of(&type_expr_of("db.Numeric(10, 2)")),
            Some("decimal")
        );
    }

    #[test]
    fn unrecognised_constructor_yields_none() {
        assert_eq!(field_type_of(&type_expr_of("db.LargeBinary")), None);
    }
}
