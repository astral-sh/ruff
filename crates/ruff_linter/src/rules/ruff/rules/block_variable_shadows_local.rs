use std::fmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{BindingKind, Scope, ScopeId};
use ruff_source_file::SourceRow;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `for` loop variables, `with` statement targets, and `except` handler names that
/// reuse the name of a local variable assigned earlier in the same scope.
///
/// ## Why is this bad?
/// Binding a name in a `for`, `with`, or `except` statement silently overwrites any value the
/// name already held. Code after the block that expects the original value instead sees
/// whatever the block left behind: the last item of the loop, the context manager's result, or,
/// for an `except` handler, no value at all, since Python deletes the exception name when the
/// handler exits.
///
/// This is usually an accidental name collision, and it can be hard to spot when the block
/// unpacks several names at once:
///
/// ```python
/// defects = load_defects()
///
/// for path, defects in snippets.items():
///     report(path, defects)
///
/// save(defects)  # Saves the last snippet's defects, not the result of `load_defects()`.
/// ```
///
/// Rename one of the variables so that each name refers to a single value.
///
/// Reassigning a local variable with another plain assignment (e.g., `x = x + 1`) is not
/// flagged. Neither is reusing the same loop variable name in consecutive loops, or shadowing a
/// bare annotation (e.g., `x: int`) that declares the type of a loop variable.
///
/// This rule is based on `WPS440` (`BlockAndLocalOverlapViolation`) from
/// `wemake-python-styleguide`.
///
/// ## Example
/// ```python
/// defects = load_defects()
///
/// for path, defects in snippets.items():
///     report(path, defects)
///
/// save(defects)
/// ```
///
/// Use instead:
/// ```python
/// defects = load_defects()
///
/// for path, snippet_defects in snippets.items():
///     report(path, snippet_defects)
///
/// save(defects)
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
///
/// ## Related rules
/// - [`import-shadowed-by-loop-var`][F402]: an import shadowed by a loop variable.
/// - [`redefined-argument-from-local`][PLR1704]: a function parameter shadowed by a `for`,
///   `with`, or `except` binding.
/// - [`redefined-loop-name`][PLW2901]: a loop variable overwritten inside the loop body.
///
/// [F402]: https://docs.astral.sh/ruff/rules/import-shadowed-by-loop-var/
/// [PLR1704]: https://docs.astral.sh/ruff/rules/redefined-argument-from-local/
/// [PLW2901]: https://docs.astral.sh/ruff/rules/redefined-loop-name/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct BlockVariableShadowsLocal {
    name: String,
    kind: BlockVariableKind,
    row: SourceRow,
}

impl Violation for BlockVariableShadowsLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlockVariableShadowsLocal { name, kind, row } = self;
        format!("{kind} `{name}` shadows local variable assigned at {row}")
    }

    fn fix_title(&self) -> Option<String> {
        let BlockVariableShadowsLocal { kind, .. } = self;
        Some(format!("Rename the {} or the local variable", kind.noun()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockVariableKind {
    LoopVariable,
    WithTarget,
    ExceptionName,
}

impl BlockVariableKind {
    fn from_binding_kind(kind: &BindingKind) -> Option<Self> {
        match kind {
            BindingKind::LoopVar => Some(Self::LoopVariable),
            BindingKind::WithItemVar => Some(Self::WithTarget),
            BindingKind::BoundException => Some(Self::ExceptionName),
            _ => None,
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::LoopVariable => "loop variable",
            Self::WithTarget => "`with` target",
            Self::ExceptionName => "exception name",
        }
    }
}

impl fmt::Display for BlockVariableKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LoopVariable => "Loop variable",
            Self::WithTarget => "`with` target",
            Self::ExceptionName => "Exception name",
        })
    }
}

/// RUF079
pub(crate) fn block_variable_shadows_local(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    let semantic = checker.semantic();

    for (name, binding_id) in scope.bindings() {
        if checker.settings().dummy_variable_rgx.is_match(name) {
            continue;
        }

        for shadow in semantic.shadowed_bindings(scope_id, binding_id) {
            // A nested function's variable doesn't overwrite the enclosing function's variable.
            if !shadow.same_scope() {
                continue;
            }

            let binding = &semantic.bindings[shadow.binding_id()];
            let Some(kind) = BlockVariableKind::from_binding_kind(&binding.kind) else {
                continue;
            };

            // Only flag shadowed values assigned by ordinary statements. Function parameters are
            // covered by `redefined-argument-from-local` (PLR1704) and imports by
            // `import-shadowed-by-loop-var` (F402). Bare annotations (`x: int`) hold no value,
            // and other block variables (e.g., the same name reused by consecutive loops) are
            // intentionally allowed.
            let shadowed = &semantic.bindings[shadow.shadowed_id()];
            if !matches!(
                shadowed.kind,
                BindingKind::Assignment | BindingKind::NamedExprAssignment
            ) {
                continue;
            }

            // Bindings in different branches of an `if`, `match`, or `try` statement never hold a
            // value at the same time, e.g.:
            //
            // ```python
            // if condition:
            //     defects = []
            // else:
            //     for defects in batches: ...
            // ```
            if shadowed.source.is_none_or(|left| {
                binding
                    .source
                    .is_none_or(|right| !semantic.same_branch(left, right))
            }) {
                continue;
            }

            checker.report_diagnostic(
                BlockVariableShadowsLocal {
                    name: name.to_string(),
                    kind,
                    row: checker.compute_source_row(shadowed.start()),
                },
                binding.range(),
            );
        }
    }
}
