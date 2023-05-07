use std::path::Path;

use nohash_hasher::{BuildNoHashHasher, IntMap};
use rustpython_parser::ast::{Expr, Stmt};
use smallvec::smallvec;

use ruff_python_ast::call_path::{collect_call_path, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::from_relative_import;
use ruff_python_ast::typing::AnnotationKind;
use ruff_python_stdlib::path::is_python_stub_file;
use ruff_python_stdlib::typing::TYPING_EXTENSIONS;

use crate::analyze::visibility::{module_visibility, Modifier, VisibleScope};
use crate::binding::{
    Binding, BindingId, BindingKind, Bindings, Exceptions, ExecutionContext, FromImportation,
    Importation, SubmoduleImportation,
};
use crate::node::{NodeId, Nodes};
use crate::scope::{Scope, ScopeId, ScopeKind, Scopes};

#[allow(clippy::struct_excessive_bools)]
pub struct Context<'a> {
    pub typing_modules: &'a [String],
    pub module_path: Option<Vec<String>>,
    // Stack of all visited statements, along with the identifier of the current statement.
    pub stmts: Nodes<'a>,
    pub stmt_id: Option<NodeId>,
    // Stack of all scopes, along with the identifier of the current scope.
    pub scopes: Scopes<'a>,
    pub scope_id: ScopeId,
    pub dead_scopes: Vec<ScopeId>,
    // A stack of all bindings created in any scope, at any point in execution.
    pub bindings: Bindings<'a>,
    // Map from binding index to indexes of bindings that shadow it in other scopes.
    pub shadowed_bindings:
        std::collections::HashMap<BindingId, Vec<BindingId>, BuildNoHashHasher<BindingId>>,
    pub exprs: Vec<&'a Expr>,
    // Body iteration; used to peek at siblings.
    pub body: &'a [Stmt],
    pub body_index: usize,
    // Internal, derivative state.
    pub visible_scope: VisibleScope,
    pub in_annotation: bool,
    pub in_type_definition: bool,
    pub in_deferred_string_type_definition: Option<AnnotationKind>,
    pub in_deferred_type_definition: bool,
    pub in_exception_handler: bool,
    pub in_f_string: bool,
    pub in_boolean_test: bool,
    pub in_literal: bool,
    pub in_subscript: bool,
    pub in_type_checking_block: bool,
    pub seen_import_boundary: bool,
    pub futures_allowed: bool,
    pub annotations_future_enabled: bool,
    pub handled_exceptions: Vec<Exceptions>,
}

impl<'a> Context<'a> {
    pub fn new(
        typing_modules: &'a [String],
        path: &'a Path,
        module_path: Option<Vec<String>>,
    ) -> Self {
        let visibility = module_visibility(module_path.as_deref(), path);
        Self {
            typing_modules,
            module_path,
            stmts: Nodes::default(),
            stmt_id: None,
            scopes: Scopes::default(),
            scope_id: ScopeId::global(),
            dead_scopes: Vec::default(),
            bindings: Bindings::default(),
            shadowed_bindings: IntMap::default(),
            exprs: Vec::default(),
            body: &[],
            body_index: 0,
            visible_scope: VisibleScope {
                modifier: Modifier::Module,
                visibility,
            },
            in_annotation: false,
            in_type_definition: false,
            in_deferred_string_type_definition: None,
            in_deferred_type_definition: false,
            in_exception_handler: false,
            in_f_string: false,
            in_boolean_test: false,
            in_literal: false,
            in_subscript: false,
            in_type_checking_block: false,
            seen_import_boundary: false,
            futures_allowed: true,
            annotations_future_enabled: is_python_stub_file(path),
            handled_exceptions: Vec::default(),
        }
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        self.resolve_call_path(expr).map_or(false, |call_path| {
            self.match_typing_call_path(&call_path, target)
        })
    }

    /// Return `true` if the call path is a reference to `typing.${target}`.
    pub fn match_typing_call_path(&self, call_path: &CallPath, target: &str) -> bool {
        if call_path.as_slice() == ["typing", target] {
            return true;
        }

        if TYPING_EXTENSIONS.contains(target) {
            if call_path.as_slice() == ["typing_extensions", target] {
                return true;
            }
        }

        if self.typing_modules.iter().any(|module| {
            let mut module: CallPath = from_unqualified_name(module);
            module.push(target);
            *call_path == module
        }) {
            return true;
        }

        false
    }

    /// Return the current `Binding` for a given `name`.
    pub fn find_binding(&self, member: &str) -> Option<&Binding> {
        self.scopes()
            .find_map(|scope| scope.get(member))
            .map(|index| &self.bindings[*index])
    }

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.find_binding(member)
            .map_or(false, |binding| binding.kind.is_builtin())
    }

    /// Resolves the [`Expr`] to a fully-qualified symbol-name, if `value` resolves to an imported
    /// or builtin symbol.
    ///
    /// E.g., given:
    ///
    ///
    /// ```python
    /// from sys import version_info as python_version
    /// print(python_version)
    /// ```
    ///
    /// ...then `resolve_call_path(${python_version})` will resolve to `sys.version_info`.
    pub fn resolve_call_path<'b>(&'a self, value: &'b Expr) -> Option<CallPath<'a>>
    where
        'b: 'a,
    {
        let Some(call_path) = collect_call_path(value) else {
            return None;
        };
        let Some(head) = call_path.first() else {
            return None;
        };
        let Some(binding) = self.find_binding(head) else {
            return None;
        };
        match &binding.kind {
            BindingKind::Importation(import) => {
                let name = import.full_name;
                if name.starts_with('.') {
                    if let Some(module) = &self.module_path {
                        let mut source_path = from_relative_import(module, name);
                        if source_path.is_empty() {
                            None
                        } else {
                            source_path.extend(call_path.into_iter().skip(1));
                            Some(source_path)
                        }
                    } else {
                        None
                    }
                } else {
                    let mut source_path: CallPath = from_unqualified_name(name);
                    source_path.extend(call_path.into_iter().skip(1));
                    Some(source_path)
                }
            }
            BindingKind::SubmoduleImportation(import) => {
                let name = import.name;
                if name.starts_with('.') {
                    if let Some(module) = &self.module_path {
                        let mut source_path = from_relative_import(module, name);
                        if source_path.is_empty() {
                            None
                        } else {
                            source_path.extend(call_path.into_iter().skip(1));
                            Some(source_path)
                        }
                    } else {
                        None
                    }
                } else {
                    let mut source_path: CallPath = from_unqualified_name(name);
                    source_path.extend(call_path.into_iter().skip(1));
                    Some(source_path)
                }
            }
            BindingKind::FromImportation(import) => {
                let name = &import.full_name;
                if name.starts_with('.') {
                    if let Some(module) = &self.module_path {
                        let mut source_path = from_relative_import(module, name);
                        if source_path.is_empty() {
                            None
                        } else {
                            source_path.extend(call_path.into_iter().skip(1));
                            Some(source_path)
                        }
                    } else {
                        None
                    }
                } else {
                    let mut source_path: CallPath = from_unqualified_name(name);
                    source_path.extend(call_path.into_iter().skip(1));
                    Some(source_path)
                }
            }
            BindingKind::Builtin => {
                let mut source_path: CallPath = smallvec![];
                source_path.push("");
                source_path.extend(call_path);
                Some(source_path)
            }
            _ => None,
        }
    }

    /// Given a `module` and `member`, return the fully-qualified name of the binding in the current
    /// scope, if it exists.
    ///
    /// E.g., given:
    ///
    /// ```python
    /// from sys import version_info as python_version
    /// print(python_version)
    /// ```
    ///
    /// ...then `resolve_qualified_import_name("sys", "version_info")` will return
    /// `Some("python_version")`.
    pub fn resolve_qualified_import_name(
        &self,
        module: &str,
        member: &str,
    ) -> Option<(&Stmt, String)> {
        self.scopes().enumerate().find_map(|(scope_index, scope)| {
            scope.binding_ids().find_map(|binding_index| {
                let binding = &self.bindings[*binding_index];
                match &binding.kind {
                    // Ex) Given `module="sys"` and `object="exit"`:
                    // `import sys`         -> `sys.exit`
                    // `import sys as sys2` -> `sys2.exit`
                    BindingKind::Importation(import) => {
                        if import.full_name == module {
                            // Verify that `sys` isn't bound in an inner scope.
                            if self
                                .scopes()
                                .take(scope_index)
                                .all(|scope| scope.get(import.name).is_none())
                            {
                                if let Some(source) = binding.source {
                                    return Some((
                                        self.stmts[source],
                                        format!("{}.{member}", import.name),
                                    ));
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os.path"` and `object="join"`:
                    // `from os.path import join`          -> `join`
                    // `from os.path import join as join2` -> `join2`
                    BindingKind::FromImportation(import) => {
                        if let Some((target_module, target_member)) =
                            import.full_name.split_once('.')
                        {
                            if target_module == module && target_member == member {
                                // Verify that `join` isn't bound in an inner scope.
                                if self
                                    .scopes()
                                    .take(scope_index)
                                    .all(|scope| scope.get(import.name).is_none())
                                {
                                    if let Some(source) = binding.source {
                                        return Some((
                                            self.stmts[source],
                                            (*import.name).to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os"` and `object="name"`:
                    // `import os.path ` -> `os.name`
                    BindingKind::SubmoduleImportation(import) => {
                        if import.name == module {
                            // Verify that `os` isn't bound in an inner scope.
                            if self
                                .scopes()
                                .take(scope_index)
                                .all(|scope| scope.get(import.name).is_none())
                            {
                                if let Some(source) = binding.source {
                                    return Some((
                                        self.stmts[source],
                                        format!("{}.{member}", import.name),
                                    ));
                                }
                            }
                        }
                    }
                    // Non-imports.
                    _ => {}
                }
                None
            })
        })
    }

    /// Push a [`Stmt`] onto the stack.
    pub fn push_stmt(&mut self, stmt: &'a Stmt) {
        self.stmt_id = Some(self.stmts.insert(stmt, self.stmt_id));
    }

    /// Pop the current [`Stmt`] off the stack.
    pub fn pop_stmt(&mut self) {
        let node_id = self.stmt_id.expect("Attempted to pop without statement");
        self.stmt_id = self.stmts.parent_id(node_id);
    }

    pub fn push_expr(&mut self, expr: &'a Expr) {
        self.exprs.push(expr);
    }

    pub fn pop_expr(&mut self) {
        self.exprs
            .pop()
            .expect("Attempted to pop without expression");
    }

    /// Push a [`Scope`] with the given [`ScopeKind`] onto the stack.
    pub fn push_scope(&mut self, kind: ScopeKind<'a>) {
        let id = self.scopes.push_scope(kind, self.scope_id);
        self.scope_id = id;
    }

    /// Pop the current [`Scope`] off the stack.
    pub fn pop_scope(&mut self) {
        self.dead_scopes.push(self.scope_id);
        self.scope_id = self.scopes[self.scope_id]
            .parent
            .expect("Attempted to pop without scope");
    }

    /// Return the current `Stmt`.
    pub fn current_stmt(&self) -> &'a Stmt {
        let node_id = self.stmt_id.expect("No current statement");
        self.stmts[node_id]
    }

    /// Return the parent `Stmt` of the current `Stmt`, if any.
    pub fn current_stmt_parent(&self) -> Option<&'a Stmt> {
        let node_id = self.stmt_id.expect("No current statement");
        let parent_id = self.stmts.parent_id(node_id)?;
        Some(self.stmts[parent_id])
    }

    /// Return the parent `Expr` of the current `Expr`.
    pub fn current_expr_parent(&self) -> Option<&'a Expr> {
        self.exprs.iter().rev().nth(1).copied()
    }

    /// Return the grandparent `Expr` of the current `Expr`.
    pub fn current_expr_grandparent(&self) -> Option<&'a Expr> {
        self.exprs.iter().rev().nth(2).copied()
    }

    /// Return an [`Iterator`] over the current `Expr` parents.
    pub fn expr_ancestors(&self) -> impl Iterator<Item = &&Expr> {
        self.exprs.iter().rev().skip(1)
    }

    /// Return the `Stmt` that immediately follows the current `Stmt`, if any.
    pub fn current_sibling_stmt(&self) -> Option<&'a Stmt> {
        self.body.get(self.body_index + 1)
    }

    /// Returns a reference to the global scope
    pub fn global_scope(&self) -> &Scope<'a> {
        self.scopes.global()
    }

    /// Returns a mutable reference to the global scope
    pub fn global_scope_mut(&mut self) -> &mut Scope<'a> {
        self.scopes.global_mut()
    }

    /// Returns the current top most scope.
    pub fn scope(&self) -> &Scope<'a> {
        &self.scopes[self.scope_id]
    }

    /// Returns a mutable reference to the current top most scope.
    pub fn scope_mut(&mut self) -> &mut Scope<'a> {
        &mut self.scopes[self.scope_id]
    }

    /// Returns an iterator over all scopes, starting from the current scope.
    pub fn scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.ancestors(self.scope_id)
    }

    pub fn parents(&self) -> impl Iterator<Item = &Stmt> + '_ {
        let node_id = self.stmt_id.expect("No current statement");
        self.stmts.ancestor_ids(node_id).map(|id| self.stmts[id])
    }

    /// Return `true` if the context is at the top level of the module (i.e., in the module scope,
    /// and not nested within any statements).
    pub fn at_top_level(&self) -> bool {
        self.scope_id.is_global()
            && self
                .stmt_id
                .map_or(true, |stmt_id| self.stmts.parent_id(stmt_id).is_none())
    }

    /// Returns `true` if the context is in an exception handler.
    pub const fn in_exception_handler(&self) -> bool {
        self.in_exception_handler
    }

    /// Return the [`ExecutionContext`] of the current scope.
    pub const fn execution_context(&self) -> ExecutionContext {
        if self.in_type_checking_block
            || self.in_annotation
            || self.in_deferred_string_type_definition.is_some()
        {
            ExecutionContext::Typing
        } else {
            ExecutionContext::Runtime
        }
    }

    /// Return the union of all handled exceptions as an [`Exceptions`] bitflag.
    pub fn exceptions(&self) -> Exceptions {
        let mut exceptions = Exceptions::empty();
        for exception in &self.handled_exceptions {
            exceptions.insert(*exception);
        }
        exceptions
    }
}
