use std::path::Path;

use nohash_hasher::{BuildNoHashHasher, IntMap};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Expr, Stmt};
use smallvec::smallvec;

use ruff_python_stdlib::path::is_python_stub_file;
use ruff_python_stdlib::typing::TYPING_EXTENSIONS;

use crate::helpers::{collect_call_path, from_relative_import};
use crate::scope::{
    Binding, BindingId, BindingKind, Bindings, Exceptions, ExecutionContext, Scope, ScopeId,
    ScopeKind, ScopeStack, Scopes,
};
use crate::types::{CallPath, RefEquality};
use crate::typing::AnnotationKind;
use crate::visibility::{module_visibility, Modifier, VisibleScope};

#[allow(clippy::struct_excessive_bools)]
pub struct Context<'a> {
    pub typing_modules: &'a [String],
    pub module_path: Option<Vec<String>>,
    // Retain all scopes and parent nodes, along with a stack of indexes to track which are active
    // at various points in time.
    pub parents: Vec<RefEquality<'a, Stmt>>,
    pub depths: FxHashMap<RefEquality<'a, Stmt>, usize>,
    pub child_to_parent: FxHashMap<RefEquality<'a, Stmt>, RefEquality<'a, Stmt>>,
    // A stack of all bindings created in any scope, at any point in execution.
    pub bindings: Bindings<'a>,
    // Map from binding index to indexes of bindings that redefine it in other scopes.
    pub redefinitions:
        std::collections::HashMap<BindingId, Vec<BindingId>, BuildNoHashHasher<BindingId>>,
    pub exprs: Vec<RefEquality<'a, Expr>>,
    pub scopes: Scopes<'a>,
    pub scope_stack: ScopeStack,
    pub dead_scopes: Vec<(ScopeId, ScopeStack)>,
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
        Self {
            typing_modules,
            module_path,
            parents: Vec::default(),
            depths: FxHashMap::default(),
            child_to_parent: FxHashMap::default(),
            bindings: Bindings::default(),
            redefinitions: IntMap::default(),
            exprs: Vec::default(),
            scopes: Scopes::default(),
            scope_stack: ScopeStack::default(),
            dead_scopes: Vec::default(),
            body: &[],
            body_index: 0,
            visible_scope: VisibleScope {
                modifier: Modifier::Module,
                visibility: module_visibility(path),
            },
            in_annotation: false,
            in_type_definition: false,
            in_deferred_string_type_definition: None,
            in_deferred_type_definition: false,
            in_exception_handler: false,
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
            let mut module: CallPath = module.split('.').collect();
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

    /// Resolves the call path, e.g. if you have a file
    ///
    /// ```python
    /// from sys import version_info as python_version
    /// print(python_version)
    /// ```
    ///
    /// then `python_version` from the print statement will resolve to `sys.version_info`.
    pub fn resolve_call_path<'b>(&'a self, value: &'b Expr) -> Option<CallPath<'a>>
    where
        'b: 'a,
    {
        let call_path = collect_call_path(value);
        let Some(head) = call_path.first() else {
            return None;
        };
        let Some(binding) = self.find_binding(head) else {
            return None;
        };
        match &binding.kind {
            BindingKind::Importation(.., name) | BindingKind::SubmoduleImportation(name, ..) => {
                if name.starts_with('.') {
                    if let Some(module) = &self.module_path {
                        let mut source_path = from_relative_import(module, name);
                        source_path.extend(call_path.into_iter().skip(1));
                        Some(source_path)
                    } else {
                        None
                    }
                } else {
                    let mut source_path: CallPath = name.split('.').collect();
                    source_path.extend(call_path.into_iter().skip(1));
                    Some(source_path)
                }
            }
            BindingKind::FromImportation(.., name) => {
                if name.starts_with('.') {
                    if let Some(module) = &self.module_path {
                        let mut source_path = from_relative_import(module, name);
                        source_path.extend(call_path.into_iter().skip(1));
                        Some(source_path)
                    } else {
                        None
                    }
                } else {
                    let mut source_path: CallPath = name.split('.').collect();
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

    pub fn push_parent(&mut self, parent: &'a Stmt) {
        let num_existing = self.parents.len();
        self.parents.push(RefEquality(parent));
        self.depths
            .insert(self.parents[num_existing].clone(), num_existing);
        if num_existing > 0 {
            self.child_to_parent.insert(
                self.parents[num_existing].clone(),
                self.parents[num_existing - 1].clone(),
            );
        }
    }

    pub fn pop_parent(&mut self) {
        self.parents.pop().expect("Attempted to pop without parent");
    }

    pub fn push_expr(&mut self, expr: &'a Expr) {
        self.exprs.push(RefEquality(expr));
    }

    pub fn pop_expr(&mut self) {
        self.exprs
            .pop()
            .expect("Attempted to pop without expression");
    }

    pub fn push_scope(&mut self, kind: ScopeKind<'a>) -> ScopeId {
        let id = self.scopes.push_scope(kind);
        self.scope_stack.push(id);
        id
    }

    pub fn pop_scope(&mut self) {
        self.dead_scopes.push((
            self.scope_stack
                .pop()
                .expect("Attempted to pop without scope"),
            self.scope_stack.clone(),
        ));
    }

    /// Return the current `Stmt`.
    pub fn current_stmt(&self) -> &RefEquality<'a, Stmt> {
        self.parents.iter().rev().next().expect("No parent found")
    }

    /// Return the parent `Stmt` of the current `Stmt`, if any.
    pub fn current_stmt_parent(&self) -> Option<&RefEquality<'a, Stmt>> {
        self.parents.iter().rev().nth(1)
    }

    /// Return the parent `Expr` of the current `Expr`.
    pub fn current_expr_parent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(1)
    }

    /// Return the grandparent `Expr` of the current `Expr`.
    pub fn current_expr_grandparent(&self) -> Option<&RefEquality<'a, Expr>> {
        self.exprs.iter().rev().nth(2)
    }

    /// Return an [`Iterator`] over the current `Expr` parents.
    pub fn expr_ancestors(&self) -> impl Iterator<Item = &RefEquality<'a, Expr>> {
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
        &self.scopes[self.scope_stack.top().expect("No current scope found")]
    }

    /// Returns the id of the top-most scope
    pub fn scope_id(&self) -> ScopeId {
        self.scope_stack.top().expect("No current scope found")
    }

    /// Returns a mutable reference to the current top most scope.
    pub fn scope_mut(&mut self) -> &mut Scope<'a> {
        let top_id = self.scope_stack.top().expect("No current scope found");
        &mut self.scopes[top_id]
    }

    pub fn parent_scope(&self) -> Option<&Scope> {
        self.scope_stack
            .iter()
            .nth(1)
            .map(|index| &self.scopes[*index])
    }

    pub fn scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scope_stack.iter().map(|index| &self.scopes[*index])
    }

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
