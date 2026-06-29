//! Class-body walk: turn a `StmtClassDef` into a [`RawClass`].
//!
//! Mirrors `ruff_ruby_spo`'s manual class-body dispatch — no visitor trait at
//! this level, just a match over the body statements. A class is treated as an
//! Odoo model when it either subclasses `models.{Model,AbstractModel,
//! TransientModel}` or declares `_name` / `_inherit`.

use ruff_python_ast::{Arguments, Expr, Stmt, StmtAssign, StmtClassDef};

use crate::functions::analyze_method;
use crate::{RawClass, RawField, expr_str, name_id};

/// Walk a class definition into a [`RawClass`], or `None` if it isn't a model.
pub(crate) fn walk_class(class: &StmtClassDef) -> Option<RawClass> {
    let mut name = None;
    let mut inherits = Vec::new();
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut is_model = is_model_base(class);

    for stmt in &class.body {
        match stmt {
            Stmt::Assign(assign) => {
                if let Some(target) = single_name_target(assign) {
                    match target {
                        "_name" => {
                            if let Some(value) = expr_str(&assign.value) {
                                name = Some(value);
                                is_model = true;
                            }
                        }
                        "_inherit" => {
                            inherits.extend(string_or_list(&assign.value));
                            is_model = true;
                        }
                        _ => {
                            if let Some(field) = field_from_assign(target, &assign.value) {
                                fields.push(field);
                            }
                        }
                    }
                }
            }
            Stmt::FunctionDef(func) => methods.push(analyze_method(func)),
            _ => {}
        }
    }

    is_model.then_some(RawClass {
        name,
        inherits,
        fields,
        methods,
    })
}

/// `True` if the class subclasses one of the Odoo model base classes.
fn is_model_base(class: &StmtClassDef) -> bool {
    class
        .arguments
        .as_deref()
        .is_some_and(|args| args.args.iter().any(is_models_base))
}

/// `True` for a base expression of the form `models.Model` /
/// `models.AbstractModel` / `models.TransientModel` / `models.BaseModel`.
fn is_models_base(expr: &Expr) -> bool {
    let Expr::Attribute(attr) = expr else {
        return false;
    };
    name_id(&attr.value) == Some("models")
        && matches!(
            attr.attr.id.as_str(),
            "Model" | "AbstractModel" | "TransientModel" | "BaseModel"
        )
}

/// The single LHS identifier of `x = ...`, or `None` for tuple/multiple targets.
fn single_name_target(assign: &StmtAssign) -> Option<&str> {
    match assign.targets.as_slice() {
        [target] => name_id(target),
        _ => None,
    }
}

/// `_inherit = 'a.b'` (string) or `_inherit = ['a.b', 'c.d']` (list) → a list.
fn string_or_list(value: &Expr) -> Vec<String> {
    match value {
        Expr::StringLiteral(s) => vec![s.value.to_str().to_string()],
        Expr::List(list) => list.elts.iter().filter_map(expr_str).collect(),
        _ => Vec::new(),
    }
}

/// Build a [`RawField`] from `name = fields.K(...)`, capturing `compute=`.
/// Returns `None` for non-`fields.*` assignments (`_order`, `models.Constraint`,
/// plain constants, …).
fn field_from_assign(name: &str, value: &Expr) -> Option<RawField> {
    let Expr::Call(call) = value else {
        return None;
    };
    let Expr::Attribute(func) = &*call.func else {
        return None;
    };
    if name_id(&func.value) != Some("fields") {
        return None;
    }
    let (target, inverse_name, relation_kind) =
        relation_target_inverse(func.attr.id.as_str(), &call.arguments);
    Some(RawField {
        name: name.to_string(),
        compute: call
            .arguments
            .find_keyword("compute")
            .and_then(|kw| expr_str(&kw.value)),
        target,
        inverse_name,
        relation_kind,
    })
}

/// Resolve a relational field's comodel (`target`), its cardinality
/// (`relation_kind`, lowercased — `many2one` / `one2many` / `many2many`),
/// and, for One2many, its inverse field name. Handles both Odoo forms: the
/// comodel as a leading positional string or a `comodel_name=` kwarg; the
/// One2many inverse as the second positional or an `inverse_name=` kwarg.
/// Non-relational kinds yield `(None, None, None)`; `Many2many`, whose
/// inverse is a join table rather than a field, yields no inverse but does
/// carry its kind (the only signal that separates it from a `Many2one`).
fn relation_target_inverse(
    kind: &str,
    args: &Arguments,
) -> (Option<String>, Option<String>, Option<String>) {
    let comodel = || {
        args.find_keyword("comodel_name")
            .and_then(|kw| expr_str(&kw.value))
            .or_else(|| args.find_positional(0).and_then(expr_str))
    };
    match kind {
        "Many2one" | "Many2many" => (comodel(), None, Some(kind.to_lowercase())),
        "One2many" => {
            let inverse = args
                .find_keyword("inverse_name")
                .and_then(|kw| expr_str(&kw.value))
                .or_else(|| args.find_positional(1).and_then(expr_str));
            (comodel(), inverse, Some(kind.to_lowercase()))
        }
        _ => (None, None, None),
    }
}
