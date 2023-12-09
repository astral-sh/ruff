use std::path::Path;

use ruff_python_ast::{Alias, Expr, ExprAttribute, Identifier, Parameter};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::settings::LinterSettings;

use crate::rules::wemake_python_styleguide::helpers::naming;

/// ## What it does
/// Checks for short variable or module names.
///
/// The length of a variable or module name is determined by its number of characters
/// excluding leading or trailing underscores.
///
/// ## Why is this bad?
/// It is hard to understand what the variable means and why it is used, if its name is too short.
///
/// ## Example
/// ```python
/// x = 1
/// y = 2
/// ```
///
/// Use instead:
/// ```python
/// x_coordinate = 1
/// abscissa = 2
/// ```
///
/// ## Options
/// - `lint.wemake-python-styleguide.min-name-length`
#[violation]
pub struct TooShortName {
    name: String,
    is_module: bool,
}

impl Violation for TooShortName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooShortName { name, is_module } = self;
        format!(
            "Found too short {}: `{}`",
            if *is_module { "module name" } else { "name" },
            name,
        )
    }
}

pub(crate) enum Checkable<'a> {
    Identifier(&'a Identifier),
    Parameter(&'a Parameter),
    Alias(&'a Alias),
    Expr(&'a Expr),
}

impl<'a> From<&'a Identifier> for Checkable<'a> {
    fn from(value: &'a Identifier) -> Self {
        Checkable::Identifier(value)
    }
}

impl<'a> From<&'a Parameter> for Checkable<'a> {
    fn from(value: &'a Parameter) -> Self {
        Checkable::Parameter(value)
    }
}

impl<'a> From<&'a Alias> for Checkable<'a> {
    fn from(value: &'a Alias) -> Self {
        Checkable::Alias(value)
    }
}

impl<'a> From<&'a Expr> for Checkable<'a> {
    fn from(value: &'a Expr) -> Self {
        Checkable::Expr(value)
    }
}

/// WPS111
pub(crate) fn too_short_name<'a, 'b>(checker: &'a mut Checker, node: impl Into<Checkable<'b>>) {
    if let Some((name, range)) = match node.into() {
        Checkable::Identifier(identifier)
        | Checkable::Parameter(Parameter {
            name: identifier, ..
        })
        | Checkable::Alias(Alias {
            asname: Some(identifier),
            ..
        })
        | Checkable::Expr(Expr::Attribute(ExprAttribute {
            attr: identifier, ..
        })) => Some((identifier.as_str(), identifier.range())),
        Checkable::Expr(Expr::Name(name)) => Some((name.id.as_str(), name.range())),
        _ => None,
    } {
        if naming::is_too_short_name(
            name,
            checker.settings.wemake_python_styleguide.min_name_length,
            true,
        ) {
            checker.diagnostics.push(Diagnostic::new(
                TooShortName {
                    name: name.to_string(),
                    is_module: false,
                },
                range,
            ));
        }
    }
}

/// WPS111 (for filesystem)
pub(crate) fn too_short_module_name(
    path: &Path,
    package: Option<&Path>,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    if !matches!(
        path.extension().and_then(std::ffi::OsStr::to_str),
        Some("py" | "pyi")
    ) {
        return None;
    }

    if let Some(package) = package {
        let module_name = if naming::is_module_file(path) {
            package.file_name().unwrap().to_string_lossy()
        } else {
            path.file_stem().unwrap().to_string_lossy()
        };
        if naming::is_too_short_name(
            &module_name,
            settings.wemake_python_styleguide.min_name_length,
            true,
        ) {
            return Some(Diagnostic::new(
                TooShortName {
                    name: module_name.to_string(),
                    is_module: true,
                },
                TextRange::default(),
            ));
        };
    }

    None
}
