use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_stdlib::identifiers::is_mangled_private;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for private class members (methods and class variables) that are
/// defined but never used.
///
/// ## Why is this bad?
/// Unused private members add unnecessary complexity to the codebase and may
/// indicate dead code or a mistake in the implementation. They should either
/// be used, removed, or made public if intended for external use.
///
/// A member is considered private if its name starts with double underscores
/// (`__`) but does not end with double underscores (which would make it a
/// "dunder" or magic method).
///
/// ## Example
/// ```python
/// class MyClass:
///     __unused_var = 42
///
///     def __unused_method(self):
///         pass
///
///     def public_method(self):
///         pass
/// ```
///
/// Use instead:
/// ```python
/// class MyClass:
///     def public_method(self):
///         pass
/// ```
///
/// Or, if the member is intentionally unused, consider removing the double
/// underscore prefix.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct UnusedPrivateMember {
    class_name: String,
    member_name: String,
}

impl Violation for UnusedPrivateMember {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedPrivateMember {
            class_name,
            member_name,
        } = self;
        format!("Unused private member `{class_name}.{member_name}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove unused private member".to_string())
    }
}

struct PrivateMember<'a> {
    name: &'a str,
    range: TextRange,
}

/// Single-pass visitor that collects both definitions and accesses.
struct PrivateMemberVisitor<'a> {
    class_name: &'a str,
    definitions: Vec<PrivateMember<'a>>,
    accessed: FxHashSet<&'a str>,
    /// Aliases for `self` and `cls` (e.g., `s = self` means `s` is an alias)
    self_aliases: FxHashSet<&'a str>,
}

impl<'a> PrivateMemberVisitor<'a> {
    fn new(class_name: &'a str) -> Self {
        Self {
            class_name,
            definitions: Vec::new(),
            accessed: FxHashSet::default(),
            self_aliases: FxHashSet::default(),
        }
    }

    fn collect_definition(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. })
                if is_mangled_private(name.as_str()) =>
            {
                self.definitions.push(PrivateMember {
                    name: name.as_str(),
                    range: name.range(),
                });
            }
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, range, .. }) = target
                        && is_mangled_private(id.as_str())
                    {
                        self.definitions.push(PrivateMember {
                            name: id.as_str(),
                            range: *range,
                        });
                    }
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, range, .. }) = target.as_ref()
                    && is_mangled_private(id.as_str())
                {
                    self.definitions.push(PrivateMember {
                        name: id.as_str(),
                        range: *range,
                    });
                }
            }
            _ => {}
        }
    }

    /// Check if a name is `self`, `cls`, the class name, or an alias of `self`/`cls`.
    fn is_self_or_cls(&self, name: &str) -> bool {
        name == "self"
            || name == "cls"
            || name == self.class_name
            || self.self_aliases.contains(name)
    }

    /// Track assignments like `s = self` or `that = cls` to build aliases.
    fn track_self_alias(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(ast::StmtAssign { targets, value, .. }) = stmt {
            // Only handle simple assignments like `s = self`
            if let Expr::Name(ast::ExprName { id: value_name, .. }) = value.as_ref() {
                if value_name.as_str() == "self" || value_name.as_str() == "cls" {
                    for target in targets {
                        if let Expr::Name(ast::ExprName {
                            id: target_name, ..
                        }) = target
                        {
                            self.self_aliases.insert(target_name.as_str());
                        }
                    }
                }
            }
        }
    }
}

impl<'a> Visitor<'a> for PrivateMemberVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, ctx, .. })
                if ctx.is_load() && is_mangled_private(id.as_str()) =>
            {
                self.accessed.insert(id.as_str());
            }
            Expr::Attribute(ast::ExprAttribute {
                value, attr, ctx, ..
            }) if ctx.is_load() => {
                if let Expr::Name(ast::ExprName {
                    id, ctx: name_ctx, ..
                }) = value.as_ref()
                {
                    // Track `self.__x`, `cls.__x`, `ClassName.__x`, and aliased access
                    if is_mangled_private(attr.as_str()) && self.is_self_or_cls(id.as_str()) {
                        self.accessed.insert(attr.as_str());
                    }
                    // Track decorator patterns like `@__prop.setter`
                    if name_ctx.is_load() && is_mangled_private(id.as_str()) {
                        self.accessed.insert(id.as_str());
                    }
                }
                // Track `type(self).__x`, `super().__x`, and `super(ClassName, self).__x` patterns
                if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref()
                    && let Expr::Name(ast::ExprName { id, .. }) = func.as_ref()
                    && (id.as_str() == "type" || id.as_str() == "super")
                    && is_mangled_private(attr.as_str())
                {
                    self.accessed.insert(attr.as_str());
                }
                visitor::walk_expr(self, value);
            }
            _ => {
                visitor::walk_expr(self, expr);
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        // Track self/cls aliases before visiting the statement
        self.track_self_alias(stmt);

        match stmt {
            Stmt::ClassDef(_) => {}
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }
}

/// PLW0238
pub(crate) fn unused_private_member(checker: &Checker, class_def: &ast::StmtClassDef) {
    let class_name = class_def.name.as_str();
    let mut visitor = PrivateMemberVisitor::new(class_name);

    // Single pass: collect definitions at class level, then traverse for accesses
    for stmt in &class_def.body {
        visitor.collect_definition(stmt);
        visitor.visit_stmt(stmt);
    }

    if visitor.definitions.is_empty() {
        return;
    }

    for member in visitor.definitions {
        if !visitor.accessed.contains(member.name) {
            checker.report_diagnostic(
                UnusedPrivateMember {
                    class_name: class_name.to_string(),
                    member_name: member.name.to_string(),
                },
                member.range,
            );
        }
    }
}
