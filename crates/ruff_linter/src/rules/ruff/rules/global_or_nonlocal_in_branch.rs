use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprContext, Stmt, visitor};
use ruff_python_semantic::cfg::graph::{BlockId, ControlFlowGraph, build_cfg};
use ruff_text_size::Ranged;
use std::collections::{HashSet, VecDeque};
use std::fmt::Display;

use crate::{Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for the use of global or nonlocal variables in branches of a function.
///
/// ## Why is this bad?
/// If the variable is declared in one branch and used in another, it may be unintuitive.
///
/// ## Example
///
/// ```python
/// def foo():
///     if True:
///         global x
///         x = 1
///     else:
///         print(x)
/// ```
///
/// Use instead:
///
/// ```python
/// def foo():
///     global x
///     if True:
///         x = 1
///     else:
///         x = 0
///     print(x)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct GlobalOrNonlocalInBranch {
    pub name: String,
    pub declaration_type: String,
}

impl Violation for GlobalOrNonlocalInBranch {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The variable {} is declared using {}, \
            but is used in other branches of this function — this may be unintuitive",
            self.name, self.declaration_type
        )
    }
}

#[derive(Clone, Copy)]
enum DeclarationType {
    Global,
    Nonlocal,
}

impl Display for DeclarationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclarationType::Global => write!(f, "global"),
            DeclarationType::Nonlocal => write!(f, "nonlocal"),
        }
    }
}

pub(crate) fn global_in_branch(checker: &mut Checker, name: &str) {
    global_or_nonlocal_in_branch(checker, name, DeclarationType::Global);
}

pub(crate) fn nonlocal_in_branch(checker: &mut Checker, name: &str) {
    global_or_nonlocal_in_branch(checker, name, DeclarationType::Nonlocal);
}

fn global_or_nonlocal_in_branch(
    checker: &mut Checker,
    name: &str,
    declaration_type: DeclarationType,
) {
    let semantic = checker.semantic();

    let current_scope = semantic.current_scope();
    let ruff_python_semantic::ScopeKind::Function(ast::StmtFunctionDef {
        body: function_body,
        ..
    }) = &current_scope.kind
    else {
        return;
    };
    let cfg = build_cfg(function_body);

    let mut declaration_blocks = HashSet::new();
    let mut usage_blocks = HashSet::new();

    for block_id in (0..cfg.num_blocks()).map(BlockId::from_usize) {
        let stmts = cfg.stmts(block_id);

        for stmt in stmts {
            match declaration_type {
                DeclarationType::Global => {
                    if let Stmt::Global(ast::StmtGlobal { names, .. }) = stmt {
                        if names.iter().any(|n| n.id.as_str() == name) {
                            declaration_blocks.insert(block_id);
                        }
                    }
                }
                DeclarationType::Nonlocal => {
                    if let Stmt::Nonlocal(ast::StmtNonlocal { names, .. }) = stmt {
                        if names.iter().any(|n| n.id.as_str() == name) {
                            declaration_blocks.insert(block_id);
                        }
                    }
                }
            }

            let mut visitor = AssignmentVisitor::new(name);
            visitor.visit_stmt(stmt);
            if !visitor.assignments.is_empty() {
                usage_blocks.insert(block_id);
            }
        }
    }

    for usage_block in usage_blocks {
        if declaration_blocks.contains(&usage_block) {
            continue;
        }

        if has_path_without_declaration(&cfg, cfg.initial(), usage_block, &declaration_blocks) {
            let stmts = cfg.stmts(usage_block);
            for stmt in stmts {
                let mut visitor = AssignmentVisitor::new(name);
                visitor.visit_stmt(stmt);
                for assignment in visitor.assignments {
                    checker.report_diagnostic(
                        GlobalOrNonlocalInBranch {
                            name: name.to_string(),
                            declaration_type: declaration_type.to_string(),
                        },
                        assignment.range(),
                    );
                }
            }
        }
    }
}

fn has_path_without_declaration(
    cfg: &ControlFlowGraph,
    start: BlockId,
    target: BlockId,
    avoid_blocks: &HashSet<BlockId>,
) -> bool {
    if start == target {
        return !avoid_blocks.contains(&start);
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }

        if avoid_blocks.contains(&current) && current != start {
            continue;
        }

        if current == target {
            return true;
        }

        for next_block in cfg.outgoing(current).targets() {
            if !visited.contains(&next_block) {
                queue.push_back(next_block);
            }
        }
    }

    false
}

struct AssignmentVisitor<'a> {
    variable_name: &'a str,
    assignments: Vec<&'a Expr>,
}

impl<'a> AssignmentVisitor<'a> {
    fn new(variable_name: &'a str) -> Self {
        Self {
            variable_name,
            assignments: Vec::new(),
        }
    }
}

impl<'a> Visitor<'a> for AssignmentVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName {
                        id,
                        ctx: ExprContext::Store,
                        ..
                    }) = target
                    {
                        if id.as_str() == self.variable_name {
                            self.assignments.push(target);
                        }
                    }
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName {
                    id,
                    ctx: ExprContext::Store,
                    ..
                }) = target.as_ref()
                {
                    if id.as_str() == self.variable_name {
                        self.assignments.push(target.as_ref());
                    }
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}
