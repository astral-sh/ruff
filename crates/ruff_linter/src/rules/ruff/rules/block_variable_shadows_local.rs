use std::fmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{Binding, BindingKind, Scope, ScopeId, SemanticModel};
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
/// A loop variable is also not flagged when no code after the loop, including the loop's `else`
/// clause, reads the name, since nothing can then observe that the earlier value was overwritten. This covers, for example, reusing the
/// name of a temporary from the body of an earlier loop (e.g., `thread = Thread(...)` in one loop
/// followed by `for thread in threads`). A loop that iterates over the earlier value while
/// overwriting it (e.g., `for name, xs in zip(names, xs)`) is still flagged.
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
}

impl Violation for BlockVariableShadowsLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlockVariableShadowsLocal { name, kind } = self;
        format!("{kind} `{name}` shadows a local variable")
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

            // Skip the block variable unless every path to it passes through the shadowed
            // assignment. Bindings in different branches of an `if`, `match`, or `try` statement
            // never hold a value at the same time:
            //
            // ```python
            // if condition:
            //     defects = []
            // else:
            //     for defects in batches: ...
            // ```
            //
            // But a block nested in a branch that the assignment precedes does overwrite it:
            //
            // ```python
            // defects = []
            // if condition:
            //     for defects in batches: ...
            // ```
            if shadowed.source.is_none_or(|shadowed_source| {
                binding
                    .source
                    .is_none_or(|source| !semantic.dominates(shadowed_source, source))
            }) {
                continue;
            }

            if kind == BlockVariableKind::LoopVariable
                && overwritten_value_is_unobservable(semantic, shadowed, binding)
            {
                continue;
            }

            let mut diagnostic = checker.report_diagnostic(
                BlockVariableShadowsLocal {
                    name: name.to_string(),
                    kind,
                },
                binding.range(),
            );
            diagnostic
                .secondary_annotation(format_args!("`{name}` previously assigned here"), shadowed);
            diagnostic.set_primary_annotation_message(format_args!("`{name}` overwritten here"));
        }
    }
}

/// Returns `true` if overwriting the earlier value with the loop variable can't affect any code.
///
/// That's the case when no code after the loop body reads the name (it would see the loop's last
/// value instead of the earlier one) and the loop's header doesn't read the earlier value. Any other read
/// of the earlier value has already happened by the time the loop starts: the semantic model
/// resolves each read to the binding that's visible at that point, so reads inside or after the
/// loop resolve to the loop variable instead.
///
/// For example, none of these loop variables are flagged:
///
/// ```python
/// for _ in range(5):
///     thread = Thread(target=work)
///     threads.append(thread)
/// for thread in threads:
///     thread.join()
///
/// fig, ax = plt.subplots()
/// ax.plot(xs, ys)
/// for ax in axes:
///     ax.grid()
///
/// result = []
/// for result in stream():
///     ...
/// ```
fn overwritten_value_is_unobservable(
    semantic: &SemanticModel,
    shadowed: &Binding,
    loop_binding: &Binding,
) -> bool {
    let Some(Stmt::For(loop_statement)) = loop_binding.statement(semantic) else {
        return false;
    };
    let loop_range = loop_statement.range();

    // The loop's `else` clause runs after the last iteration, so a read there sees the loop's
    // last value just like a read after the loop does:
    //
    // ```python
    // for defects in snippets: ...
    // else:
    //     save(defects)
    // ```
    let body_end = loop_statement
        .body
        .last()
        .map_or(loop_range.end(), Ranged::end);
    let read_after_loop = loop_binding
        .references()
        .any(|reference_id| semantic.reference(reference_id).start() >= body_end);

    // E.g. `for name, x in zip(names, x)`.
    let read_by_loop_header = shadowed
        .references()
        .any(|reference_id| loop_range.contains_range(semantic.reference(reference_id).range()));

    !read_after_loop && !read_by_loop_header
}
