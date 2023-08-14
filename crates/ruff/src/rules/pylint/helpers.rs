use std::fmt;

use ruff_python_ast as ast;
use ruff_python_ast::{Arguments, CmpOp, Constant, Expr};
use ruff_python_semantic::analyze::function_type;
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::settings::Settings;

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

pub(super) fn in_dunder_init(semantic: &SemanticModel, settings: &Settings) -> bool {
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

/// Returns `true` if a method is a known dunder method.
pub(super) fn is_known_dunder_method(method: &str) -> bool {
    matches!(
        method,
        "__abs__"
            | "__add__"
            | "__aenter__"
            | "__aexit__"
            | "__aiter__"
            | "__and__"
            | "__anext__"
            | "__await__"
            | "__bool__"
            | "__bytes__"
            | "__call__"
            | "__ceil__"
            | "__class__"
            | "__class_getitem__"
            | "__complex__"
            | "__contains__"
            | "__copy__"
            | "__deepcopy__"
            | "__del__"
            | "__delattr__"
            | "__delete__"
            | "__delitem__"
            | "__dict__"
            | "__dir__"
            | "__divmod__"
            | "__doc__"
            | "__enter__"
            | "__eq__"
            | "__exit__"
            | "__float__"
            | "__floor__"
            | "__floordiv__"
            | "__format__"
            | "__fspath__"
            | "__ge__"
            | "__get__"
            | "__getattr__"
            | "__getattribute__"
            | "__getitem__"
            | "__getnewargs__"
            | "__getnewargs_ex__"
            | "__getstate__"
            | "__gt__"
            | "__hash__"
            | "__iadd__"
            | "__iand__"
            | "__ifloordiv__"
            | "__ilshift__"
            | "__imatmul__"
            | "__imod__"
            | "__imul__"
            | "__init__"
            | "__init_subclass__"
            | "__instancecheck__"
            | "__int__"
            | "__invert__"
            | "__ior__"
            | "__ipow__"
            | "__irshift__"
            | "__isub__"
            | "__iter__"
            | "__itruediv__"
            | "__ixor__"
            | "__le__"
            | "__len__"
            | "__length_hint__"
            | "__lshift__"
            | "__lt__"
            | "__matmul__"
            | "__missing__"
            | "__mod__"
            | "__module__"
            | "__mul__"
            | "__ne__"
            | "__neg__"
            | "__new__"
            | "__next__"
            | "__or__"
            | "__pos__"
            | "__post_init__"
            | "__pow__"
            | "__radd__"
            | "__rand__"
            | "__rdivmod__"
            | "__reduce__"
            | "__reduce_ex__"
            | "__repr__"
            | "__reversed__"
            | "__rfloordiv__"
            | "__rlshift__"
            | "__rmatmul__"
            | "__rmod__"
            | "__rmul__"
            | "__ror__"
            | "__round__"
            | "__rpow__"
            | "__rrshift__"
            | "__rshift__"
            | "__rsub__"
            | "__rtruediv__"
            | "__rxor__"
            | "__set__"
            | "__set_name__"
            | "__setattr__"
            | "__setitem__"
            | "__setstate__"
            | "__sizeof__"
            | "__str__"
            | "__sub__"
            | "__subclasscheck__"
            | "__subclasses__"
            | "__subclasshook__"
            | "__truediv__"
            | "__trunc__"
            | "__weakref__"
            | "__xor__"
    )
}
