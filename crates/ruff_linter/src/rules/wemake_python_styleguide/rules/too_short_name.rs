use std::path::Path;

use ruff_python_ast::{Alias, Expr, ExprAttribute, ExprName, Identifier, Parameter};
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

pub(crate) struct Checkable(Identifier);

impl From<&Identifier> for Checkable {
    fn from(value: &Identifier) -> Self {
        Self(value.clone())
    }
}

impl From<&Parameter> for Checkable {
    fn from(value: &Parameter) -> Self {
        (&value.name).into()
    }
}

impl TryFrom<&Alias> for Checkable {
    type Error = ();

    fn try_from(value: &Alias) -> Result<Self, Self::Error> {
        match value {
            Alias {
                asname: Some(identifier),
                ..
            } => Ok(identifier.into()),
            _ => Err(()),
        }
    }
}

impl TryFrom<&Expr> for Checkable {
    type Error = ();

    fn try_from(value: &Expr) -> Result<Self, Self::Error> {
        match value {
            Expr::Name(ExprName { id, range, .. }) => Ok(Self(Identifier::new(id, *range))),
            Expr::Attribute(ExprAttribute { attr, .. }) => Ok(attr.into()),
            _ => Err(()),
        }
    }
}

impl TryFrom<&Box<Expr>> for Checkable {
    type Error = ();

    fn try_from(value: &Box<Expr>) -> Result<Self, Self::Error> {
        (&(**value)).try_into()
    }
}

/// WPS111
pub(crate) fn too_short_name(checker: &mut Checker, node: impl TryInto<Checkable>) {
    if let Ok(Checkable(identifier)) = node.try_into() {
        if naming::is_too_short_name(
            identifier.as_str(),
            checker.settings.wemake_python_styleguide.min_name_length,
            true,
        ) {
            checker.diagnostics.push(Diagnostic::new(
                TooShortName {
                    name: identifier.to_string(),
                    is_module: false,
                },
                identifier.range(),
            ));
        }
    };
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
