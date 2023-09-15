use std::path::Path;

use bitflags::bitflags;
use rustc_hash::FxHashMap;
use smallvec::smallvec;

use ruff_python_ast::call_path::{collect_call_path, from_unqualified_name, CallPath};
use ruff_python_ast::helpers::from_relative_import;
use ruff_python_ast::{self as ast, Expr, Operator, Stmt};
use ruff_python_stdlib::path::is_python_stub_file;
use ruff_python_stdlib::typing::is_typing_extension;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::binding::{
    Binding, BindingFlags, BindingId, BindingKind, Bindings, Exceptions, FromImport, Import,
    SubmoduleImport,
};
use crate::branches::{BranchId, Branches};
use crate::context::ExecutionContext;
use crate::definition::{Definition, DefinitionId, Definitions, Member, Module};
use crate::globals::{Globals, GlobalsArena};
use crate::nodes::{NodeId, NodeRef, Nodes};
use crate::reference::{
    ResolvedReference, ResolvedReferenceId, ResolvedReferences, UnresolvedReference,
    UnresolvedReferenceFlags, UnresolvedReferences,
};
use crate::scope::{Scope, ScopeId, ScopeKind, Scopes};
use crate::Imported;

/// A semantic model for a Python module, to enable querying the module's semantic information.
pub struct SemanticModel<'a> {
    typing_modules: &'a [String],
    module_path: Option<&'a [String]>,

    /// Stack of all AST nodes in the program.
    nodes: Nodes<'a>,

    /// The ID of the current AST node.
    node_id: Option<NodeId>,

    /// Stack of all branches in the program.
    branches: Branches,

    /// The ID of the current branch.
    branch_id: Option<BranchId>,

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
    resolved_references: ResolvedReferences,

    /// Stack of all unresolved references created in any scope, at any point in execution.
    unresolved_references: UnresolvedReferences,

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
    pub shadowed_bindings: FxHashMap<BindingId, BindingId>,

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
    delayed_annotations: FxHashMap<BindingId, Vec<BindingId>>,

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
    rebinding_scopes: FxHashMap<BindingId, Vec<ScopeId>>,

    /// Flags for the semantic model.
    pub flags: SemanticModelFlags,

    /// Exceptions that have been handled by the current scope.
    pub handled_exceptions: Vec<Exceptions>,

    /// Map from [`ast::ExprName`] node (represented as a [`NameId`]) to the [`Binding`] to which
    /// it resolved (represented as a [`BindingId`]).
    resolved_names: FxHashMap<NameId, BindingId>,
}

impl<'a> SemanticModel<'a> {
    pub fn new(typing_modules: &'a [String], path: &'a Path, module: Module<'a>) -> Self {
        Self {
            typing_modules,
            module_path: module.path(),
            nodes: Nodes::default(),
            node_id: None,
            branches: Branches::default(),
            branch_id: None,
            scopes: Scopes::default(),
            scope_id: ScopeId::global(),
            definitions: Definitions::for_module(module),
            definition_id: DefinitionId::module(),
            bindings: Bindings::default(),
            resolved_references: ResolvedReferences::default(),
            unresolved_references: UnresolvedReferences::default(),
            globals: GlobalsArena::default(),
            shadowed_bindings: FxHashMap::default(),
            delayed_annotations: FxHashMap::default(),
            rebinding_scopes: FxHashMap::default(),
            flags: SemanticModelFlags::new(path),
            handled_exceptions: Vec::default(),
            resolved_names: FxHashMap::default(),
        }
    }

    /// Return the [`Binding`] for the given [`BindingId`].
    #[inline]
    pub fn binding(&self, id: BindingId) -> &Binding {
        &self.bindings[id]
    }

    /// Resolve the [`ResolvedReference`] for the given [`ResolvedReferenceId`].
    #[inline]
    pub fn reference(&self, id: ResolvedReferenceId) -> &ResolvedReference {
        &self.resolved_references[id]
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        self.resolve_call_path(expr)
            .is_some_and(|call_path| self.match_typing_call_path(&call_path, target))
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
            scope: ScopeId::global(),
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
            scope: self.scope_id,
            source: self.node_id,
            context: self.execution_context(),
            exceptions: self.exceptions(),
        })
    }

    /// Return the [`BindingId`] that the given [`BindingId`] shadows, if any.
    ///
    /// Note that this will only return bindings that are shadowed by a binding in a parent scope.
    pub fn shadowed_binding(&self, binding_id: BindingId) -> Option<BindingId> {
        self.shadowed_bindings.get(&binding_id).copied()
    }

    /// Return `true` if `member` is bound as a builtin.
    pub fn is_builtin(&self, member: &str) -> bool {
        self.lookup_symbol(member)
            .map(|binding_id| &self.bindings[binding_id])
            .is_some_and(|binding| binding.kind.is_builtin())
    }

    /// Return `true` if `member` is an "available" symbol, i.e., a symbol that has not been bound
    /// in the current scope, or in any containing scope.
    pub fn is_available(&self, member: &str) -> bool {
        self.lookup_symbol(member)
            .map(|binding_id| &self.bindings[binding_id])
            .map_or(true, |binding| binding.kind.is_builtin())
    }

    /// Resolve a `del` reference to `symbol` at `range`.
    pub fn resolve_del(&mut self, symbol: &str, range: TextRange) {
        let is_unbound = self.scopes[self.scope_id]
            .get(symbol)
            .map_or(true, |binding_id| {
                // Treat the deletion of a name as a reference to that name.
                self.add_local_reference(binding_id, range);
                self.bindings[binding_id].is_unbound()
            });

        // If the binding is unbound, we need to add an unresolved reference.
        if is_unbound {
            self.unresolved_references.push(
                range,
                self.exceptions(),
                UnresolvedReferenceFlags::empty(),
            );
        }
    }

    /// Resolve a `load` reference to an [`ast::ExprName`].
    pub fn resolve_load(&mut self, name: &ast::ExprName) -> ReadResult {
        // PEP 563 indicates that if a forward reference can be resolved in the module scope, we
        // should prefer it over local resolutions.
        if self.in_forward_reference() {
            if let Some(binding_id) = self.scopes.global().get(name.id.as_str()) {
                if !self.bindings[binding_id].is_unbound() {
                    // Mark the binding as used.
                    let reference_id =
                        self.resolved_references
                            .push(ScopeId::global(), name.range, self.flags);
                    self.bindings[binding_id].references.push(reference_id);

                    // Mark any submodule aliases as used.
                    if let Some(binding_id) =
                        self.resolve_submodule(name.id.as_str(), ScopeId::global(), binding_id)
                    {
                        let reference_id = self.resolved_references.push(
                            ScopeId::global(),
                            name.range,
                            self.flags,
                        );
                        self.bindings[binding_id].references.push(reference_id);
                    }

                    self.resolved_names.insert(name.into(), binding_id);
                    return ReadResult::Resolved(binding_id);
                }
            }
        }

        let mut seen_function = false;
        let mut import_starred = false;
        let mut class_variables_visible = true;
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
                if seen_function && matches!(name.id.as_str(), "__class__") {
                    return ReadResult::ImplicitGlobal;
                }
                // Do not allow usages of class symbols unless it is the immediate parent
                // (excluding type scopes), e.g.:
                //
                // ```python
                // class Foo:
                //      a = 0
                //
                //      b = a  # allowed
                //      def c(self, arg=a):  # allowed
                //          print(arg)
                //
                //      def d(self):
                //          print(a)  # not allowed
                // ```
                if !class_variables_visible {
                    continue;
                }
            }

            // Allow class variables to be visible for an additional scope level
            // when a type scope is seen — this covers the type scope present between
            // function and class definitions and their parent class scope.
            class_variables_visible = scope.kind.is_type() && index == 0;

            if let Some(binding_id) = scope.get(name.id.as_str()) {
                // Mark the binding as used.
                let reference_id =
                    self.resolved_references
                        .push(self.scope_id, name.range, self.flags);
                self.bindings[binding_id].references.push(reference_id);

                // Mark any submodule aliases as used.
                if let Some(binding_id) =
                    self.resolve_submodule(name.id.as_str(), scope_id, binding_id)
                {
                    let reference_id =
                        self.resolved_references
                            .push(self.scope_id, name.range, self.flags);
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
                        self.unresolved_references.push(
                            name.range,
                            self.exceptions(),
                            UnresolvedReferenceFlags::empty(),
                        );
                        return ReadResult::UnboundLocal(binding_id);
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
                        let reference_id =
                            self.resolved_references
                                .push(self.scope_id, name.range, self.flags);
                        self.bindings[binding_id].references.push(reference_id);

                        // Mark any submodule aliases as used.
                        if let Some(binding_id) =
                            self.resolve_submodule(name.id.as_str(), scope_id, binding_id)
                        {
                            let reference_id = self.resolved_references.push(
                                self.scope_id,
                                name.range,
                                self.flags,
                            );
                            self.bindings[binding_id].references.push(reference_id);
                        }

                        self.resolved_names.insert(name.into(), binding_id);
                        return ReadResult::Resolved(binding_id);
                    }

                    _ => {
                        // Otherwise, treat it as resolved.
                        self.resolved_names.insert(name.into(), binding_id);
                        return ReadResult::Resolved(binding_id);
                    }
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
                if matches!(name.id.as_str(), "__module__" | "__qualname__") {
                    return ReadResult::ImplicitGlobal;
                }
            }

            seen_function |= scope.kind.is_function();
            import_starred = import_starred || scope.uses_star_imports();
        }

        if import_starred {
            self.unresolved_references.push(
                name.range,
                self.exceptions(),
                UnresolvedReferenceFlags::WILDCARD_IMPORT,
            );
            ReadResult::WildcardImport
        } else {
            self.unresolved_references.push(
                name.range,
                self.exceptions(),
                UnresolvedReferenceFlags::empty(),
            );
            ReadResult::NotFound
        }
    }

    /// Lookup a symbol in the current scope. This is a carbon copy of [`Self::resolve_load`], but
    /// doesn't add any read references to the resolved symbol.
    pub fn lookup_symbol(&self, symbol: &str) -> Option<BindingId> {
        if self.in_forward_reference() {
            if let Some(binding_id) = self.scopes.global().get(symbol) {
                if !self.bindings[binding_id].is_unbound() {
                    return Some(binding_id);
                }
            }
        }

        let mut seen_function = false;
        let mut class_variables_visible = true;
        for (index, scope_id) in self.scopes.ancestor_ids(self.scope_id).enumerate() {
            let scope = &self.scopes[scope_id];
            if scope.kind.is_class() {
                if seen_function && matches!(symbol, "__class__") {
                    return None;
                }
                if !class_variables_visible {
                    continue;
                }
            }

            class_variables_visible = scope.kind.is_type() && index == 0;

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

            seen_function |= scope.kind.is_function();
        }

        None
    }

    /// Lookup a qualified attribute in the current scope.
    ///
    /// For example, given `["Class", "method"`], resolve the `BindingKind::ClassDefinition`
    /// associated with `Class`, then the `BindingKind::FunctionDefinition` associated with
    /// `Class.method`.
    pub fn lookup_attribute(&'a self, value: &'a Expr) -> Option<BindingId> {
        let call_path = collect_call_path(value)?;

        // Find the symbol in the current scope.
        let (symbol, attribute) = call_path.split_first()?;
        let mut binding_id = self.lookup_symbol(symbol)?;

        // Recursively resolve class attributes, e.g., `foo.bar.baz` in.
        let mut tail = attribute;
        while let Some((symbol, rest)) = tail.split_first() {
            // Find the next symbol in the class scope.
            let BindingKind::ClassDefinition(scope_id) = self.binding(binding_id).kind else {
                return None;
            };
            binding_id = self.scopes[scope_id].get(symbol)?;
            tail = rest;
        }

        Some(binding_id)
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
        let import = self.bindings[binding_id].as_any_import()?;
        if !import.is_import() {
            return None;
        }

        // Grab, e.g., `pyarrow` from `import pyarrow as pa`.
        let call_path = import.call_path();
        let segment = call_path.last()?;
        if *segment == symbol {
            return None;
        }

        // Locate the submodule import (e.g., `pyarrow.csv`) that `pa` aliases.
        let binding_id = self.scopes[scope_id].get(segment)?;
        let submodule = &self.bindings[binding_id].as_any_import()?;
        if !submodule.is_submodule_import() {
            return None;
        }

        // Ensure that the submodule import and the aliased import are from the same module.
        if import.module_name() != submodule.module_name() {
            return None;
        }

        Some(binding_id)
    }

    /// Resolves the [`ast::ExprName`] to the [`BindingId`] of the symbol it refers to, if any.
    pub fn resolve_name(&self, name: &ast::ExprName) -> Option<BindingId> {
        self.resolved_names.get(&name.into()).copied()
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
        /// Return the [`ast::ExprName`] at the head of the expression, if any.
        const fn match_head(value: &Expr) -> Option<&ast::ExprName> {
            match value {
                Expr::Attribute(ast::ExprAttribute { value, .. }) => match_head(value),
                Expr::Name(name) => Some(name),
                _ => None,
            }
        }

        // If the name was already resolved, look it up; otherwise, search for the symbol.
        let head = match_head(value)?;
        let binding = self
            .resolve_name(head)
            .or_else(|| self.lookup_symbol(&head.id))
            .map(|id| self.binding(id))?;

        match &binding.kind {
            BindingKind::Import(Import { call_path }) => {
                let value_path = collect_call_path(value)?;
                let (_, tail) = value_path.split_first()?;
                let resolved: CallPath = call_path.iter().chain(tail.iter()).copied().collect();
                Some(resolved)
            }
            BindingKind::SubmoduleImport(SubmoduleImport { call_path }) => {
                let value_path = collect_call_path(value)?;
                let (_, tail) = value_path.split_first()?;
                let resolved: CallPath = call_path
                    .iter()
                    .take(1)
                    .chain(tail.iter())
                    .copied()
                    .collect();
                Some(resolved)
            }
            BindingKind::FromImport(FromImport { call_path }) => {
                let value_path = collect_call_path(value)?;
                let (_, tail) = value_path.split_first()?;

                let resolved: CallPath =
                    if call_path.first().map_or(false, |segment| *segment == ".") {
                        from_relative_import(self.module_path?, call_path, tail)?
                    } else {
                        call_path.iter().chain(tail.iter()).copied().collect()
                    };
                Some(resolved)
            }
            BindingKind::Builtin => Some(smallvec!["", head.id.as_str()]),
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
        // TODO(charlie): Pass in a slice.
        let module_path: Vec<&str> = module.split('.').collect();
        self.current_scopes()
            .enumerate()
            .find_map(|(scope_index, scope)| {
                scope.bindings().find_map(|(name, binding_id)| {
                    let binding = &self.bindings[binding_id];
                    match &binding.kind {
                        // Ex) Given `module="sys"` and `object="exit"`:
                        // `import sys`         -> `sys.exit`
                        // `import sys as sys2` -> `sys2.exit`
                        BindingKind::Import(Import { call_path }) => {
                            if call_path.as_ref() == module_path.as_slice() {
                                if let Some(source) = binding.source {
                                    // Verify that `sys` isn't bound in an inner scope.
                                    if self
                                        .current_scopes()
                                        .take(scope_index)
                                        .all(|scope| !scope.has(name))
                                    {
                                        return Some(ImportedName {
                                            name: format!("{name}.{member}"),
                                            range: self.nodes[source].range(),
                                            context: binding.context,
                                        });
                                    }
                                }
                            }
                        }
                        // Ex) Given `module="os.path"` and `object="join"`:
                        // `from os.path import join`          -> `join`
                        // `from os.path import join as join2` -> `join2`
                        BindingKind::FromImport(FromImport { call_path }) => {
                            if let Some((target_member, target_module)) = call_path.split_last() {
                                if target_module == module_path.as_slice()
                                    && target_member == &member
                                {
                                    if let Some(source) = binding.source {
                                        // Verify that `join` isn't bound in an inner scope.
                                        if self
                                            .current_scopes()
                                            .take(scope_index)
                                            .all(|scope| !scope.has(name))
                                        {
                                            return Some(ImportedName {
                                                name: (*name).to_string(),
                                                range: self.nodes[source].range(),
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
                                        .current_scopes()
                                        .take(scope_index)
                                        .all(|scope| !scope.has(name))
                                    {
                                        return Some(ImportedName {
                                            name: format!("{name}.{member}"),
                                            range: self.nodes[source].range(),
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

    /// Push an AST node [`NodeRef`] onto the stack.
    pub fn push_node<T: Into<NodeRef<'a>>>(&mut self, node: T) {
        self.node_id = Some(self.nodes.insert(node.into(), self.node_id, self.branch_id));
    }

    /// Pop the current AST node [`NodeRef`] off the stack.
    pub fn pop_node(&mut self) {
        let node_id = self.node_id.expect("Attempted to pop without node");
        self.node_id = self.nodes.parent_id(node_id);
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

    /// Push a new branch onto the stack, returning its [`BranchId`].
    pub fn push_branch(&mut self) -> Option<BranchId> {
        self.branch_id = Some(self.branches.insert(self.branch_id));
        self.branch_id
    }

    /// Pop the current [`BranchId`] off the stack.
    pub fn pop_branch(&mut self) {
        let node_id = self.branch_id.expect("Attempted to pop without branch");
        self.branch_id = self.branches.parent_id(node_id);
    }

    /// Set the current [`BranchId`].
    pub fn set_branch(&mut self, branch_id: Option<BranchId>) {
        self.branch_id = branch_id;
    }

    /// Returns an [`Iterator`] over the current statement hierarchy, from the current [`Stmt`]
    /// through to any parents.
    pub fn current_statements(&self) -> impl Iterator<Item = &'a Stmt> + '_ {
        let id = self.node_id.expect("No current node");
        self.nodes
            .ancestor_ids(id)
            .filter_map(move |id| self.nodes[id].as_statement())
    }

    /// Return the current [`Stmt`].
    pub fn current_statement(&self) -> &'a Stmt {
        self.current_statements()
            .next()
            .expect("No current statement")
    }

    /// Return the parent [`Stmt`] of the current [`Stmt`], if any.
    pub fn current_statement_parent(&self) -> Option<&'a Stmt> {
        self.current_statements().nth(1)
    }

    /// Returns an [`Iterator`] over the current expression hierarchy, from the current [`Expr`]
    /// through to any parents.
    pub fn current_expressions(&self) -> impl Iterator<Item = &'a Expr> + '_ {
        let id = self.node_id.expect("No current node");
        self.nodes
            .ancestor_ids(id)
            .filter_map(move |id| self.nodes[id].as_expression())
    }

    /// Return the current [`Expr`].
    pub fn current_expression(&self) -> Option<&'a Expr> {
        self.current_expressions().next()
    }

    /// Return the parent [`Expr`] of the current [`Expr`], if any.
    pub fn current_expression_parent(&self) -> Option<&'a Expr> {
        self.current_expressions().nth(1)
    }

    /// Return the grandparent [`Expr`] of the current [`Expr`], if any.
    pub fn current_expression_grandparent(&self) -> Option<&'a Expr> {
        self.current_expressions().nth(2)
    }

    /// Returns an [`Iterator`] over the current statement hierarchy represented as [`NodeId`],
    /// from the current [`NodeId`] through to any parents.
    pub fn current_statement_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.node_id
            .iter()
            .flat_map(|id| self.nodes.ancestor_ids(*id))
            .filter(|id| self.nodes[*id].is_statement())
    }

    /// Return the [`NodeId`] of the current [`Stmt`].
    pub fn current_statement_id(&self) -> NodeId {
        self.current_statement_ids()
            .next()
            .expect("No current statement")
    }

    /// Return the [`NodeId`] of the current [`Stmt`] parent, if any.
    pub fn current_statement_parent_id(&self) -> Option<NodeId> {
        self.current_statement_ids().nth(1)
    }

    /// Returns a reference to the global [`Scope`].
    pub fn global_scope(&self) -> &Scope<'a> {
        self.scopes.global()
    }

    /// Returns a mutable reference to the global [`Scope`].
    pub fn global_scope_mut(&mut self) -> &mut Scope<'a> {
        self.scopes.global_mut()
    }

    /// Returns the current top-most [`Scope`].
    pub fn current_scope(&self) -> &Scope<'a> {
        &self.scopes[self.scope_id]
    }

    /// Returns a mutable reference to the current top-most [`Scope`].
    pub fn current_scope_mut(&mut self) -> &mut Scope<'a> {
        &mut self.scopes[self.scope_id]
    }

    /// Returns an iterator over all scopes, starting from the current [`Scope`].
    pub fn current_scopes(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.ancestors(self.scope_id)
    }

    /// Returns an iterator over all scopes IDs, starting from the current [`Scope`].
    pub fn current_scope_ids(&self) -> impl Iterator<Item = ScopeId> + '_ {
        self.scopes.ancestor_ids(self.scope_id)
    }

    /// Returns the parent of the given [`Scope`], if any.
    pub fn parent_scope(&self, scope: &Scope) -> Option<&Scope<'a>> {
        scope.parent.map(|scope_id| &self.scopes[scope_id])
    }

    /// Returns the first parent of the given [`Scope`] that is not of [`ScopeKind::Type`], if any.
    pub fn first_non_type_parent_scope(&self, scope: &Scope) -> Option<&Scope<'a>> {
        let mut current_scope = scope;
        while let Some(parent) = self.parent_scope(current_scope) {
            if parent.kind.is_type() {
                current_scope = parent;
            } else {
                return Some(parent);
            }
        }
        None
    }

    /// Return the [`Stmt`] corresponding to the given [`NodeId`].
    #[inline]
    pub fn node(&self, node_id: NodeId) -> &NodeRef<'a> {
        &self.nodes[node_id]
    }

    /// Return the [`Stmt`] corresponding to the given [`NodeId`].
    #[inline]
    pub fn statement(&self, node_id: NodeId) -> &'a Stmt {
        self.nodes
            .ancestor_ids(node_id)
            .find_map(|id| self.nodes[id].as_statement())
            .expect("No statement found")
    }

    /// Given a [`Stmt`], return its parent, if any.
    #[inline]
    pub fn parent_statement(&self, node_id: NodeId) -> Option<&'a Stmt> {
        self.nodes
            .ancestor_ids(node_id)
            .filter_map(|id| self.nodes[id].as_statement())
            .nth(1)
    }

    /// Given a [`NodeId`], return the [`NodeId`] of the parent statement, if any.
    pub fn parent_statement_id(&self, node_id: NodeId) -> Option<NodeId> {
        self.nodes
            .ancestor_ids(node_id)
            .filter(|id| self.nodes[*id].is_statement())
            .nth(1)
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
                    scope: self.scope_id,
                    source: self.node_id,
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
        self.scope_id.is_global() && self.current_statement_parent_id().is_none()
    }

    /// Return `true` if the model is in an async context.
    pub fn in_async_context(&self) -> bool {
        for scope in self.current_scopes() {
            if let ScopeKind::Function(ast::StmtFunctionDef { is_async, .. }) = scope.kind {
                return *is_async;
            }
        }
        false
    }

    /// Return `true` if the model is in a nested union expression (e.g., the inner `Union` in
    /// `Union[Union[int, str], float]`).
    pub fn in_nested_union(&self) -> bool {
        // Ex) `Union[Union[int, str], float]`
        if self
            .current_expression_grandparent()
            .and_then(Expr::as_subscript_expr)
            .is_some_and(|parent| self.match_typing_expr(&parent.value, "Union"))
        {
            return true;
        }

        // Ex) `int | Union[str, float]`
        if self.current_expression_parent().is_some_and(|parent| {
            matches!(
                parent,
                Expr::BinOp(ast::ExprBinOp {
                    op: Operator::BitOr,
                    ..
                })
            )
        }) {
            return true;
        }

        false
    }

    /// Returns `true` if `left` and `right` are on different branches of an `if`, `match`, or
    /// `try` statement.
    ///
    /// This implementation assumes that the statements are in the same scope.
    pub fn different_branches(&self, left: NodeId, right: NodeId) -> bool {
        // Collect the branch path for the left statement.
        let left = self
            .nodes
            .branch_id(left)
            .iter()
            .flat_map(|branch_id| self.branches.ancestor_ids(*branch_id))
            .collect::<Vec<_>>();

        // Collect the branch path for the right statement.
        let right = self
            .nodes
            .branch_id(right)
            .iter()
            .flat_map(|branch_id| self.branches.ancestor_ids(*branch_id))
            .collect::<Vec<_>>();

        !left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| left == right)
    }

    /// Returns `true` if the given [`BindingId`] is used.
    pub fn is_used(&self, binding_id: BindingId) -> bool {
        self.bindings[binding_id].is_used()
    }

    /// Add a reference to the given [`BindingId`] in the local scope.
    pub fn add_local_reference(&mut self, binding_id: BindingId, range: TextRange) {
        let reference_id = self
            .resolved_references
            .push(self.scope_id, range, self.flags);
        self.bindings[binding_id].references.push(reference_id);
    }

    /// Add a reference to the given [`BindingId`] in the global scope.
    pub fn add_global_reference(&mut self, binding_id: BindingId, range: TextRange) {
        let reference_id = self
            .resolved_references
            .push(ScopeId::global(), range, self.flags);
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

    /// Return an iterator over all [`UnresolvedReference`]s in the semantic model.
    pub fn unresolved_references(&self) -> impl Iterator<Item = &UnresolvedReference> {
        self.unresolved_references.iter()
    }

    /// Return the union of all handled exceptions as an [`Exceptions`] bitflag.
    pub fn exceptions(&self) -> Exceptions {
        let mut exceptions = Exceptions::empty();
        for exception in &self.handled_exceptions {
            exceptions.insert(*exception);
        }
        exceptions
    }

    /// Generate a [`Snapshot`] of the current semantic model.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            scope_id: self.scope_id,
            node_id: self.node_id,
            branch_id: self.branch_id,
            definition_id: self.definition_id,
            flags: self.flags,
        }
    }

    /// Restore the semantic model to the given [`Snapshot`].
    pub fn restore(&mut self, snapshot: Snapshot) {
        let Snapshot {
            scope_id,
            node_id,
            branch_id,
            definition_id,
            flags,
        } = snapshot;
        self.scope_id = scope_id;
        self.node_id = node_id;
        self.branch_id = branch_id;
        self.definition_id = definition_id;
        self.flags = flags;
    }

    /// Return the [`ExecutionContext`] of the current scope.
    pub const fn execution_context(&self) -> ExecutionContext {
        if self.flags.intersects(SemanticModelFlags::TYPING_CONTEXT) {
            ExecutionContext::Typing
        } else {
            ExecutionContext::Runtime
        }
    }

    /// Return `true` if the model is in a type annotation.
    pub const fn in_annotation(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::ANNOTATION)
    }

    /// Return `true` if the model is in a typing-only type annotation.
    pub const fn in_typing_only_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::TYPING_ONLY_ANNOTATION)
    }

    /// Return `true` if the model is in a runtime-required type annotation.
    pub const fn in_runtime_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::RUNTIME_ANNOTATION)
    }

    /// Return `true` if the model is in a type definition.
    pub const fn in_type_definition(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::TYPE_DEFINITION)
    }

    /// Return `true` if the model is in a string type definition.
    pub const fn in_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is in a "simple" string type definition.
    pub const fn in_simple_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::SIMPLE_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is in a "complex" string type definition.
    pub const fn in_complex_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::COMPLEX_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is in a `__future__` type definition.
    pub const fn in_future_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::FUTURE_TYPE_DEFINITION)
    }

    /// Return `true` if the model is in any kind of deferred type definition.
    pub const fn in_deferred_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::DEFERRED_TYPE_DEFINITION)
    }

    /// Return `true` if the model is in a forward type reference.
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
        self.in_string_type_definition()
            || (self.in_future_type_definition() && self.in_typing_only_annotation())
    }

    /// Return `true` if the model is in an exception handler.
    pub const fn in_exception_handler(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::EXCEPTION_HANDLER)
    }

    /// Return `true` if the model is in an f-string.
    pub const fn in_f_string(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::F_STRING)
    }

    /// Return `true` if the model is in boolean test.
    pub const fn in_boolean_test(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::BOOLEAN_TEST)
    }

    /// Return `true` if the model is in a `typing::Literal` annotation.
    pub const fn in_literal(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::LITERAL)
    }

    /// Return `true` if the model is in a subscript expression.
    pub const fn in_subscript(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::SUBSCRIPT)
    }

    /// Return `true` if the model is in a type-checking block.
    pub const fn in_type_checking_block(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::TYPE_CHECKING_BLOCK)
    }

    /// Return `true` if the model has traversed past the "top-of-file" import boundary.
    pub const fn seen_import_boundary(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::IMPORT_BOUNDARY)
    }

    /// Return `true` if the model has traverse past the `__future__` import boundary.
    pub const fn seen_futures_boundary(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::FUTURES_BOUNDARY)
    }

    /// Return `true` if `__future__`-style type annotations are enabled.
    pub const fn future_annotations(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::FUTURE_ANNOTATIONS)
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
    /// Flags indicating the current model state.
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct SemanticModelFlags: u16 {
        /// The model is in a typing-time-only type annotation.
        ///
        /// For example, the model could be visiting `int` in:
        /// ```python
        /// def foo() -> int:
        ///     x: int = 1
        /// ```
        ///
        /// In this case, Python doesn't require that the type annotation be evaluated at runtime.
        ///
        /// If `from __future__ import annotations` is used, all annotations are evaluated at
        /// typing time. Otherwise, all function argument annotations are evaluated at runtime, as
        /// are any annotated assignments in module or class scopes.
        const TYPING_ONLY_ANNOTATION = 1 << 0;

        /// The model is in a runtime type annotation.
        ///
        /// For example, the model could be visiting `int` in:
        /// ```python
        /// def foo(x: int) -> int:
        ///     ...
        /// ```
        ///
        /// In this case, Python requires that the type annotation be evaluated at runtime,
        /// as it needs to be available on the function's `__annotations__` attribute.
        ///
        /// If `from __future__ import annotations` is used, all annotations are evaluated at
        /// typing time. Otherwise, all function argument annotations are evaluated at runtime, as
        /// are any annotated assignments in module or class scopes.
        const RUNTIME_ANNOTATION = 1 << 1;

        /// The model is in a type definition.
        ///
        /// For example, the model could be visiting `int` in:
        /// ```python
        /// from typing import NewType
        ///
        /// UserId = NewType("UserId", int)
        /// ```
        ///
        /// All type annotations are also type definitions, but the converse is not true.
        /// In our example, `int` is a type definition but not a type annotation, as it
        /// doesn't appear in a type annotation context, but rather in a type definition.
        const TYPE_DEFINITION = 1 << 2;

        /// The model is in a (deferred) "simple" string type definition.
        ///
        /// For example, the model could be visiting `list[int]` in:
        /// ```python
        /// x: "list[int]" = []
        /// ```
        ///
        /// "Simple" string type definitions are those that consist of a single string literal,
        /// as opposed to an implicitly concatenated string literal.
        const SIMPLE_STRING_TYPE_DEFINITION =  1 << 3;

        /// The model is in a (deferred) "complex" string type definition.
        ///
        /// For example, the model could be visiting `list[int]` in:
        /// ```python
        /// x: ("list" "[int]") = []
        /// ```
        ///
        /// "Complex" string type definitions are those that consist of a implicitly concatenated
        /// string literals. These are uncommon but valid.
        const COMPLEX_STRING_TYPE_DEFINITION = 1 << 4;

        /// The model is in a (deferred) `__future__` type definition.
        ///
        /// For example, the model could be visiting `list[int]` in:
        /// ```python
        /// from __future__ import annotations
        ///
        /// x: list[int] = []
        /// ```
        ///
        /// `__future__`-style type annotations are only enabled if the `annotations` feature
        /// is enabled via `from __future__ import annotations`.
        const FUTURE_TYPE_DEFINITION = 1 << 5;

        /// The model is in an exception handler.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// try:
        ///     ...
        /// except Exception:
        ///     x: int = 1
        /// ```
        const EXCEPTION_HANDLER = 1 << 6;

        /// The model is in an f-string.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// f'{x}'
        /// ```
        const F_STRING = 1 << 7;

        /// The model is in a boolean test.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// if x:
        ///     ...
        /// ```
        ///
        /// The implication is that the actual value returned by the current expression is
        /// not used, only its truthiness.
        const BOOLEAN_TEST = 1 << 8;

        /// The model is in a `typing::Literal` annotation.
        ///
        /// For example, the model could be visiting any of `"A"`, `"B"`, or `"C"` in:
        /// ```python
        /// def f(x: Literal["A", "B", "C"]):
        ///     ...
        /// ```
        const LITERAL = 1 << 9;

        /// The model is in a subscript expression.
        ///
        /// For example, the model could be visiting `x["a"]` in:
        /// ```python
        /// x["a"]["b"]
        /// ```
        const SUBSCRIPT = 1 << 10;

        /// The model is in a type-checking block.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// from typing import TYPE_CHECKING
        ///
        ///
        /// if TYPE_CHECKING:
        ///    x: int = 1
        /// ```
        const TYPE_CHECKING_BLOCK = 1 << 11;

        /// The model has traversed past the "top-of-file" import boundary.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// import os
        ///
        /// def f() -> None:
        ///     ...
        ///
        /// x: int = 1
        /// ```
        const IMPORT_BOUNDARY = 1 << 12;

        /// The model has traversed past the `__future__` import boundary.
        ///
        /// For example, the model could be visiting `x` in:
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

        /// `__future__`-style type annotations are enabled in this model.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// from __future__ import annotations
        ///
        ///
        /// def f(x: int) -> int:
        ///   ...
        /// ```
        const FUTURE_ANNOTATIONS = 1 << 14;

        /// The context is in any type annotation.
        const ANNOTATION = Self::TYPING_ONLY_ANNOTATION.bits() | Self::RUNTIME_ANNOTATION.bits();

        /// The context is in any string type definition.
        const STRING_TYPE_DEFINITION = Self::SIMPLE_STRING_TYPE_DEFINITION.bits()
            | Self::COMPLEX_STRING_TYPE_DEFINITION.bits();

        /// The context is in any deferred type definition.
        const DEFERRED_TYPE_DEFINITION = Self::SIMPLE_STRING_TYPE_DEFINITION.bits()
            | Self::COMPLEX_STRING_TYPE_DEFINITION.bits()
            | Self::FUTURE_TYPE_DEFINITION.bits();

        /// The context is in a typing-only context.
        const TYPING_CONTEXT = Self::TYPE_CHECKING_BLOCK.bits() | Self::TYPING_ONLY_ANNOTATION.bits() |
            Self::STRING_TYPE_DEFINITION.bits();
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
    node_id: Option<NodeId>,
    branch_id: Option<BranchId>,
    definition_id: DefinitionId,
    flags: SemanticModelFlags,
}

#[derive(Debug)]
pub enum ReadResult {
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

/// A unique identifier for an [`ast::ExprName`]. No two names can even appear at the same location
/// in the source code, so the starting offset is a cheap and sufficient unique identifier.
#[derive(Debug, Hash, PartialEq, Eq)]
struct NameId(TextSize);

impl From<&ast::ExprName> for NameId {
    fn from(name: &ast::ExprName) -> Self {
        Self(name.start())
    }
}
