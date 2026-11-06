use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{Binding, BindingKind, NodeId, Scope, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `global` and `nonlocal` declarations placed inside a block that
/// may not execute (such as an `if`, `match`, or `try` branch, or a `for`/`while`
/// loop body) when the declared name is also used on a code path that does not
/// pass through the declaration.
///
/// ## Why is this bad?
/// A `global` or `nonlocal` statement applies to the entire enclosing function,
/// not just the block it appears in. Placing it inside a block that may be
/// skipped, while using the name on a path that does not pass through that block,
/// is misleading: it looks like the declaration only affects that block, when in
/// fact the other path's assignment also targets the global or enclosing
/// variable. The same applies to a nested function or comprehension that reads
/// the name: it sees the global or enclosing variable because of the declaration.
///
/// ## Example
/// ```python
/// counter = 0
///
///
/// def update(flag):
///     if flag:
///         global counter
///         counter = 1
///     else:
///         counter = 2  # also rebinds the global `counter`
/// ```
///
/// Use instead:
///
/// ```python
/// counter = 0
///
///
/// def update(flag):
///     global counter
///     if flag:
///         counter = 1
///     else:
///         counter = 2
/// ```
///
/// ## References
/// - [Python documentation: The `global` statement](https://docs.python.org/3/reference/simple_stmts.html#the-global-statement)
/// - [Python documentation: The `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#the-nonlocal-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct ConditionalGlobalOrNonlocal {
    name: String,
    keyword: &'static str,
}

impl Violation for ConditionalGlobalOrNonlocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ConditionalGlobalOrNonlocal { name, keyword } = self;
        format!(
            "`{name}` is declared `{keyword}` in a block that may not run on every path, but the declaration applies to the entire function"
        )
    }
}

/// conditional-global-or-nonlocal
pub(crate) fn conditional_global_or_nonlocal(checker: &Checker, scope: &Scope) {
    let semantic = checker.semantic();

    for (name, binding_id) in scope.bindings() {
        // `shadowed_bindings` walks from the newest binding backwards, so `last` is the first
        // declaration in source order.
        let Some(declaration) = scope
            .shadowed_bindings(binding_id)
            .filter_map(|id| Declaration::from_binding(&semantic.bindings[id]))
            .last()
        else {
            continue;
        };

        // A `with` body always runs, but the semantic model records it as a branch because a
        // context manager may suppress an exception raised inside it. For the purpose of this
        // rule, a declaration inside a `with` body is as good as one placed before the `with`.
        let mut dominator = declaration.statement;
        while let Some(parent) = semantic.parent_statement_id(dominator)
            && semantic.statement(parent).is_with_stmt()
        {
            dominator = parent;
        }

        // `dominates` doesn't model loops; a loop may run zero times, so a use outside an
        // enclosing loop is reachable without a declaration inside it.
        let loop_range = semantic
            .statements(declaration.statement)
            .find_map(|statement| {
                matches!(statement, Stmt::For(_) | Stmt::While(_)).then_some(statement.range())
            });

        let is_misleading = |use_node: NodeId| {
            !semantic.dominates(dominator, use_node)
                || loop_range.is_some_and(|range| {
                    !range.contains_range(semantic.statement(use_node).range())
                })
        };

        // Uses are the other bindings of the name in this scope (assignments, deletions, and so
        // on) plus every reference that resolved to one of its bindings, including references
        // from nested scopes, which read the global or enclosing variable because of the
        // declaration.
        let misleading = scope.shadowed_bindings(binding_id).any(|id| {
            let binding = &semantic.bindings[id];
            let source = binding
                .source
                .filter(|_| Declaration::from_binding(binding).is_none());
            source
                .into_iter()
                .chain(references(semantic, binding))
                .any(is_misleading)
        });

        if misleading {
            checker.report_diagnostic(
                ConditionalGlobalOrNonlocal {
                    name: name.to_string(),
                    keyword: declaration.keyword,
                },
                declaration.name_range,
            );
        }
    }
}

/// Returns the expression nodes of the references that resolved to `binding`.
fn references<'a>(
    semantic: &'a SemanticModel,
    binding: &'a Binding,
) -> impl Iterator<Item = NodeId> + 'a {
    binding
        .references()
        .filter_map(|reference_id| semantic.reference(reference_id).expression_id())
}

struct Declaration {
    /// The `global`/`nonlocal` statement node, used for the dominance check.
    statement: NodeId,
    /// The declared name's range, used as the diagnostic range.
    name_range: TextRange,
    keyword: &'static str,
}

impl Declaration {
    fn from_binding(binding: &Binding) -> Option<Self> {
        let keyword = match binding.kind {
            BindingKind::Global(_) => "global",
            BindingKind::Nonlocal(_, _) => "nonlocal",
            _ => return None,
        };
        Some(Self {
            statement: binding.source?,
            name_range: binding.range(),
            keyword,
        })
    }
}
