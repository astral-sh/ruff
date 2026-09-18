use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::{self as ast, ArgOrKeyword};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `tarfile` archives opened with `ignore_zeros=True`.
///
/// ## Why is this bad?
/// By default, `tarfile` stops at the first empty block. `ignore_zeros=True`
/// makes it skip empty *and invalid* blocks and read on, a mode the Python
/// documentation recommends only for concatenated or damaged archives.
///
/// It is usually enabled to tolerate a missing end-of-archive marker, but it
/// relaxes every other structural check too. The archive's contents then
/// depend on how Python recovers from malformed data, so a crafted archive can
/// yield different members here than under a stricter parser.
///
/// ## Example
/// ```python
/// import tarfile
///
/// tar = tarfile.open("archive.tar", ignore_zeros=True)
/// ```
///
/// Use instead:
/// ```python
/// import tarfile
///
/// tar = tarfile.open("archive.tar")
/// ```
///
/// ## References
/// - [Python documentation: `tarfile.TarFile`](https://docs.python.org/3/library/tarfile.html#tarfile.TarFile)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Security)]
pub(crate) struct TarfileIgnoreZerosTrue {
    is_exact: bool,
}

impl Violation for TarfileIgnoreZerosTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_exact {
            "`tarfile` opened with `ignore_zeros=True`".to_string()
        } else {
            "`tarfile` opened with truthy `ignore_zeros`".to_string()
        }
    }
}

/// S203
pub(crate) fn tarfile_ignore_zeros_true(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::TARFILE) {
        return;
    }

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    // Keyword-only on the openers, but the `TarFile` constructor also takes it
    // positionally, after `name, mode, fileobj, format, tarinfo, dereference`.
    let argument = match qualified_name.segments() {
        ["tarfile", "TarFile"] => call.arguments.find_argument("ignore_zeros", 6),
        ["tarfile", "open"]
        | [
            "tarfile",
            "TarFile",
            "open" | "taropen" | "gzopen" | "bz2open" | "xzopen" | "zstopen",
        ] => call
            .arguments
            .find_keyword("ignore_zeros")
            .map(ArgOrKeyword::from),
        _ => None,
    };

    let Some(argument) = argument else {
        return;
    };

    let truthiness = Truthiness::from_expr(argument.value(), |id| {
        checker.semantic().has_builtin_binding(id)
    });

    if !matches!(truthiness, Truthiness::True | Truthiness::Truthy) {
        return;
    }

    checker.report_diagnostic(
        TarfileIgnoreZerosTrue {
            is_exact: matches!(truthiness, Truthiness::True),
        },
        argument.range(),
    );
}
