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
/// By default, `tarfile` treats an empty block as the end of the archive.
/// With `ignore_zeros=True`, it instead skips empty *and invalid* blocks and
/// keeps reading for as long as it can. The Python documentation describes
/// this mode as "only useful for reading concatenated or damaged archives".
///
/// Outside of that use case, the permissive mode makes the archive's contents
/// depend on how Python recovers from malformed data. An attacker can craft an
/// archive that Python reads differently from other tools or a stricter
/// parser, so that the members you inspect are not the members you extract.
/// The flag is often enabled to tolerate a missing end-of-archive marker
/// without realizing that it also relaxes every other structural check.
///
/// ## Example
/// ```python
/// import tarfile
///
/// with tarfile.open("archive.tar", ignore_zeros=True) as tar:
///     tar.extractall(path="output", filter="data")
/// ```
///
/// Use instead:
/// ```python
/// import tarfile
///
/// with tarfile.open("archive.tar") as tar:
///     tar.extractall(path="output", filter="data")
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

    // Every opener takes `ignore_zeros` as a keyword-only argument, but the
    // `TarFile` constructor also accepts it as its seventh positional
    // parameter: `TarFile(name, mode, fileobj, format, tarinfo, dereference,
    // ignore_zeros, ...)`.
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
