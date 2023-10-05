use std::fmt;

use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, BoolOp, CmpOp, Constant, Expr, ExprBoolOp};
use ruff_python_parser::parse_expression;
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::settings::LinterSettings;

/// Returns the value of the `name` parameter to, e.g., a `TypeVar` constructor.
pub(super) fn type_param_name(arguments: &Arguments) -> Option<&str> {
    // Handle both `TypeVar("T")` and `TypeVar(name="T")`.
    let name_param = arguments.find_argument("name", 0)?;
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(name),
        ..
    }) = &name_param
    {
        Some(name)
    } else {
        None
    }
}

pub(super) fn in_dunder_init(semantic: &SemanticModel, settings: &LinterSettings) -> bool {
    let scope = semantic.current_scope();
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        decorator_list,
        ..
    }) = scope.kind
    else {
        return false;
    };
    if name != "__init__" {
        return false;
    }
    let Some(parent) = semantic.first_non_type_parent_scope(scope) else {
        return false;
    };

    if !matches!(
        function_type::classify(
            name,
            decorator_list,
            parent,
            semantic,
            &settings.pep8_naming.classmethod_decorators,
            &settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return false;
    }
    true
}

/// A wrapper around [`CmpOp`] that implements `Display`.
#[derive(Debug)]
pub(super) struct CmpOpExt(CmpOp);

impl From<&CmpOp> for CmpOpExt {
    fn from(cmp_op: &CmpOp) -> Self {
        CmpOpExt(*cmp_op)
    }
}

impl fmt::Display for CmpOpExt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self.0 {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        };
        write!(f, "{representation}")
    }
}

/// Returns `true` if `bool_op` is `condition and true_value or false_value` form.
pub(super) fn is_and_or_ternary(bool_op: &ExprBoolOp) -> bool {
    let and_op = &bool_op.values[0];
    if !and_op.is_bool_op_expr() {
        return false;
    }
    let false_value = &bool_op.values[1];
    let and_values = &and_op.as_bool_op_expr().unwrap().values;
    let true_value = &and_values[1];
    bool_op.op == BoolOp::Or
        && bool_op.values.len() == 2
        && !false_value.is_bool_op_expr()
        && and_op.as_bool_op_expr().unwrap().op == BoolOp::And
        && !true_value.is_bool_op_expr()
        && and_values.len() == 2
}

#[allow(dead_code)]
fn parse_bool_op(s: &str) -> Option<ExprBoolOp> {
    if let Ok(expr) = parse_expression(s, "<embedded>") {
        expr.as_bool_op_expr().cloned()
    } else {
        None
    }
}

#[allow(dead_code)]
fn is_str_and_or_ternary(s: &str) -> bool {
    if let Some(bool_op) = parse_bool_op(s) {
        is_and_or_ternary(&bool_op)
    } else {
        false
    }
}

#[test]
fn test_is_and_or_ternary() {
    // positive
    assert!(is_str_and_or_ternary("1<2 and 'a' or 'b'"));

    // negative
    assert!(!is_str_and_or_ternary("1<2 and 'a' or 'b' and 'd'")); // 'a' if 1<2 else 'b' and 'd'
    assert!(!is_str_and_or_ternary("1<2 and 'a' or 'b' or 'd'")); // 'a' if 1<2 else 'b' or 'd'
    assert!(!is_str_and_or_ternary("1<2 and 'a'"));
    assert!(!is_str_and_or_ternary("1<2 or 'a'"));
    assert!(!is_str_and_or_ternary("2>1 or 'a' and 'b'"));
    assert!(!is_str_and_or_ternary("2>1"));
    assert!(!is_str_and_or_ternary("'string'"));
}
