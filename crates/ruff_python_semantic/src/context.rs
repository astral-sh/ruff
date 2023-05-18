use std::collections::HashMap;
use std::path::Path;

use bitflags::bitflags;
use nohash_hasher::{BuildNoHashHasher, IntMap};
use rustpython_parser::ast::{Expr, Stmt};
use smallvec::smallvec;

use ruff_python_ast::call_path::{collect_call_path, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::from_relative_import;
use ruff_python_stdlib::path::is_python_stub_file;
use ruff_python_stdlib::typing::TYPING_EXTENSIONS;

use crate::binding::{
    Binding, BindingId, BindingKind, Bindings, Exceptions, ExecutionContext, FromImportation,
    Importation, SubmoduleImportation,
};
use crate::definition::{Definition, DefinitionId, Definitions, Member, Module};
use crate::node::{NodeId, Nodes};
use crate::scope::{Scope, ScopeId, ScopeKind, Scopes};

#[allow(clippy::struct_excessive_bools)]
pub struct Context<'a> {
    pub typing_modules: &'a [String],
    pub module_path: Option<&'a [String]>,
    // Stack of all visited statements, along with the identifier of the current statement.
    pub stmts: Nodes<'a>,
    pub stmt_id: Option<NodeId>,
    // Stack of current expressions.
    pub exprs: Vec<&'a Expr>,
    // Stack of all scopes, along with the identifier of the current scope.
    pub scopes: Scopes<'a>,
    pub scope_id: ScopeId,
    pub dead_scopes: Vec<ScopeId>,
    // Stack of all definitions created in any scope, at any point in execution, along with the
    // identifier of the current definition.
    pub definitions: Definitions<'a>,
    pub definition_id: DefinitionId,
    // A stack of all bindings created in any scope, at any point in execution.
    pub bindings: Bindings<'a>,
    // Map from binding index to indexes of bindings that shadow it in other scopes.
    pub shadowed_bindings: HashMap<BindingId, Vec<BindingId>, BuildNoHashHasher<BindingId>>,
    // Body iteration; used to peek at siblings.
    pub body: &'a [Stmt],
    pub body_index: usize,
    // Internal, derivative state.
    pub flags: ContextFlags,
    pub handled_exceptions: Vec<Exceptions>,
}

impl<'a> Context<'a> {
    pub fn new(typing_modules: &'a [String], path: &'a Path, module: Module<'a>) -> Self {
        Self {
            typing_modules,
            module_path: module.path(),
            stmts: Nodes::default(),
            stmt_id: None,
            exprs: Vec::default(),
            scopes: Scopes::default(),
            scope_id: ScopeId::global(),
            dead_scopes: Vec::default(),
            definitions: Definitions::for_module(module),
            definition_id: DefinitionId::module(),
            bindings: Bindings::default(),
            shadowed_bindings: IntMap::default(),
            body: &[],
            body_index: 0,
            flags: ContextFlags::new(path),
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
            BindingKind::Importation(Importation {
                full_name: name, ..
            })
            | BindingKind::SubmoduleImportation(SubmoduleImportation { name, .. }) => {
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
            BindingKind::FromImportation(FromImportation {
                full_name: name, ..
            }) => {
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
                    BindingKind::Importation(Importation { name, full_name }) => {
                        if full_name == &module {
                            // Verify that `sys` isn't bound in an inner scope.
                            if self
                                .scopes()
                                .take(scope_index)
                                .all(|scope| scope.get(name).is_none())
                            {
                                if let Some(source) = binding.source {
                                    return Some((self.stmts[source], format!("{name}.{member}")));
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os.path"` and `object="join"`:
                    // `from os.path import join`          -> `join`
                    // `from os.path import join as join2` -> `join2`
                    BindingKind::FromImportation(FromImportation { name, full_name }) => {
                        if let Some((target_module, target_member)) = full_name.split_once('.') {
                            if target_module == module && target_member == member {
                                // Verify that `join` isn't bound in an inner scope.
                                if self
                                    .scopes()
                                    .take(scope_index)
                                    .all(|scope| scope.get(name).is_none())
                                {
                                    if let Some(source) = binding.source {
                                        return Some((self.stmts[source], (*name).to_string()));
                                    }
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os"` and `object="name"`:
                    // `import os.path ` -> `os.name`
                    BindingKind::SubmoduleImportation(SubmoduleImportation { name, .. }) => {
                        if name == &module {
                            // Verify that `os` isn't bound in an inner scope.
                            if self
                                .scopes()
                                .take(scope_index)
                                .all(|scope| scope.get(name).is_none())
                            {
                                if let Some(source) = binding.source {
                                    return Some((self.stmts[source], format!("{name}.{member}")));
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

    /// Push an [`Expr`] onto the stack.
    pub fn push_expr(&mut self, expr: &'a Expr) {
        self.exprs.push(expr);
    }

    /// Pop the current [`Expr`] off the stack.
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

    /// Push a [`Member`] onto the stack.
    pub fn push_definition(&mut self, definition: Member<'a>) {
        self.definition_id = self.definitions.push_member(definition);
    }

    /// Pop the current [`Member`] off the stack.
    pub fn pop_definition(&mut self) {
        let Definition::Member(member) = &self.definitions[self.definition_id] else {
            panic!("Attempted to pop without member definition");
        };
        self.definition_id = member.parent;
    }

    /// Return the current `Stmt`.
    pub fn stmt(&self) -> &'a Stmt {
        let node_id = self.stmt_id.expect("No current statement");
        self.stmts[node_id]
    }

    /// Return the parent `Stmt` of the current `Stmt`, if any.
    pub fn stmt_parent(&self) -> Option<&'a Stmt> {
        let node_id = self.stmt_id.expect("No current statement");
        let parent_id = self.stmts.parent_id(node_id)?;
        Some(self.stmts[parent_id])
    }

    /// Return the current `Expr`.
    pub fn expr(&self) -> Option<&'a Expr> {
        self.exprs.iter().last().copied()
    }

    /// Return the parent `Expr` of the current `Expr`.
    pub fn expr_parent(&self) -> Option<&'a Expr> {
        self.exprs.iter().rev().nth(1).copied()
    }

    /// Return the grandparent `Expr` of the current `Expr`.
    pub fn expr_grandparent(&self) -> Option<&'a Expr> {
        self.exprs.iter().rev().nth(2).copied()
    }

    /// Return an [`Iterator`] over the current `Expr` parents.
    pub fn expr_ancestors(&self) -> impl Iterator<Item = &&Expr> {
        self.exprs.iter().rev().skip(1)
    }

    /// Return the `Stmt` that immediately follows the current `Stmt`, if any.
    pub fn sibling_stmt(&self) -> Option<&'a Stmt> {
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

    /// Return the [`ExecutionContext`] of the current scope.
    pub const fn execution_context(&self) -> ExecutionContext {
        if self.in_type_checking_block()
            || self.in_annotation()
            || self.in_complex_string_type_definition()
            || self.in_simple_string_type_definition()
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

    /// Generate a [`Snapshot`] of the current context.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            scope_id: self.scope_id,
            stmt_id: self.stmt_id,
            definition_id: self.definition_id,
            flags: self.flags,
        }
    }

    /// Restore the context to the given [`Snapshot`].
    pub fn restore(&mut self, snapshot: Snapshot) {
        let Snapshot {
            scope_id,
            stmt_id,
            definition_id,
            flags,
        } = snapshot;
        self.scope_id = scope_id;
        self.stmt_id = stmt_id;
        self.definition_id = definition_id;
        self.flags = flags;
    }

    /// Return `true` if the context is in a type annotation.
    pub const fn in_annotation(&self) -> bool {
        self.flags.contains(ContextFlags::ANNOTATION)
    }

    /// Return `true` if the context is in a type definition.
    pub const fn in_type_definition(&self) -> bool {
        self.flags.contains(ContextFlags::TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a "simple" string type definition.
    pub const fn in_simple_string_type_definition(&self) -> bool {
        self.flags
            .contains(ContextFlags::SIMPLE_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a "complex" string type definition.
    pub const fn in_complex_string_type_definition(&self) -> bool {
        self.flags
            .contains(ContextFlags::COMPLEX_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a `__future__` type definition.
    pub const fn in_future_type_definition(&self) -> bool {
        self.flags.contains(ContextFlags::FUTURE_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in any kind of deferred type definition.
    pub const fn in_deferred_type_definition(&self) -> bool {
        self.in_simple_string_type_definition()
            || self.in_complex_string_type_definition()
            || self.in_future_type_definition()
    }

    /// Return `true` if the context is in an exception handler.
    pub const fn in_exception_handler(&self) -> bool {
        self.flags.contains(ContextFlags::EXCEPTION_HANDLER)
    }

    /// Return `true` if the context is in an f-string.
    pub const fn in_f_string(&self) -> bool {
        self.flags.contains(ContextFlags::F_STRING)
    }

    /// Return `true` if the context is in a nested f-string.
    pub const fn in_nested_f_string(&self) -> bool {
        self.flags.contains(ContextFlags::NESTED_F_STRING)
    }

    /// Return `true` if the context is in boolean test.
    pub const fn in_boolean_test(&self) -> bool {
        self.flags.contains(ContextFlags::BOOLEAN_TEST)
    }

    /// Return `true` if the context is in a `typing::Literal` annotation.
    pub const fn in_literal(&self) -> bool {
        self.flags.contains(ContextFlags::LITERAL)
    }

    /// Return `true` if the context is in a subscript expression.
    pub const fn in_subscript(&self) -> bool {
        self.flags.contains(ContextFlags::SUBSCRIPT)
    }

    /// Return `true` if the context is in a type-checking block.
    pub const fn in_type_checking_block(&self) -> bool {
        self.flags.contains(ContextFlags::TYPE_CHECKING_BLOCK)
    }

    /// Return `true` if the context has traversed past the "top-of-file" import boundary.
    pub const fn seen_import_boundary(&self) -> bool {
        self.flags.contains(ContextFlags::IMPORT_BOUNDARY)
    }

    /// Return `true` if the context has traverse past the `__future__` import boundary.
    pub const fn seen_futures_boundary(&self) -> bool {
        self.flags.contains(ContextFlags::FUTURES_BOUNDARY)
    }

    /// Return `true` if `__future__`-style type annotations are enabled.
    pub const fn future_annotations(&self) -> bool {
        self.flags.contains(ContextFlags::FUTURE_ANNOTATIONS)
    }
}

bitflags! {
    /// Flags indicating the current context of the analysis.
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct ContextFlags: u16 {
        /// The context is in a type annotation.
        ///
        /// For example, the context could be visiting `int` in:
        /// ```python
        /// x: int = 1
        /// ```
        const ANNOTATION = 1 << 0;

        /// The context is in a type definition.
        ///
        /// For example, the context could be visiting `int` in:
        /// ```python
        /// from typing import NewType
        ///
        /// UserId = NewType("UserId", int)
        /// ```
        ///
        /// All type annotations are also type definitions, but the converse is not true.
        /// In our example, `int` is a type definition but not a type annotation, as it
        /// doesn't appear in a type annotation context, but rather in a type definition.
        const TYPE_DEFINITION = 1 << 1;

        /// The context is in a (deferred) "simple" string type definition.
        ///
        /// For example, the context could be visiting `list[int]` in:
        /// ```python
        /// x: "list[int]" = []
        /// ```
        ///
        /// "Simple" string type definitions are those that consist of a single string literal,
        /// as opposed to an implicitly concatenated string literal.
        const SIMPLE_STRING_TYPE_DEFINITION =  1 << 2;

        /// The context is in a (deferred) "complex" string type definition.
        ///
        /// For example, the context could be visiting `list[int]` in:
        /// ```python
        /// x: ("list" "[int]") = []
        /// ```
        ///
        /// "Complex" string type definitions are those that consist of a implicitly concatenated
        /// string literals. These are uncommon but valid.
        const COMPLEX_STRING_TYPE_DEFINITION = 1 << 3;

        /// The context is in a (deferred) `__future__` type definition.
        ///
        /// For example, the context could be visiting `list[int]` in:
        /// ```python
        /// from __future__ import annotations
        ///
        /// x: list[int] = []
        /// ```
        ///
        /// `__future__`-style type annotations are only enabled if the `annotations` feature
        /// is enabled via `from __future__ import annotations`.
        const FUTURE_TYPE_DEFINITION = 1 << 4;

        /// The context is in an exception handler.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// try:
        ///     ...
        /// except Exception:
        ///     x: int = 1
        /// ```
        const EXCEPTION_HANDLER = 1 << 5;

        /// The context is in an f-string.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// f'{x}'
        /// ```
        const F_STRING = 1 << 6;

        /// The context is in a nested f-string.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// f'{f"{x}"}'
        /// ```
        const NESTED_F_STRING = 1 << 7;

        /// The context is in a boolean test.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// if x:
        ///     ...
        /// ```
        ///
        /// The implication is that the actual value returned by the current expression is
        /// not used, only its truthiness.
        const BOOLEAN_TEST = 1 << 8;

        /// The context is in a `typing::Literal` annotation.
        ///
        /// For example, the context could be visiting any of `"A"`, `"B"`, or `"C"` in:
        /// ```python
        /// def f(x: Literal["A", "B", "C"]):
        ///     ...
        /// ```
        const LITERAL = 1 << 9;

        /// The context is in a subscript expression.
        ///
        /// For example, the context could be visiting `x["a"]` in:
        /// ```python
        /// x["a"]["b"]
        /// ```
        const SUBSCRIPT = 1 << 10;

        /// The context is in a type-checking block.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// from typing import TYPE_CHECKING
        ///
        ///
        /// if TYPE_CHECKING:
        ///    x: int = 1
        /// ```
        const TYPE_CHECKING_BLOCK = 1 << 11;


        /// The context has traversed past the "top-of-file" import boundary.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// import os
        ///
        /// def f() -> None:
        ///     ...
        ///
        /// x: int = 1
        /// ```
        const IMPORT_BOUNDARY = 1 << 12;

        /// The context has traversed past the `__future__` import boundary.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// from __future__ import annotations
        ///
        /// import os
        ///
        /// x: int = 1
        /// ```
        ///
        /// Python considers it a syntax error to import from `__future__` after
        /// any other non-`__future__`-importing statements.
        const FUTURES_BOUNDARY = 1 << 13;

        /// `__future__`-style type annotations are enabled in this context.
        ///
        /// For example, the context could be visiting `x` in:
        /// ```python
        /// from __future__ import annotations
        ///
        ///
        /// def f(x: int) -> int:
        ///   ...
        /// ```
        const FUTURE_ANNOTATIONS = 1 << 14;
    }
}

impl ContextFlags {
    pub fn new(path: &Path) -> Self {
        let mut flags = Self::default();
        if is_python_stub_file(path) {
            flags |= Self::FUTURE_ANNOTATIONS;
        }
        flags
    }
}

/// A snapshot of the [`Context`] at a given point in the AST traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    scope_id: ScopeId,
    stmt_id: Option<NodeId>,
    definition_id: DefinitionId,
    flags: ContextFlags,
>>>>>>> fd16d658e (Avoid autofixing within nested f-strings)
}
