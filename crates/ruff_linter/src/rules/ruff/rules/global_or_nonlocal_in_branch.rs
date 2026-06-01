use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{BindingKind, NodeId, Scope, ScopeId};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `global` and `nonlocal` declarations placed inside a conditional
/// branch (such as an `if`, `match`, or `try` body) when the declared name is
/// also used on a code path that does not pass through the declaration.
///
/// ## Why is this bad?
/// A `global` or `nonlocal` statement applies to the entire enclosing function,
/// not just the branch it appears in. Placing it inside one branch while using
/// the name on a path that may skip that branch is misleading: it looks like the
/// declaration only affects that branch, when in fact the other path's assignment
/// also targets the global (or enclosing) variable.
///
/// ## Known problems
/// Detection uses Ruff's branch analysis, which models `if`, `match`, and `try`
/// statements but not loops. A `global` or `nonlocal` inside a `for` or `while`
/// body is therefore not flagged, even though it has the same function-wide effect.
///
/// ## Example
/// ```python
/// counter = 0
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
/// def update(flag):
///     global counter
///     if flag:
///         counter = 1
///     else:
///         counter = 2
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct GlobalOrNonlocalInBranch {
    name: String,
    keyword: &'static str,
}

impl Violation for GlobalOrNonlocalInBranch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let GlobalOrNonlocalInBranch { name, keyword } = self;
        format!(
            "`{name}` is declared `{keyword}` in a branch but used in another branch of this function"
        )
    }
}

/// RUF076
pub(crate) fn global_or_nonlocal_in_branch(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    let semantic = checker.semantic();

    for (name, binding_id) in scope.bindings() {
        let mut declaration: Option<Declaration> = None;
        let mut occurrences: Vec<NodeId> = Vec::new();

        // `dominates` only compares nodes in one scope; references can resolve into
        // nested scopes, so keep only this scope's (the chain is already scope-local).
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

        // Misleading when the declaration doesn't dominate a use (#6470).
        let misleading = occurrences
            .iter()
            .any(|&use_node| !semantic.dominates(declaration.statement, use_node));

        if misleading {
            checker.report_diagnostic(
                GlobalOrNonlocalInBranch {
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
