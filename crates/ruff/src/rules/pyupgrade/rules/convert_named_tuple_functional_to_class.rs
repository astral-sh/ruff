use anyhow::{bail, Result};
use log::debug;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, Keyword, Ranged, Stmt};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Generator;
use ruff_python_semantic::model::SemanticModel;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct ConvertNamedTupleFunctionalToClass {
    name: String,
}

impl Violation for ConvertNamedTupleFunctionalToClass {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ConvertNamedTupleFunctionalToClass { name } = self;
        format!("Convert `{name}` from `NamedTuple` functional to class syntax")
    }

    fn autofix_title(&self) -> Option<String> {
        let ConvertNamedTupleFunctionalToClass { name } = self;

        Some(format!("Convert `{name}` to class syntax"))
    }
}

/// Return the typename, args, keywords, and base class.
fn match_named_tuple_assign<'a>(
    model: &SemanticModel,
    targets: &'a [Expr],
    value: &'a Expr,
) -> Option<(&'a str, &'a [Expr], &'a [Keyword], &'a Expr)> {
    let target = targets.get(0)?;
    let Expr::Name(ast::ExprName { id: typename, .. }) = target else {
        return None;
    };
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    }) = value else {
        return None;
    };
    if !model.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "NamedTuple"]
    }) {
        return None;
    }
    Some((typename, args, keywords, func))
}

/// Generate a `Stmt::AnnAssign` representing the provided property
/// definition.
fn create_property_assignment_stmt(
    property: &str,
    annotation: &Expr,
    value: Option<&Expr>,
) -> Stmt {
    let node = ast::ExprName {
        id: property.into(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    };
    let node1 = ast::StmtAnnAssign {
        target: Box::new(node.into()),
        annotation: Box::new(annotation.clone()),
        value: value.map(|value| Box::new(value.clone())),
        simple: true,
        range: TextRange::default(),
    };
    node1.into()
}

/// Match the `defaults` keyword in a `NamedTuple(...)` call.
fn match_defaults(keywords: &[Keyword]) -> Result<&[Expr]> {
    let defaults = keywords.iter().find(|keyword| {
        if let Some(arg) = &keyword.arg {
            arg == "defaults"
        } else {
            false
        }
    });
    match defaults {
        Some(defaults) => match &defaults.value {
            Expr::List(ast::ExprList { elts, .. }) => Ok(elts),
            Expr::Tuple(ast::ExprTuple { elts, .. }) => Ok(elts),
            _ => bail!("Expected defaults to be `Expr::List` | `Expr::Tuple`"),
        },
        None => Ok(&[]),
    }
}

/// Create a list of property assignments from the `NamedTuple` arguments.
fn create_properties_from_args(args: &[Expr], defaults: &[Expr]) -> Result<Vec<Stmt>> {
    let Some(fields) = args.get(1) else {
        let node = Stmt::Pass(ast::StmtPass { range: TextRange::default()});
        return Ok(vec![node]);
    };
    let Expr::List(ast::ExprList { elts, .. } )= &fields else {
        bail!("Expected argument to be `Expr::List`");
    };
    if elts.is_empty() {
        let node = Stmt::Pass(ast::StmtPass {
            range: TextRange::default(),
        });
        return Ok(vec![node]);
    }
    let padded_defaults = if elts.len() >= defaults.len() {
        std::iter::repeat(None)
            .take(elts.len() - defaults.len())
            .chain(defaults.iter().map(Some))
    } else {
        bail!("Defaults must be `None` or an iterable of at least the number of fields")
    };
    elts.iter()
        .zip(padded_defaults)
        .map(|(field, default)| {
            let Expr::Tuple(ast::ExprTuple { elts, .. }) = &field else {
                bail!("Expected `field` to be `Expr::Tuple`")
            };
            let [field_name, annotation] = elts.as_slice() else {
                bail!("Expected `elts` to have exactly two elements")
            };
            let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(property),
                ..
            }) = &field_name else {
                bail!("Expected `field_name` to be `Constant::Str`")
            };
            if !is_identifier(property) {
                bail!("Invalid property name: {}", property)
            }
            Ok(create_property_assignment_stmt(
                property, annotation, default,
            ))
        })
        .collect()
}

/// Generate a `StmtKind:ClassDef` statement based on the provided body and
/// keywords.
fn create_class_def_stmt(typename: &str, body: Vec<Stmt>, base_class: &Expr) -> Stmt {
    let node = ast::StmtClassDef {
        name: typename.into(),
        bases: vec![base_class.clone()],
        keywords: vec![],
        body,
        decorator_list: vec![],
        range: TextRange::default(),
    };
    node.into()
}

/// Generate a `Fix` to convert a `NamedTuple` assignment to a class definition.
fn convert_to_class(
    stmt: &Stmt,
    typename: &str,
    body: Vec<Stmt>,
    base_class: &Expr,
    generator: Generator,
) -> Fix {
    #[allow(deprecated)]
    Fix::unspecified(Edit::range_replacement(
        generator.stmt(&create_class_def_stmt(typename, body, base_class)),
        stmt.range(),
    ))
}

/// UP014
pub(crate) fn convert_named_tuple_functional_to_class(
    checker: &mut Checker,
    stmt: &Stmt,
    targets: &[Expr],
    value: &Expr,
) {
    let Some((typename, args, keywords, base_class)) =
        match_named_tuple_assign(checker.semantic_model(), targets, value) else
    {
        return;
    };

    let properties = match match_defaults(keywords)
        .and_then(|defaults| create_properties_from_args(args, defaults))
    {
        Ok(properties) => properties,
        Err(err) => {
            debug!("Skipping `NamedTuple` \"{typename}\": {err}");
            return;
        }
    };
    // TODO(charlie): Preserve indentation, to remove the first-column requirement.
    let fixable = checker.locator.is_at_start_of_line(stmt.start());
    let mut diagnostic = Diagnostic::new(
        ConvertNamedTupleFunctionalToClass {
            name: typename.to_string(),
        },
        stmt.range(),
    );
    if fixable && checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(convert_to_class(
            stmt,
            typename,
            properties,
            base_class,
            checker.generator(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
