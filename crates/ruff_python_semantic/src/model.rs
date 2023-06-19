use std::collections::HashMap;
use std::path::Path;

use bitflags::bitflags;
use nohash_hasher::{BuildNoHashHasher, IntMap};
use ruff_text_size::TextRange;
use rustpython_parser::ast::{Expr, Ranged, Stmt};
use smallvec::smallvec;

use ruff_python_ast::call_path::{collect_call_path, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::from_relative_import;
use ruff_python_stdlib::path::is_python_stub_file;
use ruff_python_stdlib::typing::is_typing_extension;

use crate::binding::{
    Binding, BindingFlags, BindingId, BindingKind, Bindings, Exceptions, FromImport, Import,
    SubmoduleImport,
};
use crate::context::ExecutionContext;
use crate::definition::{Definition, DefinitionId, Definitions, Member, Module};
use crate::globals::{Globals, GlobalsArena};
use crate::node::{NodeId, Nodes};
use crate::reference::{Reference, ReferenceId, References};
use crate::scope::{Scope, ScopeId, ScopeKind, Scopes};

/// A semantic model for a Python module, to enable querying the module's semantic information.
pub struct SemanticModel<'a> {
    typing_modules: &'a [String],
    module_path: Option<&'a [String]>,

    /// Stack of all visited statements.
    pub stmts: Nodes<'a>,

    /// The identifier of the current statement.
    stmt_id: Option<NodeId>,

    /// Stack of current expressions.
    exprs: Vec<&'a Expr>,

    /// Stack of all scopes, along with the identifier of the current scope.
    pub scopes: Scopes<'a>,
    pub scope_id: ScopeId,

    /// Stack of all definitions created in any scope, at any point in execution.
    pub definitions: Definitions<'a>,

    /// The ID of the current definition.
    pub definition_id: DefinitionId,

    /// A stack of all bindings created in any scope, at any point in execution.
    pub bindings: Bindings<'a>,

    /// Stack of all references created in any scope, at any point in execution.
    references: References,

    /// Arena of global bindings.
    globals: GlobalsArena<'a>,

    /// Map from binding ID to binding ID that it shadows (in another scope).
    ///
    /// For example, given:
    /// ```python
    /// import x
    ///
    /// def f():
    ///     x = 1
    /// ```
    ///
    /// In this case, the binding created by `x = 1` shadows the binding created by `import x`,
    /// despite the fact that they're in different scopes.
    pub shadowed_bindings: HashMap<BindingId, BindingId, BuildNoHashHasher<BindingId>>,

    /// Map from binding index to indexes of bindings that annotate it (in the same scope).
    ///
    /// For example, given:
    /// ```python
    /// x = 1
    /// x: int
    /// ```
    ///
    /// In this case, the binding created by `x = 1` is annotated by the binding created by
    /// `x: int`. We don't consider the latter binding to _shadow_ the former, because it doesn't
    /// change the value of the binding, and so we don't store in on the scope. But we _do_ want to
    /// track the annotation in some form, since it's a reference to `x`.
    ///
    /// Note that, given:
    /// ```python
    /// x: int
    /// ```
    ///
    /// In this case, we _do_ store the binding created by `x: int` directly on the scope, and not
    /// as a delayed annotation. Annotations are thus treated as bindings only when they are the
    /// first binding in a scope; any annotations that follow are treated as "delayed" annotations.
    delayed_annotations: HashMap<BindingId, Vec<BindingId>, BuildNoHashHasher<BindingId>>,

    /// Map from binding ID to the IDs of all scopes in which it is declared a `global` or
    /// `nonlocal`.
    ///
    /// For example, given:
    /// ```python
    /// x = 1
    ///
    /// def f():
    ///    global x
    /// ```
    ///
    /// In this case, the binding created by `x = 1` is rebound within the scope created by `f`
    /// by way of the `global x` statement.
    rebinding_scopes: HashMap<BindingId, Vec<ScopeId>, BuildNoHashHasher<BindingId>>,

    /// Body iteration; used to peek at siblings.
    pub body: &'a [Stmt],
    pub body_index: usize,

    /// Flags for the semantic model.
    pub flags: SemanticModelFlags,

    /// Exceptions that have been handled by the current scope.
    pub handled_exceptions: Vec<Exceptions>,
}

impl<'a> SemanticModel<'a> {
    pub fn new(typing_modules: &'a [String], path: &'a Path, module: Module<'a>) -> Self {
        Self {
            typing_modules,
            module_path: module.path(),
            stmts: Nodes::default(),
            stmt_id: None,
            exprs: Vec::default(),
            scopes: Scopes::default(),
            scope_id: ScopeId::global(),
            definitions: Definitions::for_module(module),
            definition_id: DefinitionId::module(),
            bindings: Bindings::default(),
            references: References::default(),
            globals: GlobalsArena::default(),
            shadowed_bindings: IntMap::default(),
            delayed_annotations: IntMap::default(),
            rebinding_scopes: IntMap::default(),
            body: &[],
            body_index: 0,
            flags: SemanticModelFlags::new(path),
            handled_exceptions: Vec::default(),
        }
    }

    /// Return the [`Binding`] for the given [`BindingId`].
    #[inline]
    pub fn binding(&self, id: BindingId) -> &Binding {
        &self.bindings[id]
    }

    /// Resolve the [`Reference`] for the given [`ReferenceId`].
    #[inline]
    pub fn reference(&self, id: ReferenceId) -> &Reference {
        &self.references[id]
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

        if call_path.as_slice() == ["_typeshed", target] {
            return true;
        }

        if is_typing_extension(target) {
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

    /// Create a new [`Binding`] for a builtin.
    pub fn push_builtin(&mut self) -> BindingId {
        self.bindings.push(Binding {
            range: TextRange::default(),
            kind: BindingKind::Builtin,
            references: Vec::new(),
            flags: BindingFlags::empty(),
            source: None,
            context: ExecutionContext::Runtime,
            exceptions: Exceptions::empty(),
        })
    }

    /// Create a new [`Binding`] for the given `name` and `range`.
    pub fn push_binding(
        &mut self,
        range: TextRange,
        kind: BindingKind<'a>,
        flags: BindingFlags,
    ) -> BindingId {
        self.bindings.push(Binding {
            range,
            kind,
            flags,
            references: Vec::new(),
            source: self.stmt_id,
            context: self.execution_context(),
            exceptions: self.exceptions(),
        })
    }

    /// Return the current [`Binding`] for a given `name`.
    pub fn find_binding(&self, member: &str) -> Option<&Binding> {
        self.scopes()
            .find_map(|scope| scope.get(member))
            .map(|binding_id| &self.bindings[binding_id])
    }

    /// Return the [`BindingId`] that the given [`BindingId`] shadows, if any.
    ///
    /// Note that this will only return bindings that are shadowed by a binding in a parent scope.
    pub fn shadowed_binding(&self, binding_id: BindingId) -> Option<BindingId> {
        self.shadowed_bindings.get(&binding_id).copied()
    }

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.find_binding(member)
            .map_or(false, |binding| binding.kind.is_builtin())
    }

    /// Return `true` if `member` is an "available" symbol, i.e., a symbol that has not been bound
    /// in the current scope, or in any containing scope.
    pub fn is_available(&self, member: &str) -> bool {
        self.find_binding(member)
            .map_or(true, |binding| binding.kind.is_builtin())
    }

    /// Resolve a read reference to `symbol` at `range`.
    pub fn resolve_read(&mut self, symbol: &str, range: TextRange) -> ResolvedRead {
        // PEP 563 indicates that if a forward reference can be resolved in the module scope, we
        // should prefer it over local resolutions.
        if self.in_forward_reference() {
            if let Some(binding_id) = self.scopes.global().get(symbol) {
                if !self.bindings[binding_id].is_unbound() {
                    // Mark the binding as used.
                    let context = self.execution_context();
                    let reference_id = self.references.push(ScopeId::global(), range, context);
                    self.bindings[binding_id].references.push(reference_id);

                    // Mark any submodule aliases as used.
                    if let Some(binding_id) =
                        self.resolve_submodule(symbol, ScopeId::global(), binding_id)
                    {
                        let reference_id = self.references.push(ScopeId::global(), range, context);
                        self.bindings[binding_id].references.push(reference_id);
                    }

                    return ResolvedRead::Resolved(binding_id);
                }
            }
        }

        let mut seen_function = false;
        let mut import_starred = false;
        for (index, scope_id) in self.scopes.ancestor_ids(self.scope_id).enumerate() {
            let scope = &self.scopes[scope_id];
            if scope.kind.is_class() {
                // Allow usages of `__class__` within methods, e.g.:
                //
                // ```python
                // class Foo:
                //     def __init__(self):
                //         print(__class__)
                // ```
                if seen_function && matches!(symbol, "__class__") {
                    return ResolvedRead::ImplicitGlobal;
                }
                if index > 0 {
                    continue;
                }
            }

            if let Some(binding_id) = scope.get(symbol) {
                // Mark the binding as used.
                let context = self.execution_context();
                let reference_id = self.references.push(self.scope_id, range, context);
                self.bindings[binding_id].references.push(reference_id);

                // Mark any submodule aliases as used.
                if let Some(binding_id) = self.resolve_submodule(symbol, scope_id, binding_id) {
                    let reference_id = self.references.push(self.scope_id, range, context);
                    self.bindings[binding_id].references.push(reference_id);
                }

                match self.bindings[binding_id].kind {
                    // If it's a type annotation, don't treat it as resolved. For example, given:
                    //
                    // ```python
                    // name: str
                    // print(name)
                    // ```
                    //
                    // The `name` in `print(name)` should be treated as unresolved, but the `name` in
                    // `name: str` should be treated as used.
                    BindingKind::Annotation => continue,

                    // If it's a deletion, don't treat it as resolved, since the name is now
                    // unbound. For example, given:
                    //
                    // ```python
                    // x = 1
                    // del x
                    // print(x)
                    // ```
                    //
                    // The `x` in `print(x)` should be treated as unresolved.
                    //
                    // Similarly, given:
                    //
                    // ```python
                    // try:
                    //     pass
                    // except ValueError as x:
                    //     pass
                    //
                    // print(x)
                    //
                    // The `x` in `print(x)` should be treated as unresolved.
                    BindingKind::Deletion | BindingKind::UnboundException(None) => {
                        return ResolvedRead::UnboundLocal(binding_id)
                    }

                    // If we hit an unbound exception that shadowed a bound name, resole to the
                    // bound name. For example, given:
                    //
                    // ```python
                    // x = 1
                    //
                    // try:
                    //     pass
                    // except ValueError as x:
                    //     pass
                    //
                    // print(x)
                    // ```
                    //
                    // The `x` in `print(x)` should resolve to the `x` in `x = 1`.
                    BindingKind::UnboundException(Some(binding_id)) => {
                        // Mark the binding as used.
                        let context = self.execution_context();
                        let reference_id = self.references.push(self.scope_id, range, context);
                        self.bindings[binding_id].references.push(reference_id);

                        // Mark any submodule aliases as used.
                        if let Some(binding_id) =
                            self.resolve_submodule(symbol, scope_id, binding_id)
                        {
                            let reference_id = self.references.push(self.scope_id, range, context);
                            self.bindings[binding_id].references.push(reference_id);
                        }

                        return ResolvedRead::Resolved(binding_id);
                    }

                    // Otherwise, treat it as resolved.
                    _ => return ResolvedRead::Resolved(binding_id),
                }
            }

            // Allow usages of `__module__` and `__qualname__` within class scopes, e.g.:
            //
            // ```python
            // class Foo:
            //     print(__qualname__)
            // ```
            //
            // Intentionally defer this check to _after_ the standard `scope.get` logic, so that
            // we properly attribute reads to overridden class members, e.g.:
            //
            // ```python
            // class Foo:
            //     __qualname__ = "Bar"
            //     print(__qualname__)
            // ```
            if index == 0 && scope.kind.is_class() {
                if matches!(symbol, "__module__" | "__qualname__") {
                    return ResolvedRead::ImplicitGlobal;
                }
            }

            seen_function |= scope.kind.is_any_function();
            import_starred = import_starred || scope.uses_star_imports();
        }

        if import_starred {
            ResolvedRead::WildcardImport
        } else {
            ResolvedRead::NotFound
        }
    }

    /// Lookup a symbol in the current scope. This is a carbon copy of [`Self::resolve_read`], but
    /// doesn't add any read references to the resolved symbol.
    pub fn lookup(&mut self, symbol: &str) -> Option<BindingId> {
        if self.in_forward_reference() {
            if let Some(binding_id) = self.scopes.global().get(symbol) {
                if !self.bindings[binding_id].is_unbound() {
                    return Some(binding_id);
                }
            }
        }

        let mut seen_function = false;
        for (index, scope_id) in self.scopes.ancestor_ids(self.scope_id).enumerate() {
            let scope = &self.scopes[scope_id];
            if scope.kind.is_class() {
                if seen_function && matches!(symbol, "__class__") {
                    return None;
                }
                if index > 0 {
                    continue;
                }
            }

            if let Some(binding_id) = scope.get(symbol) {
                match self.bindings[binding_id].kind {
                    BindingKind::Annotation => continue,
                    BindingKind::Deletion | BindingKind::UnboundException(None) => return None,
                    BindingKind::UnboundException(Some(binding_id)) => return Some(binding_id),
                    _ => return Some(binding_id),
                }
            }

            if index == 0 && scope.kind.is_class() {
                if matches!(symbol, "__module__" | "__qualname__") {
                    return None;
                }
            }

            seen_function |= scope.kind.is_any_function();
        }

        None
    }

    /// Given a `BindingId`, return the `BindingId` of the submodule import that it aliases.
    fn resolve_submodule(
        &self,
        symbol: &str,
        scope_id: ScopeId,
        binding_id: BindingId,
    ) -> Option<BindingId> {
        // If the name of a submodule import is the same as an alias of another import, and the
        // alias is used, then the submodule import should be marked as used too.
        //
        // For example, mark `pyarrow.csv` as used in:
        //
        // ```python
        // import pyarrow as pa
        // import pyarrow.csv
        // print(pa.csv.read_csv("test.csv"))
        // ```
        let qualified_name = self.bindings[binding_id].qualified_name()?;
        let has_alias = qualified_name
            .split('.')
            .last()
            .map(|segment| segment != symbol)
            .unwrap_or_default();
        if !has_alias {
            return None;
        }

        self.scopes[scope_id].get(qualified_name)
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
    pub fn resolve_call_path(&'a self, value: &'a Expr) -> Option<CallPath<'a>> {
        let call_path = collect_call_path(value)?;
        let head = call_path.first()?;
        let binding = self.find_binding(head)?;
        match &binding.kind {
            BindingKind::Import(Import {
                qualified_name: name,
            }) => {
                if name.starts_with('.') {
                    let mut source_path = from_relative_import(self.module_path?, name);
                    if source_path.is_empty() {
                        None
                    } else {
                        source_path.extend(call_path.into_iter().skip(1));
                        Some(source_path)
                    }
                } else {
                    let mut source_path: CallPath = from_unqualified_name(name);
                    source_path.extend(call_path.into_iter().skip(1));
                    Some(source_path)
                }
            }
            BindingKind::SubmoduleImport(SubmoduleImport {
                qualified_name: name,
            }) => {
                let name = name.split('.').next().unwrap_or(name);
                let mut source_path: CallPath = from_unqualified_name(name);
                source_path.extend(call_path.into_iter().skip(1));
                Some(source_path)
            }
            BindingKind::FromImport(FromImport {
                qualified_name: name,
            }) => {
                if name.starts_with('.') {
                    let mut source_path = from_relative_import(self.module_path?, name);
                    if source_path.is_empty() {
                        None
                    } else {
                        source_path.extend(call_path.into_iter().skip(1));
                        Some(source_path)
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
    ) -> Option<ImportedName> {
        self.scopes().enumerate().find_map(|(scope_index, scope)| {
            scope.bindings().find_map(|(name, binding_id)| {
                let binding = &self.bindings[binding_id];
                match &binding.kind {
                    // Ex) Given `module="sys"` and `object="exit"`:
                    // `import sys`         -> `sys.exit`
                    // `import sys as sys2` -> `sys2.exit`
                    BindingKind::Import(Import { qualified_name }) => {
                        if qualified_name == &module {
                            if let Some(source) = binding.source {
                                // Verify that `sys` isn't bound in an inner scope.
                                if self
                                    .scopes()
                                    .take(scope_index)
                                    .all(|scope| !scope.has(name))
                                {
                                    return Some(ImportedName {
                                        name: format!("{name}.{member}"),
                                        range: self.stmts[source].range(),
                                        context: binding.context,
                                    });
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os.path"` and `object="join"`:
                    // `from os.path import join`          -> `join`
                    // `from os.path import join as join2` -> `join2`
                    BindingKind::FromImport(FromImport { qualified_name }) => {
                        if let Some((target_module, target_member)) = qualified_name.split_once('.')
                        {
                            if target_module == module && target_member == member {
                                if let Some(source) = binding.source {
                                    // Verify that `join` isn't bound in an inner scope.
                                    if self
                                        .scopes()
                                        .take(scope_index)
                                        .all(|scope| !scope.has(name))
                                    {
                                        return Some(ImportedName {
                                            name: (*name).to_string(),
                                            range: self.stmts[source].range(),
                                            context: binding.context,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // Ex) Given `module="os"` and `object="name"`:
                    // `import os.path ` -> `os.name`
                    BindingKind::SubmoduleImport(SubmoduleImport { .. }) => {
                        if name == module {
                            if let Some(source) = binding.source {
                                // Verify that `os` isn't bound in an inner scope.
                                if self
                                    .scopes()
                                    .take(scope_index)
                                    .all(|scope| !scope.has(name))
                                {
                                    return Some(ImportedName {
                                        name: format!("{name}.{member}"),
                                        range: self.stmts[source].range(),
                                        context: binding.context,
                                    });
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

    /// Returns an iterator over all parent statements.
    pub fn parents(&self) -> impl Iterator<Item = &Stmt> + '_ {
        let node_id = self.stmt_id.expect("No current statement");
        self.stmts.ancestor_ids(node_id).map(|id| self.stmts[id])
    }

    /// Set the [`Globals`] for the current [`Scope`].
    pub fn set_globals(&mut self, globals: Globals<'a>) {
        // If any global bindings don't already exist in the global scope, add them.
        for (name, range) in globals.iter() {
            if self
                .global_scope()
                .get(name)
                .map_or(true, |binding_id| self.bindings[binding_id].is_unbound())
            {
                let id = self.bindings.push(Binding {
                    kind: BindingKind::Assignment,
                    range: *range,
                    references: Vec::new(),
                    source: self.stmt_id,
                    context: self.execution_context(),
                    exceptions: self.exceptions(),
                    flags: BindingFlags::empty(),
                });
                self.global_scope_mut().add(name, id);
            }
        }

        self.scopes[self.scope_id].set_globals_id(self.globals.push(globals));
    }

    /// Return the [`TextRange`] at which a name is declared as global in the current [`Scope`].
    pub fn global(&self, name: &str) -> Option<TextRange> {
        let global_id = self.scopes[self.scope_id].globals_id()?;
        self.globals[global_id].get(name).copied()
    }

    /// Given a `name` that has been declared `nonlocal`, return the [`ScopeId`] and [`BindingId`]
    /// to which it refers.
    ///
    /// Unlike `global` declarations, for which the scope is unambiguous, Python requires that
    /// `nonlocal` declarations refer to the closest enclosing scope that contains a binding for
    /// the given name.
    pub fn nonlocal(&self, name: &str) -> Option<(ScopeId, BindingId)> {
        self.scopes
            .ancestor_ids(self.scope_id)
            .skip(1)
            .find_map(|scope_id| {
                let scope = &self.scopes[scope_id];
                if scope.kind.is_module() || scope.kind.is_class() {
                    None
                } else {
                    scope.get(name).map(|binding_id| (scope_id, binding_id))
                }
            })
    }

    /// Return `true` if the given [`ScopeId`] matches that of the current scope.
    pub fn is_current_scope(&self, scope_id: ScopeId) -> bool {
        self.scope_id == scope_id
    }

    /// Return `true` if the model is at the top level of the module (i.e., in the module scope,
    /// and not nested within any statements).
    pub fn at_top_level(&self) -> bool {
        self.scope_id.is_global()
            && self
                .stmt_id
                .map_or(true, |stmt_id| self.stmts.parent_id(stmt_id).is_none())
    }

    /// Return `true` if the model is in an async context.
    pub fn in_async_context(&self) -> bool {
        for scope in self.scopes() {
            if scope.kind.is_async_function() {
                return true;
            }
            if scope.kind.is_function() {
                return false;
            }
        }
        false
    }

    /// Returns `true` if the given [`BindingId`] is used.
    pub fn is_used(&self, binding_id: BindingId) -> bool {
        self.bindings[binding_id].is_used()
    }

    /// Add a reference to the given [`BindingId`] in the local scope.
    pub fn add_local_reference(
        &mut self,
        binding_id: BindingId,
        range: TextRange,
        context: ExecutionContext,
    ) {
        let reference_id = self.references.push(self.scope_id, range, context);
        self.bindings[binding_id].references.push(reference_id);
    }

    /// Add a reference to the given [`BindingId`] in the global scope.
    pub fn add_global_reference(
        &mut self,
        binding_id: BindingId,
        range: TextRange,
        context: ExecutionContext,
    ) {
        let reference_id = self.references.push(ScopeId::global(), range, context);
        self.bindings[binding_id].references.push(reference_id);
    }

    /// Add a [`BindingId`] to the list of delayed annotations for the given [`BindingId`].
    pub fn add_delayed_annotation(&mut self, binding_id: BindingId, annotation_id: BindingId) {
        self.delayed_annotations
            .entry(binding_id)
            .or_insert_with(Vec::new)
            .push(annotation_id);
    }

    /// Return the list of delayed annotations for the given [`BindingId`].
    pub fn delayed_annotations(&self, binding_id: BindingId) -> Option<&[BindingId]> {
        self.delayed_annotations.get(&binding_id).map(Vec::as_slice)
    }

    /// Mark the given [`BindingId`] as rebound in the given [`ScopeId`] (i.e., declared as
    /// `global` or `nonlocal`).
    pub fn add_rebinding_scope(&mut self, binding_id: BindingId, scope_id: ScopeId) {
        self.rebinding_scopes
            .entry(binding_id)
            .or_insert_with(Vec::new)
            .push(scope_id);
    }

    /// Return the list of [`ScopeId`]s in which the given [`BindingId`] is rebound (i.e., declared
    /// as `global` or `nonlocal`).
    pub fn rebinding_scopes(&self, binding_id: BindingId) -> Option<&[ScopeId]> {
        self.rebinding_scopes.get(&binding_id).map(Vec::as_slice)
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
        self.flags.contains(SemanticModelFlags::ANNOTATION)
    }

    /// Return `true` if the context is in a type definition.
    pub const fn in_type_definition(&self) -> bool {
        self.flags.contains(SemanticModelFlags::TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a "simple" string type definition.
    pub const fn in_simple_string_type_definition(&self) -> bool {
        self.flags
            .contains(SemanticModelFlags::SIMPLE_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a "complex" string type definition.
    pub const fn in_complex_string_type_definition(&self) -> bool {
        self.flags
            .contains(SemanticModelFlags::COMPLEX_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in a `__future__` type definition.
    pub const fn in_future_type_definition(&self) -> bool {
        self.flags
            .contains(SemanticModelFlags::FUTURE_TYPE_DEFINITION)
    }

    /// Return `true` if the context is in any kind of deferred type definition.
    pub const fn in_deferred_type_definition(&self) -> bool {
        self.in_simple_string_type_definition()
            || self.in_complex_string_type_definition()
            || self.in_future_type_definition()
    }

    /// Return `true` if the context is in a forward type reference.
    ///
    /// Includes deferred string types, and future types in annotations.
    ///
    /// ## Examples
    /// ```python
    /// from __future__ import annotations
    ///
    /// from threading import Thread
    ///
    ///
    /// x: Thread  # Forward reference
    /// cast("Thread", x)  # Forward reference
    /// cast(Thread, x)  # Non-forward reference
    /// ```
    pub const fn in_forward_reference(&self) -> bool {
        self.in_simple_string_type_definition()
            || self.in_complex_string_type_definition()
            || (self.in_future_type_definition() && self.in_annotation())
    }

    /// Return `true` if the context is in an exception handler.
    pub const fn in_exception_handler(&self) -> bool {
        self.flags.contains(SemanticModelFlags::EXCEPTION_HANDLER)
    }

    /// Return `true` if the context is in an f-string.
    pub const fn in_f_string(&self) -> bool {
        self.flags.contains(SemanticModelFlags::F_STRING)
            || self.flags.contains(SemanticModelFlags::NESTED_F_STRING)
    }

    /// Return `true` if the context is in a nested f-string.
    pub const fn in_nested_f_string(&self) -> bool {
        self.flags.contains(SemanticModelFlags::NESTED_F_STRING)
    }

    /// Return `true` if the context is in boolean test.
    pub const fn in_boolean_test(&self) -> bool {
        self.flags.contains(SemanticModelFlags::BOOLEAN_TEST)
    }

    /// Return `true` if the context is in a `typing::Literal` annotation.
    pub const fn in_literal(&self) -> bool {
        self.flags.contains(SemanticModelFlags::LITERAL)
    }

    /// Return `true` if the context is in a subscript expression.
    pub const fn in_subscript(&self) -> bool {
        self.flags.contains(SemanticModelFlags::SUBSCRIPT)
    }

    /// Return `true` if the context is in a type-checking block.
    pub const fn in_type_checking_block(&self) -> bool {
        self.flags.contains(SemanticModelFlags::TYPE_CHECKING_BLOCK)
    }

    /// Return `true` if the context has traversed past the "top-of-file" import boundary.
    pub const fn seen_import_boundary(&self) -> bool {
        self.flags.contains(SemanticModelFlags::IMPORT_BOUNDARY)
    }

    /// Return `true` if the context has traverse past the `__future__` import boundary.
    pub const fn seen_futures_boundary(&self) -> bool {
        self.flags.contains(SemanticModelFlags::FUTURES_BOUNDARY)
    }

    /// Return `true` if `__future__`-style type annotations are enabled.
    pub const fn future_annotations(&self) -> bool {
        self.flags.contains(SemanticModelFlags::FUTURE_ANNOTATIONS)
    }

    /// Return an iterator over all bindings shadowed by the given [`BindingId`], within the
    /// containing scope, and across scopes.
    pub fn shadowed_bindings(
        &self,
        scope_id: ScopeId,
        binding_id: BindingId,
    ) -> impl Iterator<Item = ShadowedBinding> + '_ {
        let mut first = true;
        let mut binding_id = binding_id;
        std::iter::from_fn(move || {
            // First, check whether this binding is shadowing another binding in a different scope.
            if std::mem::take(&mut first) {
                if let Some(shadowed_id) = self.shadowed_bindings.get(&binding_id).copied() {
                    return Some(ShadowedBinding {
                        binding_id,
                        shadowed_id,
                        same_scope: false,
                    });
                }
            }

            // Otherwise, check whether this binding is shadowing another binding in the same scope.
            if let Some(shadowed_id) = self.scopes[scope_id].shadowed_binding(binding_id) {
                let next = ShadowedBinding {
                    binding_id,
                    shadowed_id,
                    same_scope: true,
                };

                // Advance to the next binding in the scope.
                first = true;
                binding_id = shadowed_id;

                return Some(next);
            }

            None
        })
    }
}

pub struct ShadowedBinding {
    /// The binding that is shadowing another binding.
    binding_id: BindingId,
    /// The binding that is being shadowed.
    shadowed_id: BindingId,
    /// Whether the shadowing and shadowed bindings are in the same scope.
    same_scope: bool,
}

impl ShadowedBinding {
    pub const fn binding_id(&self) -> BindingId {
        self.binding_id
    }

    pub const fn shadowed_id(&self) -> BindingId {
        self.shadowed_id
    }

    pub const fn same_scope(&self) -> bool {
        self.same_scope
    }
}

bitflags! {
    /// Flags indicating the current context of the analysis.
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct SemanticModelFlags: u16 {
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

impl SemanticModelFlags {
    pub fn new(path: &Path) -> Self {
        let mut flags = Self::default();
        if is_python_stub_file(path) {
            flags |= Self::FUTURE_ANNOTATIONS;
        }
        flags
    }
}

/// A snapshot of the [`SemanticModel`] at a given point in the AST traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    scope_id: ScopeId,
    stmt_id: Option<NodeId>,
    definition_id: DefinitionId,
    flags: SemanticModelFlags,
}

#[derive(Debug)]
pub enum ResolvedRead {
    /// The read reference is resolved to a specific binding.
    ///
    /// For example, given:
    /// ```python
    /// x = 1
    /// print(x)
    /// ```
    ///
    /// The `x` in `print(x)` is resolved to the binding of `x` in `x = 1`.
    Resolved(BindingId),

    /// The read reference is resolved to a context-specific, implicit global (e.g., `__class__`
    /// within a class scope).
    ///
    /// For example, given:
    /// ```python
    /// class C:
    ///    print(__class__)
    /// ```
    ///
    /// The `__class__` in `print(__class__)` is resolved to the implicit global `__class__`.
    ImplicitGlobal,

    /// The read reference is unresolved, but at least one of the containing scopes contains a
    /// wildcard import.
    ///
    /// For example, given:
    /// ```python
    /// from x import *
    ///
    /// print(y)
    /// ```
    ///
    /// The `y` in `print(y)` is unresolved, but the containing scope contains a wildcard import,
    /// so `y` _may_ be resolved to a symbol imported by the wildcard import.
    WildcardImport,

    /// The read reference is resolved, but to an unbound local variable.
    ///
    /// For example, given:
    /// ```python
    /// x = 1
    /// del x
    /// print(x)
    /// ```
    ///
    /// The `x` in `print(x)` is an unbound local.
    UnboundLocal(BindingId),

    /// The read reference is definitively unresolved.
    ///
    /// For example, given:
    /// ```python
    /// print(x)
    /// ```
    ///
    /// The `x` in `print(x)` is definitively unresolved.
    NotFound,
}

#[derive(Debug)]
pub struct ImportedName {
    /// The name to which the imported symbol is bound.
    name: String,
    /// The range at which the symbol is imported.
    range: TextRange,
    /// The context in which the symbol is imported.
    context: ExecutionContext,
}

impl ImportedName {
    pub fn into_name(self) -> String {
        self.name
    }

    pub const fn context(&self) -> ExecutionContext {
        self.context
    }
}

impl Ranged for ImportedName {
    fn range(&self) -> TextRange {
        self.range
    }
}
