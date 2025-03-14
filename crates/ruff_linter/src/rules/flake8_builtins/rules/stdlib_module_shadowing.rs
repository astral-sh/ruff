use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{PySourceType, PythonVersion};
use ruff_python_stdlib::path::is_module_file;
use ruff_python_stdlib::sys::is_known_standard_library;
use ruff_text_size::TextRange;

use crate::settings::LinterSettings;

/// ## What it does
/// Checks for modules that use the same names as Python standard-library
/// modules.
///
/// ## Why is this bad?
/// Reusing a standard-library module name for the name of a module increases
/// the difficulty of reading and maintaining the code, and can cause
/// non-obvious errors. Readers may mistake the first-party module for the
/// standard-library module and vice versa.
///
/// Standard-library modules can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.allowed-modules`] configuration option.
///
/// By default, the module path relative to the project root or [`src`] directories is considered,
/// so a top-level `logging.py` or `logging/__init__.py` will clash with the builtin `logging`
/// module, but `utils/logging.py`, for example, will not. With the
/// [`lint.flake8-builtins.strict-checking`] option set to `true`, only the last component
/// of the module name is considered, so `logging.py`, `utils/logging.py`, and
/// `utils/logging/__init__.py` will all trigger the rule.
///
/// This rule is not applied to stub files, as the name of a stub module is out
/// of the control of the author of the stub file. Instead, a stub should aim to
/// faithfully emulate the runtime module it is stubbing.
///
/// As of Python 3.13, errors from modules that use the same name as
/// standard-library modules now display a custom message.
///
/// ## Example
///
/// ```console
/// $ touch random.py
/// $ python3 -c 'from random import choice'
/// Traceback (most recent call last):
///   File "<string>", line 1, in <module>
///     from random import choice
/// ImportError: cannot import name 'choice' from 'random' (consider renaming '/random.py' since it has the same name as the standard library module named 'random' and prevents importing that standard library module)
/// ```
///
/// ## Options
/// - `lint.flake8-builtins.allowed-modules`
/// - `lint.flake8-builtins.strict-checking`
#[derive(ViolationMetadata)]
pub(crate) struct StdlibModuleShadowing {
    name: String,
}

impl Violation for StdlibModuleShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StdlibModuleShadowing { name } = self;
        format!("Module `{name}` shadows a Python standard-library module")
    }
}

/// A005
pub(crate) fn stdlib_module_shadowing(
    mut path: &Path,
    settings: &LinterSettings,
    target_version: PythonVersion,
) -> Option<Diagnostic> {
    if !PySourceType::try_from_path(path).is_some_and(PySourceType::is_py_file) {
        return None;
    }

    // strip src and root prefixes before converting to a fully-qualified module path
    let prefix = get_prefix(settings, path);
    if let Some(Ok(new_path)) = prefix.map(|p| path.strip_prefix(p)) {
        path = new_path;
    }

    // for modules like `modname/__init__.py`, use the parent directory name, otherwise just trim
    // the `.py` extension
    let path = if is_module_file(path) {
        Cow::from(path.parent()?)
    } else {
        Cow::from(path.with_extension(""))
    };

    // convert a filesystem path like `foobar/collections/abc` to a reversed sequence of modules
    // like `["abc", "collections", "foobar"]`, stripping anything that's not a normal component
    let mut components = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .map(|c| c.as_os_str().to_string_lossy())
        .rev();

    let module_name = components.next()?;

    if is_allowed_module(settings, target_version, &module_name) {
        return None;
    }

    // not allowed generally, but check for a parent in non-strict mode
    if !settings.flake8_builtins.strict_checking && components.next().is_some() {
        return None;
    }

    Some(Diagnostic::new(
        StdlibModuleShadowing {
            name: module_name.to_string(),
        },
        TextRange::default(),
    ))
}

/// Return the longest prefix of `path` between `settings.src` and `settings.project_root`.
fn get_prefix<'a>(settings: &'a LinterSettings, path: &Path) -> Option<&'a PathBuf> {
    let mut prefix = None;
    for dir in settings.src.iter().chain([&settings.project_root]) {
        if path.starts_with(dir) && prefix.is_none_or(|existing| existing < dir) {
            prefix = Some(dir);
        }
    }
    prefix
}

fn is_allowed_module(settings: &LinterSettings, version: PythonVersion, module: &str) -> bool {
    // Shadowing private stdlib modules is okay.
    // https://github.com/astral-sh/ruff/issues/12949
    if module.starts_with('_') && !module.starts_with("__") {
        return true;
    }

    if settings
        .flake8_builtins
        .allowed_modules
        .iter()
        .any(|allowed_module| allowed_module == module)
    {
        return true;
    }

    !is_known_standard_library(version.minor, module)
}
