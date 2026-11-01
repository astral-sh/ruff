use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Arguments, Expr, ExprKind, NodeList, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::pyupgrade::fixes::{ClassField, functional_to_class_fix};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for `TypedDict` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `TypedDict` types can be defined either through a functional syntax
/// (`Foo = TypedDict(...)`) or a class syntax (`class Foo(TypedDict): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax.
///
/// Nonetheless, there are some situations in which it is impossible to use
/// the class-based syntax. This rule will not apply to those cases. Namely,
/// it is impossible to use the class-based syntax if any `TypedDict` fields are:
/// - Not valid [python identifiers] (for example, `@x`)
/// - [Python keywords] such as `in`
/// - [Private names] such as `__id` that would undergo [name mangling] at runtime
///   if the class-based syntax was used
/// - [Dunder names] such as `__int__` that can confuse type checkers if they're used
///   with the class-based syntax.
///
/// ## Example
/// ```python
/// from typing import TypedDict
///
/// Foo = TypedDict("Foo", {"a": int, "b": str})
/// ```
///
/// Use instead:
/// ```python
/// from typing import TypedDict
///
///
/// class Foo(TypedDict):
///     a: int
///     b: str
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe if there are any comments within the
/// range of the `TypedDict` definition, as these will be dropped by the
/// autofix.
///
/// ## References
/// - [Python documentation: `typing.TypedDict`](https://docs.python.org/3/library/typing.html#typing.TypedDict)
///
/// [Private names]: https://docs.python.org/3/tutorial/classes.html#private-variables
/// [name mangling]: https://docs.python.org/3/reference/expressions.html#private-name-mangling
/// [python identifiers]: https://docs.python.org/3/reference/lexical_analysis.html#identifiers
/// [Python keywords]: https://docs.python.org/3/reference/lexical_analysis.html#keywords
/// [Dunder names]: https://docs.python.org/3/reference/lexical_analysis.html#reserved-classes-of-identifiers
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155", category = Category::Pedantic)]
pub(crate) struct ConvertTypedDictFunctionalToClass {
    name: String,
}

impl Violation for ConvertTypedDictFunctionalToClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn fix_title(&self) -> Option<String> {
        let ConvertTypedDictFunctionalToClass { name } = self;
        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// UP013
pub(crate) fn convert_typed_dict_functional_to_class(
    checker: &Checker,
    stmt: Stmt<'_>,
    targets: NodeList<'_, ruff_python_ast::kind::Expr>,
    value: Expr<'_>,
) {
    let Some((class_name, arguments, base_class)) =
        match_typed_dict_assign(targets, value, checker.semantic())
    else {
        return;
    };

    let Some((fields, keywords)) = match_fields_and_keywords(arguments) else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
        ConvertTypedDictFunctionalToClass {
            name: class_name.to_string(),
        },
        stmt.range(),
    );
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    if checker.locator().is_at_start_of_line(stmt.start()) {
        diagnostic.set_fix(functional_to_class_fix(
            checker, stmt, class_name, base_class, fields, keywords,
        ));
    }
}

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    targets: NodeList<'a, ruff_python_ast::kind::Expr>,
    value: Expr<'a>,
    semantic: &SemanticModel,
) -> Option<(&'a str, Arguments<'a>, Expr<'a>)> {
    let [target] = targets.as_array()?;
    let class_name = target.as_name_expr()?.id();
    let call = value.as_call_expr()?;
    let base = call.func();
    semantic
        .match_typing_expr(base, "TypedDict")
        .then_some((class_name, call.arguments(), base))
}

/// Match the fields and class keywords from a `TypedDict` call.
fn match_fields_and_keywords(
    arguments: Arguments<'_>,
) -> Option<(
    Vec<ClassField<'_>>,
    NodeList<'_, ruff_python_ast::kind::Keyword>,
)> {
    let args = arguments.args();
    let keywords = arguments.keywords();
    if let Some([_typename, fields]) = args.as_array() {
        let fields = match fields.classify() {
            // TypedDict("MyType", {"a": int, "b": str})
            ExprKind::Dict(dict) => dict
                .items()
                .iter()
                .map(|item| {
                    let name = item.key()?.as_string_literal_expr()?.value().to_str();
                    // Private names are mangled in a class and dunder names can confuse type checkers.
                    if !is_identifier(name) || name.starts_with("__") {
                        return None;
                    }
                    Some(ClassField {
                        name,
                        annotation: item.value(),
                    })
                })
                .collect::<Option<_>>()?,
            // TypedDict("MyType", dict(a=int, b=str))
            ExprKind::Call(call) if call.func().as_name_expr()?.id() == "dict" => {
                ClassField::from_keywords(call.arguments().keywords())?
            }
            _ => return None,
        };
        Some((fields, keywords))
    } else if let Some([_typename]) = args.as_array() {
        // The keyword form was deprecated in Python 3.11 and removed in Python 3.13.
        Some((ClassField::from_keywords(keywords)?, NodeList::default()))
    } else {
        None
    }
}
