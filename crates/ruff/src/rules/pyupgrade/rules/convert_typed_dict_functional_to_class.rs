use anyhow::{bail, Result};
use log::debug;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{
    self, Constant, Expr, ExprContext, Identifier, Keyword, Ranged, Stmt,
};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::source_code::Generator;
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `TypedDict` declarations that use functional syntax.
///
/// ## Why is this bad?
/// `TypedDict` subclasses can be defined either through a functional syntax
/// (`Foo = TypedDict(...)`) or a class syntax (`class Foo(TypedDict): ...`).
///
/// The class syntax is more readable and generally preferred over the
/// functional syntax.
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
/// ## References
/// - [Python documentation: `typing.TypedDict`](https://docs.python.org/3/library/typing.html#typing.TypedDict)
#[violation]
pub struct ConvertTypedDictFunctionalToClass {
    name: String,
}

impl Violation for ConvertTypedDictFunctionalToClass {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertTypedDictFunctionalToClass { name } = self;
        format!("Convert `{name}` from `TypedDict` functional to class syntax")
    }

    fn autofix_title(&self) -> Option<String> {
        let ConvertTypedDictFunctionalToClass { name } = self;
        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// Return the class name, arguments, keywords and base class for a `TypedDict`
/// assignment.
fn match_typed_dict_assign<'a>(
    targets: &'a [Expr],
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let target = targets.get(0)?;
    let Expr::Name(ast::ExprName { id: class_name, .. }) = target else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    }) = value
    else {
        return None;
    };
    if !semantic.match_typing_expr(func, "TypedDict") {
        return None;
    }
    Some((class_name, args, keywords, func))
}

/// Generate a `Stmt::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(property: &str, annotation: &Expr) -> Stmt {
    ast::StmtAnnAssign {
        target: Box::new(
            ast::ExprName {
                id: property.into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            }
            .into(),
        ),
        annotation: Box::new(annotation.clone()),
        value: None,
        simple: true,
        range: TextRange::default(),
    }
    .into()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body,
/// keywords and base class.
fn create_class_def_stmt(
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
) -> Stmt {
    let keywords = match total_keyword {
        Some(keyword) => vec![keyword.clone()],
        None => vec![],
    };
    ast::StmtClassDef {
        name: Identifier::new(class_name.to_string(), TextRange::default()),
        bases: vec![base_class.clone()],
        keywords,
        body,
        decorator_list: vec![],
        range: TextRange::default(),
    }
    .into()
}

fn properties_from_dict_literal(keys: &[Option<Expr>], values: &[Expr]) -> Result<Vec<Stmt>> {
    if keys.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Ok(vec![node]);
    }

    keys.iter()
        .zip(values.iter())
        .map(|(key, value)| match key {
            Some(Expr::Constant(ast::ExprConstant {
                value: Constant::Str(property),
                ..
            })) => {
                if !is_identifier(property) {
                    bail!("Invalid property name: {}", property)
                }
                if is_dunder(property) {
                    bail!("Cannot use dunder property name: {}", property)
                }
                Ok(create_property_assignment_stmt(property, value))
            }
            _ => bail!("Expected `key` to be `Constant::Str`"),
        })
        .collect()
}

fn properties_from_dict_call(func: &Expr, keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        bail!("Expected `func` to be `Expr::Name`")
    };
    if id != "dict" {
        bail!("Expected `id` to be `\"dict\"`")
    }
    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Ok(vec![node]);
    }

    properties_from_keywords(keywords)
}

// Deprecated in Python 3.11, removed in Python 3.13.
fn properties_from_keywords(keywords: &[Keyword]) -> Result<Vec<Stmt>> {
    if keywords.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Ok(vec![node]);
    }

    keywords
        .iter()
        .map(|keyword| {
            if let Some(property) = &keyword.arg {
                Ok(create_property_assignment_stmt(property, &keyword.value))
            } else {
                bail!("Expected `arg` to be `Some`")
            }
        })
        .collect()
}

// The only way to have the `total` keyword is to use the args version, like:
// ```
// TypedDict('name', {'a': int}, total=True)
// ```
fn match_total_from_only_keyword(keywords: &[Keyword]) -> Option<&Keyword> {
    keywords.iter().find(|keyword| {
        let Some(arg) = &keyword.arg else {
            return false;
        };
        arg.as_str() == "total"
    })
}

fn match_properties_and_total<'a>(
    args: &'a [Expr],
    keywords: &'a [Keyword],
) -> Result<(Vec<Stmt>, Option<&'a Keyword>)> {
    // We don't have to manage the hybrid case because it's not possible to have a
    // dict and keywords. For example, the following is illegal:
    // ```
    // MyType = TypedDict('MyType', {'a': int, 'b': str}, a=int, b=str)
    // ```
    if let Some(dict) = args.get(1) {
        let total = match_total_from_only_keyword(keywords);
        match dict {
            Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _,
            }) => Ok((properties_from_dict_literal(keys, values)?, total)),
            Expr::Call(ast::ExprCall { func, keywords, .. }) => {
                Ok((properties_from_dict_call(func, keywords)?, total))
            }
            _ => bail!("Expected `arg` to be `Expr::Dict` or `Expr::Call`"),
        }
    } else if !keywords.is_empty() {
        Ok((properties_from_keywords(keywords)?, None))
    } else {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        Ok((vec![node], None))
    }
}

/// Generate a `Fix` to convert a `TypedDict` from functional to class.
fn convert_to_class(
    stmt: &Stmt,
    class_name: &str,
    body: Vec<Stmt>,
    total_keyword: Option<&Keyword>,
    base_class: &Expr,
    generator: Generator,
) -> Fix {
    Fix::suggested(Edit::range_replacement(
        generator.stmt(&create_class_def_stmt(
            class_name,
            body,
            total_keyword,
            base_class,
        )),
        stmt.range(),
    ))
}

/// UP013
pub(crate) fn convert_typed_dict_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((class_name, args, keywords, base_class)) =
        match_typed_dict_assign(targets, value, checker.semantic())
    else {
        return;
    };

    let (body, total_keyword) = match match_properties_and_total(args, keywords) {
        Ok((body, total_keyword)) => (body, total_keyword),
        Err(err) => {
            debug!("Skipping ineligible `TypedDict` \"{class_name}\": {err}");
            return;
        }
    };

    let mut diagnostic = Diagnostic::new(
        ConvertTypedDictFunctionalToClass {
            name: class_name.to_string(),
        },
        stmt.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        // TODO(charlie): Preserve indentation, to remove the first-column requirement.
        if checker.locator.is_at_start_of_line(stmt.start()) {
            diagnostic.set_fix(convert_to_class(
                stmt,
                class_name,
                body,
                total_keyword,
                base_class,
                checker.generator(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}
