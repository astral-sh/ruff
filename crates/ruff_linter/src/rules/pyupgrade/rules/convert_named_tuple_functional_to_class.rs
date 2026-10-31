use log::debug;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::pyupgrade::fixes::{ClassField, functional_to_class_fix};
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for `NamedTuple` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `NamedTuple` subclasses can be defined either through a functional syntax
/// (`Foo = NamedTuple(...)`) or a class syntax (`class Foo(NamedTuple): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax, which exists primarily for backwards compatibility
/// with `collections.namedtuple`.
///
/// ## Example
/// ```python
/// from typing import NamedTuple
///
/// Foo = NamedTuple("Foo", [("a", int), ("b", str)])
/// ```
///
/// Use instead:
/// ```python
/// from typing import NamedTuple
///
///
/// class Foo(NamedTuple):
///     a: int
///     b: str
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe if there are any comments within the
/// range of the `NamedTuple` definition, as these will be dropped by the
/// autofix.
///
/// ## References
/// - [Python documentation: `typing.NamedTuple`](https://docs.python.org/3/library/typing.html#typing.NamedTuple)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155", category = Category::Style)]
pub(crate) struct ConvertNamedTupleFunctionalToClass {
    name: String,
}

impl Violation for ConvertNamedTupleFunctionalToClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn fix_title(&self) -> Option<String> {
        let ConvertNamedTupleFunctionalToClass { name } = self;

        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// UP014
pub(crate) fn convert_named_tuple_functional_to_class(
    checker: &Checker,
    stmt: flat::View<'_, Stmt>,
    targets: flat::NodeList<'_, Expr>,
    value: flat::View<'_, Expr>,
) {
    let Some((typename, call)) = match_named_tuple_assign(targets, value, checker.semantic())
    else {
        return;
    };

    let arguments = call.arguments();
    let fields = match (arguments.args(), arguments.keywords()) {
        // Ex) `NamedTuple("MyType", [("a", int), ("b", str)])`
        (args, keywords)
            if let Some([_typename, fields]) = args.as_array()
                && keywords.is_empty() =>
        {
            if let Some(fields) = fields_from_list(fields) {
                fields
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse fields");
                return;
            }
        }
        // Ex) `NamedTuple("MyType", a=int, b=str)`
        (args, keywords) if let Some([_typename]) = args.as_array() => {
            if let Some(fields) = ClassField::from_keywords(keywords) {
                fields
            } else {
                debug!("Skipping `NamedTuple` \"{typename}\": unable to parse keywords");
                return;
            }
        }
        // Ex) `NamedTuple()`
        _ => {
            debug!("Skipping `NamedTuple` \"{typename}\": mixed fields and keywords");
            return;
        }
    };

    let mut diagnostic = checker.report_diagnostic(
        ConvertNamedTupleFunctionalToClass {
            name: typename.to_string(),
        },
        stmt.range(),
    );
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    if checker.locator().is_at_start_of_line(stmt.start()) {
        diagnostic.set_fix(functional_to_class_fix(
            checker,
            stmt,
            typename,
            call.func(),
            fields,
            flat::NodeList::default(),
        ));
    }
}

/// Match an assignment to the result of calling `typing.NamedTuple`.
fn match_named_tuple_assign<'a>(
    targets: flat::NodeList<'a, Expr>,
    value: flat::View<'a, Expr>,
    semantic: &SemanticModel,
) -> Option<(&'a str, flat::View<'a, ast::ExprCall>)> {
    let [target] = targets.as_array()?;
    let name = target.as_name_expr()?.id();
    let call = value.as_call_expr()?;
    semantic
        .match_typing_expr(call.func(), "NamedTuple")
        .then_some((name, call))
}

fn fields_from_list(fields: flat::View<'_, Expr>) -> Option<Vec<ClassField<'_>>> {
    fields
        .as_list_expr()?
        .elts()
        .iter()
        .map(|field| {
            let [name, annotation] = field.as_tuple_expr()?.elts().as_array()?;
            let name = name.as_string_literal_expr()?.value().to_str();
            if annotation.is_starred_expr() || !is_identifier(name) || is_dunder(name) {
                return None;
            }
            Some(ClassField { name, annotation })
        })
        .collect()
}
