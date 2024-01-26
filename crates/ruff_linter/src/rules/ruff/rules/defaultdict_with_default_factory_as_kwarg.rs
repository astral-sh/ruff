use ast::{ExprAttribute, ExprName, Keyword};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{any_over_expr, is_constant};
use ruff_python_ast::{self as ast, Arguments, Expr, ExprCall, ExprContext};
use ruff_python_semantic::{BindingKind, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary `default_factory` keyword argument when initializing
/// `defaultdict`.
///
/// ## Why is this bad?
/// Type checkers can't reliably infer the type of the default value when using
/// `default_factory` keyword argument.
///
/// Prefer `defaultdict(callable)` over `defaultdict(default_factory=callable)`
///
/// ## Examples
/// ```python
/// defaultdict(default_factory=int)
/// defaultdict(default_factory=list)
/// defaultdict(default_factory=any_callable)
/// ```
///
/// Use instead:
/// ```python
/// defaultdict(int)
/// defaultdict(list)
/// defaultdict(any_callable)
/// ```
#[violation]
pub struct DefaultDictWithDefaultFactoryAsKwArg {
    callable_id: Option<String>,
}

impl Violation for DefaultDictWithDefaultFactoryAsKwArg {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(id) = self.callable_id.as_ref() {
            format!(
                "Prefer using `defaultdict({id})` instead of initializing with `default_factory` keyword argument"
            )
        } else {
            format!(
                "Prefer using `defaultdict(callable)` instead of initializing with `default_factory` keyword argument"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.callable_id {
            Some(ref id) => Some(format!(
                "Prefer using `defaultdict({id})` instead of initializing with `default_factory` keyword argument"
            )),
            None => Some(format!(
                "Prefer using `defaultdict(callable)` instead of initializing with `default_factory` keyword argument"
            )),
        }
    }
}

enum ValueKind<'a> {
    Fixable(&'a str),
    NonFixable,
    NonCallable,
}

/// RUF026
pub(crate) fn default_dict_with_default_factory_as_kwarg(
    checker: &mut Checker,
    default_dict: &ast::ExprCall,
) {
    let ExprCall {
        func, arguments, ..
    } = default_dict;

    if !is_defaultdict(func.as_ref()) {
        return;
    }

    if arguments.keywords.is_empty() {
        return;
    }

    let Some(keyword) = find_default_factory_keyword(arguments) else {
        return;
    };

    match determine_kw_value_kind(keyword, checker.semantic()) {
        ValueKind::Fixable(id) => {
            let mut diagnostic = Diagnostic::new(
                DefaultDictWithDefaultFactoryAsKwArg {
                    callable_id: Some(id.to_string()),
                },
                default_dict.range(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                checker
                    .generator()
                    .expr(&fix_defaultdict_with_default_factory_as_kw_arg(
                        &keyword.value,
                    )),
                default_dict.range(),
            )));

            checker.diagnostics.push(diagnostic);
        }
        ValueKind::NonFixable => {
            let diagnostic = Diagnostic::new(
                DefaultDictWithDefaultFactoryAsKwArg { callable_id: None },
                default_dict.range(),
            );

            checker.diagnostics.push(diagnostic);
        }
        ValueKind::NonCallable => {}
    }
}

fn is_defaultdict(func: &Expr) -> bool {
    let id = match func {
        Expr::Name(ExprName {
            id,
            ctx: _,
            range: _,
        }) => id,
        Expr::Attribute(ExprAttribute { attr, .. }) => attr.as_str(),
        _ => {
            return false;
        }
    };
    id == "defaultdict"
}

fn find_default_factory_keyword(arguments: &Arguments) -> Option<&ast::Keyword> {
    arguments.keywords.iter().find(|&keyword| {
        keyword
            .arg
            .as_ref()
            .is_some_and(|arg| arg.as_str() == "default_factory")
    })
}

fn determine_kw_value_kind<'a>(keyword: &'a Keyword, semantic: &'a SemanticModel) -> ValueKind<'a> {
    if let Some(call_path) = semantic.resolve_call_path(&keyword.value) {
        if let Some(&member) = call_path.last() {
            if semantic.is_builtin(member) {
                return ValueKind::Fixable(member);
            }
        }
    }

    if matches!(keyword.value, Expr::NoneLiteral(_)) {
        return ValueKind::Fixable("None");
    }

    if is_non_callable_value(&keyword.value) {
        return ValueKind::NonCallable;
    }

    match &keyword.value {
        Expr::Name(ExprName {
            id,
            ctx: _,
            range: _,
        }) => {
            let Some(binding_id) = semantic.lookup_symbol(id) else {
                return ValueKind::NonFixable;
            };
            if matches!(
                semantic.binding(binding_id).kind,
                BindingKind::FunctionDefinition(_)
            ) {
                ValueKind::Fixable(id)
            } else {
                ValueKind::NonFixable
            }
        }

        _ => ValueKind::NonFixable,
    }
}

fn is_non_callable_value(value: &Expr) -> bool {
    if is_constant(value) {
        return true;
    }
    any_over_expr(value, &|expr| {
        matches!(expr, |Expr::List(_)| Expr::Dict(_)
            | Expr::Set(_)
            | Expr::Tuple(_)
            | Expr::Slice(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::GeneratorExp(_)
            | Expr::FString(_))
    })
}

/// Generate a [`Fix`] to replace `defaultdict(default_factory=callable)` with `defaultdict(callable)`.
///
/// For example:
/// - Given `defaultdict(default_factory=list)`, generate `defaultdict(list)`.
/// - Given `def foo(): pass` `defaultdict(default_factory=foo)`, generate `defaultdict(foo)`.
fn fix_defaultdict_with_default_factory_as_kw_arg(value: &Expr) -> Expr {
    let args = Arguments {
        args: vec![value.clone()],
        keywords: vec![],
        range: TextRange::default(),
    };
    Expr::Call(ExprCall {
        func: Box::new(Expr::Name(ExprName {
            id: "defaultdict".into(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
        })),
        arguments: args,
        range: TextRange::default(),
    })
}
