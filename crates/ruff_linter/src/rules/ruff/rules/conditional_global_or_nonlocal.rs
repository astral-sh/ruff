use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_semantic::{BindingKind, NodeId, Scope, ScopeId};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

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
/// variable.
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
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
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

/// RUF077
pub(crate) fn conditional_global_or_nonlocal(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    let semantic = checker.semantic();

    for (name, binding_id) in scope.bindings() {
        let mut declaration: Option<Declaration> = None;
        let mut occurrences: Vec<NodeId> = Vec::new();

        // References can resolve into nested scopes; keep only this scope's.
        for id in scope.shadowed_bindings(binding_id) {
            let binding = &semantic.bindings[id];
            let keyword = match binding.kind {
                BindingKind::Global(_) => Some("global"),
                BindingKind::Nonlocal(_, _) => Some("nonlocal"),
                _ => None,
            };
            if let Some(keyword) = keyword
                && let Some(statement) = binding.source
            {
                declaration = Some(Declaration {
                    statement,
                    name_range: binding.range(),
                    keyword,
                });
            } else {
                occurrences.extend(binding.source);
            }
            for reference_id in binding.references() {
                let reference = semantic.reference(reference_id);
                if reference.scope_id() == scope_id
                    && let Some(node_id) = reference.expression_id()
                {
                    occurrences.push(node_id);
                }
            }
        }

        let Some(declaration) = declaration else {
            continue;
        };

        // `dominates` doesn't model loops; a loop may run zero times, so a use
        // outside an enclosing loop is reachable without a declaration inside it.
        // (Searching past the enclosing function would cross into another scope.)
        let loop_range = semantic
            .statements(declaration.statement)
            .take_while(|statement| !statement.is_function_def_stmt())
            .find_map(|statement| {
                matches!(statement, Stmt::For(_) | Stmt::While(_)).then_some(statement.range())
            });

        let misleading = occurrences.iter().any(|&use_node| {
            !semantic.dominates(declaration.statement, use_node)
                || loop_range.is_some_and(|range| {
                    !range.contains_range(semantic.statement(use_node).range())
                })
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

struct Declaration {
    /// The `global`/`nonlocal` statement node, used for the dominance check.
    statement: NodeId,
    /// The declared name's range, used as the diagnostic range.
    name_range: TextRange,
    keyword: &'static str,
}
