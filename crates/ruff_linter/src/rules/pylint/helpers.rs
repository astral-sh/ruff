use std::fmt;

use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, BoolOp, CmpOp, Constant, Expr, ExprBoolOp};
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

/// Returns `Some([condition, true_value, false_value])`
/// if `bool_op` is `condition and true_value or false_value` form.
pub(super) fn parse_and_or_ternary(bool_op: &ExprBoolOp) -> Option<[Expr; 3]> {
    if bool_op.op != BoolOp::Or {
        return None;
    }
    let [expr, false_value] = bool_op.values.as_slice() else {
        return None;
    };
    match expr.as_bool_op_expr() {
        Some(and_op) if and_op.op == BoolOp::And => {
            let [condition, true_value] = and_op.values.as_slice() else {
                return None;
            };
            if !false_value.is_bool_op_expr() && !true_value.is_bool_op_expr() {
                return Some([condition.clone(), true_value.clone(), false_value.clone()]);
            }
        }
        _ => {}
    }
    None
}
