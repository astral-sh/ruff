//! Analysis rules for the `typing` module.

use ruff_python_ast::helpers::{any_over_expr, map_subscript};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{
    self as ast, Expr, ExprCall, ExprName, Operator, ParameterWithDefault, Parameters, Stmt,
    StmtAssign,
};
use ruff_python_stdlib::typing::{
    as_pep_585_generic, has_pep_585_generic, is_immutable_generic_type,
    is_immutable_non_generic_type, is_immutable_return_type, is_literal_member,
    is_mutable_return_type, is_pep_593_generic_member, is_pep_593_generic_type,
    is_standard_library_generic, is_standard_library_generic_member, is_standard_library_literal,
    is_typed_dict, is_typed_dict_member,
};
use ruff_text_size::Ranged;
use smallvec::{smallvec, SmallVec};

use crate::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use crate::model::SemanticModel;
use crate::{Binding, BindingKind, Modules};

#[derive(Debug, Copy, Clone)]
pub enum Callable {
    Bool,
    Cast,
    NewType,
    TypeVar,
    NamedTuple,
    TypedDict,
    MypyExtension,
    TypeAliasType,
}

#[derive(Debug, Copy, Clone)]
pub enum SubscriptKind {
    /// A subscript of the form `typing.Literal["foo", "bar"]`, i.e., a literal.
    Literal,
    /// A subscript of the form `typing.List[int]`, i.e., a generic.
    Generic,
    /// A subscript of the form `typing.Annotated[int, "foo"]`, i.e., a PEP 593 annotation.
    PEP593Annotation,
    /// A subscript of the form `typing.TypedDict[{"key": Type}]`, i.e., a [PEP 764] annotation.
    ///
    /// [PEP 764]: https://github.com/python/peps/pull/4082
    TypedDict,
}

pub fn is_known_to_be_of_type_dict(semantic: &SemanticModel, expr: &ExprName) -> bool {
    let Some(binding) = semantic.only_binding(expr).map(|id| semantic.binding(id)) else {
        return false;
    };

    is_dict(binding, semantic)
}

pub fn match_annotated_subscript<'a>(
    expr: &Expr,
    semantic: &SemanticModel,
    typing_modules: impl Iterator<Item = &'a str>,
    extend_generics: &[String],
) -> Option<SubscriptKind> {
    semantic
        .resolve_qualified_name(expr)
        .and_then(|qualified_name| {
            if is_standard_library_literal(qualified_name.segments()) {
                return Some(SubscriptKind::Literal);
            }

            if is_standard_library_generic(qualified_name.segments())
                || extend_generics
                    .iter()
                    .map(|target| QualifiedName::from_dotted_name(target))
                    .any(|target| qualified_name == target)
            {
                return Some(SubscriptKind::Generic);
            }

            if is_pep_593_generic_type(qualified_name.segments()) {
                return Some(SubscriptKind::PEP593Annotation);
            }

            if is_typed_dict(qualified_name.segments()) {
                return Some(SubscriptKind::TypedDict);
            }

            for module in typing_modules {
                let module_qualified_name = QualifiedName::user_defined(module);
                if qualified_name.starts_with(&module_qualified_name) {
                    if let Some(member) = qualified_name.segments().last() {
                        if is_literal_member(member) {
                            return Some(SubscriptKind::Literal);
                        }
                        if is_standard_library_generic_member(member) {
                            return Some(SubscriptKind::Generic);
                        }
                        if is_pep_593_generic_member(member) {
                            return Some(SubscriptKind::PEP593Annotation);
                        }
                        if is_typed_dict_member(member) {
                            return Some(SubscriptKind::TypedDict);
                        }
                    }
                }
            }

            None
        })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ModuleMember {
    /// A builtin symbol, like `"list"`.
    BuiltIn(&'static str),
    /// A module member, like `("collections", "deque")`.
    Member(&'static str, &'static str),
}

impl std::fmt::Display for ModuleMember {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ModuleMember::BuiltIn(name) => std::write!(f, "{name}"),
            ModuleMember::Member(module, member) => std::write!(f, "{module}.{member}"),
        }
    }
}

/// Returns the PEP 585 standard library generic variant for a `typing` module reference, if such
/// a variant exists.
pub fn to_pep585_generic(expr: &Expr, semantic: &SemanticModel) -> Option<ModuleMember> {
    semantic
        .seen_module(Modules::TYPING | Modules::TYPING_EXTENSIONS)
        .then(|| semantic.resolve_qualified_name(expr))
        .flatten()
        .and_then(|qualified_name| {
            let [module, member] = qualified_name.segments() else {
                return None;
            };
            as_pep_585_generic(module, member).map(|(module, member)| {
                if module.is_empty() {
                    ModuleMember::BuiltIn(member)
                } else {
                    ModuleMember::Member(module, member)
                }
            })
        })
}

/// Return whether a given expression uses a PEP 585 standard library generic.
pub fn is_pep585_generic(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            let [module, name] = qualified_name.segments() else {
                return false;
            };
            has_pep_585_generic(module, name)
        })
}

#[derive(Debug, Copy, Clone)]
pub enum Pep604Operator {
    /// The union operator, e.g., `Union[str, int]`, expressible as `str | int` after PEP 604.
    Union,
    /// The union operator, e.g., `Optional[str]`, expressible as `str | None` after PEP 604.
    Optional,
}

/// Return the PEP 604 operator variant to which the given subscript [`Expr`] corresponds, if any.
pub fn to_pep604_operator(
    value: &Expr,
    slice: &Expr,
    semantic: &SemanticModel,
) -> Option<Pep604Operator> {
    /// Returns `true` if any argument in the slice is a quoted annotation.
    fn quoted_annotation(slice: &Expr) -> bool {
        match slice {
            Expr::StringLiteral(_) => true,
            Expr::Tuple(tuple) => tuple.iter().any(quoted_annotation),
            _ => false,
        }
    }

    /// Returns `true` if any argument in the slice is a starred expression.
    fn starred_annotation(slice: &Expr) -> bool {
        match slice {
            Expr::Starred(_) => true,
            Expr::Tuple(tuple) => tuple.iter().any(starred_annotation),
            _ => false,
        }
    }

    // If the typing modules were never imported, we'll never match below.
    if !semantic.seen_typing() {
        return None;
    }

    // If the slice is a forward reference (e.g., `Optional["Foo"]`), it can only be rewritten
    // if we're in a typing-only context.
    //
    // This, for example, is invalid, as Python will evaluate `"Foo" | None` at runtime in order to
    // populate the function's `__annotations__`:
    // ```python
    // def f(x: "Foo" | None): ...
    // ```
    //
    // This, however, is valid:
    // ```python
    // def f():
    //     x: "Foo" | None
    // ```
    if quoted_annotation(slice) {
        if semantic.execution_context().is_runtime() {
            return None;
        }
    }

    // If any of the elements are starred expressions, we can't rewrite the subscript:
    // ```python
    // def f(x: Union[*int, str]): ...
    // ```
    if starred_annotation(slice) {
        return None;
    }

    semantic
        .resolve_qualified_name(value)
        .as_ref()
        .and_then(|qualified_name| {
            if semantic.match_typing_qualified_name(qualified_name, "Optional") {
                Some(Pep604Operator::Optional)
            } else if semantic.match_typing_qualified_name(qualified_name, "Union") {
                Some(Pep604Operator::Union)
            } else {
                None
            }
        })
}

/// Return `true` if `Expr` represents a reference to a type annotation that resolves to an
/// immutable type.
pub fn is_immutable_annotation(
    expr: &Expr,
    semantic: &SemanticModel,
    extend_immutable_calls: &[QualifiedName],
) -> bool {
    match expr {
        Expr::Name(_) | Expr::Attribute(_) => {
            semantic
                .resolve_qualified_name(expr)
                .is_some_and(|qualified_name| {
                    is_immutable_non_generic_type(qualified_name.segments())
                        || is_immutable_generic_type(qualified_name.segments())
                        || extend_immutable_calls
                            .iter()
                            .any(|target| qualified_name == *target)
                })
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => semantic
            .resolve_qualified_name(value)
            .is_some_and(|qualified_name| {
                if is_immutable_generic_type(qualified_name.segments()) {
                    true
                } else if matches!(qualified_name.segments(), ["typing", "Union"]) {
                    if let Expr::Tuple(tuple) = &**slice {
                        tuple.iter().all(|element| {
                            is_immutable_annotation(element, semantic, extend_immutable_calls)
                        })
                    } else {
                        false
                    }
                } else if matches!(qualified_name.segments(), ["typing", "Optional"]) {
                    is_immutable_annotation(slice, semantic, extend_immutable_calls)
                } else if is_pep_593_generic_type(qualified_name.segments()) {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        elts.first().is_some_and(|elt| {
                            is_immutable_annotation(elt, semantic, extend_immutable_calls)
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            }),
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
            range: _,
        }) => {
            is_immutable_annotation(left, semantic, extend_immutable_calls)
                && is_immutable_annotation(right, semantic, extend_immutable_calls)
        }
        Expr::NoneLiteral(_) => true,
        _ => false,
    }
}

/// Return `true` if `func` is a function that returns an immutable value.
pub fn is_immutable_func(
    func: &Expr,
    semantic: &SemanticModel,
    extend_immutable_calls: &[QualifiedName],
) -> bool {
    semantic
        .resolve_qualified_name(map_subscript(func))
        .is_some_and(|qualified_name| {
            is_immutable_return_type(qualified_name.segments())
                || extend_immutable_calls
                    .iter()
                    .any(|target| qualified_name == *target)
        })
}

/// Return `true` if `name` is bound to the `typing.NewType` call where the original type is
/// immutable.
///
/// For example:
/// ```python
/// from typing import NewType
///
/// UserId = NewType("UserId", int)
/// ```
///
/// Here, `name` would be `UserId`.
pub fn is_immutable_newtype_call(
    name: &ast::ExprName,
    semantic: &SemanticModel,
    extend_immutable_calls: &[QualifiedName],
) -> bool {
    let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
        return false;
    };

    if !binding.kind.is_assignment() {
        return false;
    }

    let Some(Stmt::Assign(StmtAssign { value, .. })) = binding.statement(semantic) else {
        return false;
    };

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return false;
    };

    if !semantic.match_typing_expr(func, "NewType") {
        return false;
    }

    if arguments.len() != 2 {
        return false;
    }

    let Some(original_type) = arguments.find_argument_value("tp", 1) else {
        return false;
    };

    is_immutable_annotation(original_type, semantic, extend_immutable_calls)
}

/// Return `true` if `func` is a function that returns a mutable value.
pub fn is_mutable_func(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .as_ref()
        .map(QualifiedName::segments)
        .is_some_and(is_mutable_return_type)
}

/// Return `true` if `expr` is an expression that resolves to a mutable value.
pub fn is_mutable_expr(expr: &Expr, semantic: &SemanticModel) -> bool {
    match expr {
        Expr::List(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::DictComp(_)
        | Expr::SetComp(_) => true,
        Expr::Call(ast::ExprCall { func, .. }) => is_mutable_func(map_subscript(func), semantic),
        _ => false,
    }
}

/// Return `true` if [`ast::StmtIf`] is a guard for a type-checking block.
pub fn is_type_checking_block(stmt: &ast::StmtIf, semantic: &SemanticModel) -> bool {
    let ast::StmtIf { test, .. } = stmt;

    match test.as_ref() {
        // As long as the symbol's name is "TYPE_CHECKING" we will treat it like `typing.TYPE_CHECKING`
        // for this specific check even if it's defined somewhere else, like the current module.
        // Ex) `if TYPE_CHECKING:`
        Expr::Name(ast::ExprName { id, .. }) => {
            id == "TYPE_CHECKING"
                // Ex) `if TC:` with `from typing import TYPE_CHECKING as TC`
                || semantic.match_typing_expr(test, "TYPE_CHECKING")
        }
        // Ex) `if typing.TYPE_CHECKING:`
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr == "TYPE_CHECKING",
        _ => false,
    }
}

/// Returns `true` if the [`ast::StmtIf`] is a version-checking block (e.g., `if sys.version_info >= ...:`).
pub fn is_sys_version_block(stmt: &ast::StmtIf, semantic: &SemanticModel) -> bool {
    let ast::StmtIf { test, .. } = stmt;

    any_over_expr(test, &|expr| {
        semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["sys", "version_info" | "platform"]
                )
            })
    })
}

/// Traverse a "union" type annotation, applying `func` to each union member.
///
/// Supports traversal of `Union` and `|` union expressions.
///
/// The function is called with each expression in the union (excluding declarations of nested
/// unions) and the parent expression.
pub fn traverse_union<'a, F>(func: &mut F, semantic: &SemanticModel, expr: &'a Expr)
where
    F: FnMut(&'a Expr, &'a Expr),
{
    fn inner<'a, F>(
        func: &mut F,
        semantic: &SemanticModel,
        expr: &'a Expr,
        parent: Option<&'a Expr>,
    ) where
        F: FnMut(&'a Expr, &'a Expr),
    {
        // Ex) `x | y`
        if let Expr::BinOp(ast::ExprBinOp {
            op: Operator::BitOr,
            left,
            right,
            range: _,
        }) = expr
        {
            // The union data structure usually looks like this:
            //  a | b | c -> (a | b) | c
            //
            // However, parenthesized expressions can coerce it into any structure:
            //  a | (b | c)
            //
            // So we have to traverse both branches in order (left, then right), to report members
            // in the order they appear in the source code.

            // Traverse the left then right arms
            inner(func, semantic, left, Some(expr));
            inner(func, semantic, right, Some(expr));
            return;
        }

        // Ex) `Union[x, y]`
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if semantic.match_typing_expr(value, "Union") {
                if let Expr::Tuple(tuple) = &**slice {
                    // Traverse each element of the tuple within the union recursively to handle cases
                    // such as `Union[..., Union[...]]`
                    tuple
                        .iter()
                        .for_each(|elem| inner(func, semantic, elem, Some(expr)));
                    return;
                }

                // Ex) `Union[Union[a, b]]` and `Union[a | b | c]`
                inner(func, semantic, slice, Some(expr));
                return;
            }
        }

        // Otherwise, call the function on expression, if it's not the top-level expression.
        if let Some(parent) = parent {
            func(expr, parent);
        }
    }

    inner(func, semantic, expr, None);
}

/// Traverse a "literal" type annotation, applying `func` to each literal member.
///
/// The function is called with each expression in the literal (excluding declarations of nested
/// literals) and the parent expression.
pub fn traverse_literal<'a, F>(func: &mut F, semantic: &SemanticModel, expr: &'a Expr)
where
    F: FnMut(&'a Expr, &'a Expr),
{
    fn inner<'a, F>(
        func: &mut F,
        semantic: &SemanticModel,
        expr: &'a Expr,
        parent: Option<&'a Expr>,
    ) where
        F: FnMut(&'a Expr, &'a Expr),
    {
        // Ex) `Literal[x, y]`
        if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if semantic.match_typing_expr(value, "Literal") {
                match &**slice {
                    Expr::Tuple(tuple) => {
                        // Traverse each element of the tuple within the literal recursively to handle cases
                        // such as `Literal[..., Literal[...]]`
                        for element in tuple {
                            inner(func, semantic, element, Some(expr));
                        }
                    }
                    other => {
                        // Ex) `Literal[Literal[...]]`
                        inner(func, semantic, other, Some(expr));
                    }
                }
            }
        } else {
            // Otherwise, call the function on expression, if it's not the top-level expression.
            if let Some(parent) = parent {
                func(expr, parent);
            }
        }
    }

    inner(func, semantic, expr, None);
}

/// Abstraction for a type checker, conservatively checks for the intended type(s).
pub trait TypeChecker {
    /// Check annotation expression to match the intended type(s).
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool;
    /// Check initializer expression to match the intended type(s).
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool;
}

/// Check if the type checker accepts the given binding with the given name.
///
/// NOTE: this function doesn't perform more serious type inference, so it won't be able
///       to understand if the value gets initialized from a call to a function always returning
///       lists. This also implies no interfile analysis.
pub fn check_type<T: TypeChecker>(binding: &Binding, semantic: &SemanticModel) -> bool {
    match binding.kind {
        BindingKind::Assignment => match binding.statement(semantic) {
            // Given:
            //
            // ```python
            // x = init_expr
            // ```
            //
            // The type checker might know how to infer the type based on `init_expr`.
            Some(Stmt::Assign(ast::StmtAssign { targets, value, .. })) => targets
                .iter()
                .find_map(|target| match_value(binding, target, value))
                .is_some_and(|value| T::match_initializer(value, semantic)),

            // ```python
            // x: annotation = some_expr
            // ```
            //
            // In this situation, we check only the annotation.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation, semantic)
            }

            _ => false,
        },

        BindingKind::NamedExprAssignment => {
            // ```python
            // if (x := some_expr) is not None:
            //     ...
            // ```
            binding.source.is_some_and(|source| {
                semantic
                    .expressions(source)
                    .find_map(|expr| expr.as_named_expr())
                    .and_then(|ast::ExprNamed { target, value, .. }| {
                        match_value(binding, target, value)
                    })
                    .is_some_and(|value| T::match_initializer(value, semantic))
            })
        }

        BindingKind::WithItemVar => match binding.statement(semantic) {
            // ```python
            // with open("file.txt") as x:
            //     ...
            // ```
            Some(Stmt::With(ast::StmtWith { items, .. })) => items
                .iter()
                .find_map(|item| {
                    let target = item.optional_vars.as_ref()?;
                    let value = &item.context_expr;
                    match_value(binding, target, value)
                })
                .is_some_and(|value| T::match_initializer(value, semantic)),

            _ => false,
        },

        BindingKind::Argument => match binding.statement(semantic) {
            // ```python
            // def foo(x: annotation):
            //   ...
            // ```
            //
            // We trust the annotation and see if the type checker matches the annotation.
            Some(Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) => {
                let Some(parameter) = find_parameter(parameters, binding) else {
                    return false;
                };
                let Some(annotation) = parameter.annotation() else {
                    return false;
                };
                T::match_annotation(annotation, semantic)
            }

            _ => false,
        },

        BindingKind::Annotation => match binding.statement(semantic) {
            // ```python
            // x: annotation
            // ```
            //
            // It's a typed declaration, type annotation is the only source of information.
            Some(Stmt::AnnAssign(ast::StmtAnnAssign { annotation, .. })) => {
                T::match_annotation(annotation, semantic)
            }
            _ => false,
        },

        _ => false,
    }
}

/// Type checker for builtin types.
trait BuiltinTypeChecker {
    /// Builtin type name.
    const BUILTIN_TYPE_NAME: &'static str;
    /// Type name as found in the `Typing` module.
    const TYPING_NAME: Option<&'static str>;
    /// [`PythonType`] associated with the intended type.
    const EXPR_TYPE: PythonType;

    /// Check annotation expression to match the intended type.
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        let value = map_subscript(annotation);
        semantic.match_builtin_expr(value, Self::BUILTIN_TYPE_NAME)
            || Self::TYPING_NAME.is_some_and(|name| semantic.match_typing_expr(value, name))
    }

    /// Check initializer expression to match the intended type.
    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        Self::match_expr_type(initializer) || Self::match_builtin_constructor(initializer, semantic)
    }

    /// Check if the type can be inferred from the given expression.
    fn match_expr_type(initializer: &Expr) -> bool {
        let init_type: ResolvedPythonType = initializer.into();
        match init_type {
            ResolvedPythonType::Atom(atom) => atom == Self::EXPR_TYPE,
            _ => false,
        }
    }

    /// Check if the given expression corresponds to a constructor call of the builtin type.
    fn match_builtin_constructor(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };
        semantic.match_builtin_expr(func, Self::BUILTIN_TYPE_NAME)
    }
}

impl<T: BuiltinTypeChecker> TypeChecker for T {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_annotation(annotation, semantic)
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        <Self as BuiltinTypeChecker>::match_initializer(initializer, semantic)
    }
}

struct ListChecker;

impl BuiltinTypeChecker for ListChecker {
    const BUILTIN_TYPE_NAME: &'static str = "list";
    const TYPING_NAME: Option<&'static str> = Some("List");
    const EXPR_TYPE: PythonType = PythonType::List;
}

struct DictChecker;

impl BuiltinTypeChecker for DictChecker {
    const BUILTIN_TYPE_NAME: &'static str = "dict";
    const TYPING_NAME: Option<&'static str> = Some("Dict");
    const EXPR_TYPE: PythonType = PythonType::Dict;
}

struct SetChecker;

impl BuiltinTypeChecker for SetChecker {
    const BUILTIN_TYPE_NAME: &'static str = "set";
    const TYPING_NAME: Option<&'static str> = Some("Set");
    const EXPR_TYPE: PythonType = PythonType::Set;
}

struct StringChecker;

impl BuiltinTypeChecker for StringChecker {
    const BUILTIN_TYPE_NAME: &'static str = "str";
    const TYPING_NAME: Option<&'static str> = None;
    const EXPR_TYPE: PythonType = PythonType::String;
}

struct BytesChecker;

impl BuiltinTypeChecker for BytesChecker {
    const BUILTIN_TYPE_NAME: &'static str = "bytes";
    const TYPING_NAME: Option<&'static str> = None;
    const EXPR_TYPE: PythonType = PythonType::Bytes;
}

struct TupleChecker;

impl BuiltinTypeChecker for TupleChecker {
    const BUILTIN_TYPE_NAME: &'static str = "tuple";
    const TYPING_NAME: Option<&'static str> = Some("Tuple");
    const EXPR_TYPE: PythonType = PythonType::Tuple;
}

struct IntChecker;

impl BuiltinTypeChecker for IntChecker {
    const BUILTIN_TYPE_NAME: &'static str = "int";
    const TYPING_NAME: Option<&'static str> = None;
    const EXPR_TYPE: PythonType = PythonType::Number(NumberLike::Integer);
}

struct FloatChecker;

impl BuiltinTypeChecker for FloatChecker {
    const BUILTIN_TYPE_NAME: &'static str = "float";
    const TYPING_NAME: Option<&'static str> = None;
    const EXPR_TYPE: PythonType = PythonType::Number(NumberLike::Float);
}

pub struct IoBaseChecker;

impl TypeChecker for IoBaseChecker {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        semantic
            .resolve_qualified_name(annotation)
            .is_some_and(|qualified_name| {
                if semantic.match_typing_qualified_name(&qualified_name, "IO") {
                    return true;
                }
                if semantic.match_typing_qualified_name(&qualified_name, "BinaryIO") {
                    return true;
                }
                if semantic.match_typing_qualified_name(&qualified_name, "TextIO") {
                    return true;
                }
                matches!(
                    qualified_name.segments(),
                    [
                        "io",
                        "IOBase"
                            | "RawIOBase"
                            | "BufferedIOBase"
                            | "TextIOBase"
                            | "BytesIO"
                            | "StringIO"
                            | "BufferedReader"
                            | "BufferedWriter"
                            | "BufferedRandom"
                            | "BufferedRWPair"
                            | "TextIOWrapper"
                    ] | ["os", "Path" | "PathLike"]
                        | [
                            "pathlib",
                            "Path" | "PurePath" | "PurePosixPath" | "PureWindowsPath"
                        ]
                )
            })
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };

        // Ex) `pathlib.Path("file.txt")`
        if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
            if attr.as_str() == "open" {
                if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
                    return semantic
                        .resolve_qualified_name(func)
                        .is_some_and(|qualified_name| {
                            matches!(
                                qualified_name.segments(),
                                [
                                    "pathlib",
                                    "Path" | "PurePath" | "PurePosixPath" | "PureWindowsPath"
                                ]
                            )
                        });
                }
            }
        }

        // Ex) `open("file.txt")`
        semantic
            .resolve_qualified_name(func)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["io", "open" | "open_code"] | ["os" | "" | "builtins", "open"]
                )
            })
    }
}

pub struct PathlibPathChecker;

impl PathlibPathChecker {
    fn is_pathlib_path_constructor(semantic: &SemanticModel, expr: &Expr) -> bool {
        let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
            return false;
        };

        matches!(
            qualified_name.segments(),
            [
                "pathlib",
                "Path"
                    | "PosixPath"
                    | "PurePath"
                    | "PurePosixPath"
                    | "PureWindowsPath"
                    | "WindowsPath"
            ]
        )
    }
}

impl TypeChecker for PathlibPathChecker {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        Self::is_pathlib_path_constructor(semantic, annotation)
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };

        Self::is_pathlib_path_constructor(semantic, func)
    }
}

pub struct FastApiRouteChecker;

impl FastApiRouteChecker {
    fn is_fastapi_route_constructor(semantic: &SemanticModel, expr: &Expr) -> bool {
        let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
            return false;
        };

        matches!(
            qualified_name.segments(),
            ["fastapi", "FastAPI" | "APIRouter"]
        )
    }
}

impl TypeChecker for FastApiRouteChecker {
    fn match_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
        Self::is_fastapi_route_constructor(semantic, annotation)
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = initializer else {
            return false;
        };

        Self::is_fastapi_route_constructor(semantic, func)
    }
}

pub struct TypeVarLikeChecker;

impl TypeVarLikeChecker {
    /// Returns `true` if an [`Expr`] is a `TypeVar`, `TypeVarTuple`, or `ParamSpec` call.
    ///
    /// See also [`ruff_linter::rules::flake8_pyi::rules::simple_defaults::is_type_var_like_call`].
    fn is_type_var_like_call(expr: &Expr, semantic: &SemanticModel) -> bool {
        let Expr::Call(ast::ExprCall { func, .. }) = expr else {
            return false;
        };
        let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
            return false;
        };

        matches!(
            qualified_name.segments(),
            [
                "typing" | "typing_extensions",
                "TypeVar" | "TypeVarTuple" | "ParamSpec"
            ]
        )
    }
}

impl TypeChecker for TypeVarLikeChecker {
    fn match_annotation(_annotation: &Expr, _semantic: &SemanticModel) -> bool {
        false
    }

    fn match_initializer(initializer: &Expr, semantic: &SemanticModel) -> bool {
        Self::is_type_var_like_call(initializer, semantic)
    }
}

/// Test whether the given binding can be considered a list.
///
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `list` and `typing.List`)
pub fn is_list(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<ListChecker>(binding, semantic)
}

/// Test whether the given binding can be considered a dictionary.
///
/// For this, we check what value might be associated with it through it's initialization,
/// what annotation it has (we consider `dict` and `typing.Dict`), and if it is a variadic keyword
/// argument parameter.
pub fn is_dict(binding: &Binding, semantic: &SemanticModel) -> bool {
    // ```python
    // def foo(**kwargs):
    //   ...
    // ```
    if matches!(binding.kind, BindingKind::Argument) {
        if let Some(Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) =
            binding.statement(semantic)
        {
            if let Some(kwarg_parameter) = parameters.kwarg.as_deref() {
                if kwarg_parameter.name.range() == binding.range() {
                    return true;
                }
            }
        }
    }

    check_type::<DictChecker>(binding, semantic)
}

/// Test whether the given binding can be considered an integer.
pub fn is_int(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<IntChecker>(binding, semantic)
}

/// Test whether the given binding can be considered an instance of `float`.
pub fn is_float(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<FloatChecker>(binding, semantic)
}

/// Test whether the given binding can be considered an instance of `str`.
pub fn is_string(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<StringChecker>(binding, semantic)
}

/// Test whether the given binding can be considered an instance of `bytes`.
pub fn is_bytes(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<BytesChecker>(binding, semantic)
}

/// Test whether the given binding can be considered a set.
///
/// For this, we check what value might be associated with it through it's initialization and
/// what annotation it has (we consider `set` and `typing.Set`).
pub fn is_set(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<SetChecker>(binding, semantic)
}

/// Test whether the given binding can be considered a tuple.
///
/// For this, we check what value might be associated with it through it's initialization, what
/// annotation it has (we consider `tuple` and `typing.Tuple`), and if it is a variadic positional
/// argument.
pub fn is_tuple(binding: &Binding, semantic: &SemanticModel) -> bool {
    // ```python
    // def foo(*args):
    //   ...
    // ```
    if matches!(binding.kind, BindingKind::Argument) {
        if let Some(Stmt::FunctionDef(ast::StmtFunctionDef { parameters, .. })) =
            binding.statement(semantic)
        {
            if let Some(arg_parameter) = parameters.vararg.as_deref() {
                if arg_parameter.name.range() == binding.range() {
                    return true;
                }
            }
        }
    }

    check_type::<TupleChecker>(binding, semantic)
}

/// Test whether the given binding can be considered a file-like object (i.e., a type that
/// implements `io.IOBase`).
pub fn is_io_base(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<IoBaseChecker>(binding, semantic)
}

/// Test whether the given expression can be considered a file-like object (i.e., a type that
/// implements `io.IOBase`).
pub fn is_io_base_expr(expr: &Expr, semantic: &SemanticModel) -> bool {
    IoBaseChecker::match_initializer(expr, semantic)
}

/// Test whether the given binding can be considered a `pathlib.PurePath`
/// or an instance of a subclass thereof.
pub fn is_pathlib_path(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<PathlibPathChecker>(binding, semantic)
}

pub fn is_fastapi_route(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<FastApiRouteChecker>(binding, semantic)
}

/// Test whether the given binding is for an old-style `TypeVar`, `TypeVarTuple` or a `ParamSpec`.
pub fn is_type_var_like(binding: &Binding, semantic: &SemanticModel) -> bool {
    check_type::<TypeVarLikeChecker>(binding, semantic)
}

/// Find the [`ParameterWithDefault`] corresponding to the given [`Binding`].
#[inline]
fn find_parameter<'a>(
    parameters: &'a Parameters,
    binding: &Binding,
) -> Option<&'a ParameterWithDefault> {
    parameters
        .iter_non_variadic_params()
        .find(|param| param.identifier() == binding.range())
}

/// Return the [`QualifiedName`] of the value to which the given [`Expr`] is assigned, if any.
///
/// For example, given:
/// ```python
/// import asyncio
///
/// loop = asyncio.get_running_loop()
/// loop.create_task(...)
/// ```
///
/// This function will return `["asyncio", "get_running_loop"]` for the `loop` binding.
///
/// This function will also automatically expand attribute accesses, so given:
/// ```python
/// from module import AppContainer
///
/// container = AppContainer()
/// container.app.get(...)
/// ```
///
/// This function will return `["module", "AppContainer", "app", "get"]` for the
/// attribute access `container.app.get`.
pub fn resolve_assignment<'a>(
    expr: &'a Expr,
    semantic: &'a SemanticModel<'a>,
) -> Option<QualifiedName<'a>> {
    // Resolve any attribute chain.
    let mut head_expr = expr;
    let mut reversed_tail: SmallVec<[_; 4]> = smallvec![];
    while let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = head_expr {
        head_expr = value;
        reversed_tail.push(attr.as_str());
    }

    // Resolve the left-most name, e.g. `foo` in `foo.bar.baz` to a qualified name,
    // then append the attributes.
    let name = head_expr.as_name_expr()?;
    let binding_id = semantic.resolve_name(name)?;
    let statement = semantic.binding(binding_id).statement(semantic)?;
    match statement {
        Stmt::Assign(ast::StmtAssign { value, .. })
        | Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => {
            let ast::ExprCall { func, .. } = value.as_call_expr()?;

            let qualified_name = semantic.resolve_qualified_name(func)?;
            Some(qualified_name.extend_members(reversed_tail.into_iter().rev()))
        }
        _ => None,
    }
}

/// Find the assigned [`Expr`] for a given symbol, if any.
///
/// For example given:
/// ```python
///  foo = 42
///  (bar, bla) = 1, "str"
/// ```
///
/// This function will return a `NumberLiteral` with value `Int(42)` when called with `foo` and a
/// `StringLiteral` with value `"str"` when called with `bla`.
pub fn find_assigned_value<'a>(symbol: &str, semantic: &'a SemanticModel<'a>) -> Option<&'a Expr> {
    let binding_id = semantic.lookup_symbol(symbol)?;
    let binding = semantic.binding(binding_id);
    find_binding_value(binding, semantic)
}

/// Find the assigned [`Expr`] for a given [`Binding`], if any.
///
/// For example given:
/// ```python
///  foo = 42
///  (bar, bla) = 1, "str"
/// ```
///
/// This function will return a `NumberLiteral` with value `Int(42)` when called with `foo` and a
/// `StringLiteral` with value `"str"` when called with `bla`.
#[allow(clippy::single_match)]
pub fn find_binding_value<'a>(binding: &Binding, semantic: &'a SemanticModel) -> Option<&'a Expr> {
    match binding.kind {
        // Ex) `x := 1`
        BindingKind::NamedExprAssignment => {
            let parent_id = binding.source?;
            let parent = semantic
                .expressions(parent_id)
                .find_map(|expr| expr.as_named_expr());
            if let Some(ast::ExprNamed { target, value, .. }) = parent {
                return match_value(binding, target, value);
            }
        }
        // Ex) `x = 1`
        BindingKind::Assignment => match binding.statement(semantic) {
            Some(Stmt::Assign(ast::StmtAssign { value, targets, .. })) => {
                return targets
                    .iter()
                    .find_map(|target| match_value(binding, target, value))
            }
            Some(Stmt::AnnAssign(ast::StmtAnnAssign {
                value: Some(value),
                target,
                ..
            })) => {
                return match_value(binding, target, value);
            }
            _ => {}
        },
        // Ex) `with open("file.txt") as f:`
        BindingKind::WithItemVar => match binding.statement(semantic) {
            Some(Stmt::With(ast::StmtWith { items, .. })) => {
                return items.iter().find_map(|item| {
                    let target = item.optional_vars.as_ref()?;
                    let value = &item.context_expr;
                    match_value(binding, target, value)
                });
            }
            _ => {}
        },
        _ => {}
    }
    None
}

/// Given a target and value, find the value that's assigned to the given symbol.
fn match_value<'a>(binding: &Binding, target: &Expr, value: &'a Expr) -> Option<&'a Expr> {
    match target {
        Expr::Name(name) if name.range() == binding.range() => Some(value),
        Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. }) => {
            match value {
                Expr::Tuple(ast::ExprTuple {
                    elts: value_elts, ..
                })
                | Expr::List(ast::ExprList {
                    elts: value_elts, ..
                })
                | Expr::Set(ast::ExprSet {
                    elts: value_elts, ..
                }) => match_target(binding, elts, value_elts),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Given a target and value, find the value that's assigned to the given symbol.
fn match_target<'a>(binding: &Binding, targets: &[Expr], values: &'a [Expr]) -> Option<&'a Expr> {
    for (target, value) in targets.iter().zip(values) {
        match target {
            Expr::Tuple(ast::ExprTuple {
                elts: target_elts, ..
            })
            | Expr::List(ast::ExprList {
                elts: target_elts, ..
            })
            | Expr::Set(ast::ExprSet {
                elts: target_elts, ..
            }) => {
                // Collection types can be mismatched like in: (a, b, [c, d]) = [1, 2, {3, 4}]
                match value {
                    Expr::Tuple(ast::ExprTuple {
                        elts: value_elts, ..
                    })
                    | Expr::List(ast::ExprList {
                        elts: value_elts, ..
                    })
                    | Expr::Set(ast::ExprSet {
                        elts: value_elts, ..
                    }) => {
                        if let Some(result) = match_target(binding, target_elts, value_elts) {
                            return Some(result);
                        }
                    }
                    _ => (),
                }
            }
            Expr::Name(name) => {
                if name.range() == binding.range() {
                    return Some(value);
                }
            }
            _ => (),
        }
    }
    None
}
