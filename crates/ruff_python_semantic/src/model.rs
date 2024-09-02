use std::path::Path;

use bitflags::bitflags;
use rustc_hash::FxHashMap;

use ruff_python_ast::helpers::from_relative_import;
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_ast::{self as ast, Expr, ExprContext, Operator, Stmt};
use ruff_python_stdlib::path::is_python_stub_file;
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

pub mod all;

/// A semantic model for a Python module, to enable querying the module's semantic information.
pub struct SemanticModel<'a> {
    typing_modules: &'a [String],
    module: Module<'a>,

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

    /// Modules that have been seen by the semantic model.
    pub seen: Modules,

    /// Exceptions that are handled by the current `try` block.
    ///
    /// For example, if we're visiting the `x = 1` assignment below,
    /// `AttributeError` is considered to be a "handled exception",
    /// but `TypeError` is not:
    ///
    /// ```py
    /// try:
    ///     try:
    ///         foo()
    ///     except TypeError:
    ///         pass
    /// except AttributeError:
    ///     pass
    /// ```
    pub handled_exceptions: Vec<Exceptions>,

    /// Map from [`ast::ExprName`] node (represented as a [`NameId`]) to the [`Binding`] to which
    /// it resolved (represented as a [`BindingId`]).
    resolved_names: FxHashMap<NameId, BindingId>,
}

impl<'a> SemanticModel<'a> {
    pub fn new(typing_modules: &'a [String], path: &Path, module: Module<'a>) -> Self {
        Self {
            typing_modules,
            module,
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
            seen: Modules::empty(),
            handled_exceptions: Vec::default(),
            resolved_names: FxHashMap::default(),
        }
    }

    /// Return the [`Binding`] for the given [`BindingId`].
    #[inline]
    pub fn binding(&self, id: BindingId) -> &Binding<'a> {
        &self.bindings[id]
    }

    /// Resolve the [`ResolvedReference`] for the given [`ResolvedReferenceId`].
    #[inline]
    pub fn reference(&self, id: ResolvedReferenceId) -> &ResolvedReference {
        &self.resolved_references[id]
    }

    /// Return `true` if the `Expr` is a reference to `typing.${target}`.
    pub fn match_typing_expr(&self, expr: &Expr, target: &str) -> bool {
        self.seen_typing()
            && self
                .resolve_qualified_name(expr)
                .is_some_and(|qualified_name| {
                    self.match_typing_qualified_name(&qualified_name, target)
                })
    }

    /// Return `true` if the call path is a reference to `typing.${target}`.
    pub fn match_typing_qualified_name(
        &self,
        qualified_name: &QualifiedName,
        target: &str,
    ) -> bool {
        if matches!(
            qualified_name.segments(),
            ["typing" | "_typeshed" | "typing_extensions", member] if *member == target
        ) {
            return true;
        }

        if self.typing_modules.iter().any(|module| {
            let module = QualifiedName::from_dotted_name(module);
            qualified_name == &module.append_member(target)
        }) {
            return true;
        }

        false
    }

    /// Return an iterator over the set of `typing` modules allowed in the semantic model.
    pub fn typing_modules(&self) -> impl Iterator<Item = &'a str> {
        ["typing", "_typeshed", "typing_extensions"]
            .iter()
            .copied()
            .chain(self.typing_modules.iter().map(String::as_str))
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
    ///
    /// Note that a "builtin binding" does *not* include explicit lookups via the `builtins`
    /// module, e.g. `import builtins; builtins.open`. It *only* includes the bindings
    /// that are pre-populated in Python's global scope before any imports have taken place.
    pub fn has_builtin_binding(&self, member: &str) -> bool {
        self.lookup_symbol(member)
            .map(|binding_id| &self.bindings[binding_id])
            .is_some_and(|binding| binding.kind.is_builtin())
    }

    /// If `expr` is a reference to a builtins symbol,
    /// return the name of that symbol. Else, return `None`.
    ///
    /// This method returns `true` both for "builtin bindings"
    /// (present even without any imports, e.g. `open()`), and for explicit lookups
    /// via the `builtins` module (e.g. `import builtins; builtins.open()`).
    pub fn resolve_builtin_symbol<'expr>(&'a self, expr: &'expr Expr) -> Option<&'a str>
    where
        'expr: 'a,
    {
        // Fast path: we only need to worry about name expressions
        if !self.seen_module(Modules::BUILTINS) {
            let name = &expr.as_name_expr()?.id;
            return if self.has_builtin_binding(name) {
                Some(name)
            } else {
                None
            };
        }

        // Slow path: we have to consider names and attributes
        let qualified_name = self.resolve_qualified_name(expr)?;
        match qualified_name.segments() {
            ["" | "builtins", name] => Some(*name),
            _ => None,
        }
    }

    /// Return `true` if `expr` is a reference to `builtins.$target`,
    /// i.e. either `object` (where `object` is not overridden in the global scope),
    /// or `builtins.object` (where `builtins` is imported as a module at the top level)
    pub fn match_builtin_expr(&self, expr: &Expr, symbol: &str) -> bool {
        debug_assert!(!symbol.contains('.'));
        // fast path with more short-circuiting
        if !self.seen_module(Modules::BUILTINS) {
            let Expr::Name(ast::ExprName { id, .. }) = expr else {
                return false;
            };
            return id == symbol && self.has_builtin_binding(symbol);
        }

        // slow path: we need to consider attribute accesses and aliased imports
        let Some(qualified_name) = self.resolve_qualified_name(expr) else {
            return false;
        };
        matches!(qualified_name.segments(), ["" | "builtins", name] if *name == symbol)
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
                self.add_local_reference(binding_id, ExprContext::Del, range);
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
                    let reference_id = self.resolved_references.push(
                        ScopeId::global(),
                        self.node_id,
                        ExprContext::Load,
                        self.flags,
                        name.range,
                    );
                    self.bindings[binding_id].references.push(reference_id);

                    // Mark any submodule aliases as used.
                    if let Some(binding_id) =
                        self.resolve_submodule(name.id.as_str(), ScopeId::global(), binding_id)
                    {
                        let reference_id = self.resolved_references.push(
                            ScopeId::global(),
                            self.node_id,
                            ExprContext::Load,
                            self.flags,
                            name.range,
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
                let reference_id = self.resolved_references.push(
                    self.scope_id,
                    self.node_id,
                    ExprContext::Load,
                    self.flags,
                    name.range,
                );
                self.bindings[binding_id].references.push(reference_id);

                // Mark any submodule aliases as used.
                if let Some(binding_id) =
                    self.resolve_submodule(name.id.as_str(), scope_id, binding_id)
                {
                    let reference_id = self.resolved_references.push(
                        self.scope_id,
                        self.node_id,
                        ExprContext::Load,
                        self.flags,
                        name.range,
                    );
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
                    //
                    // Stub files are an exception. In a stub file, it _is_ considered valid to
                    // resolve to a type annotation.
                    BindingKind::Annotation if !self.in_stub_file() => continue,

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

                    BindingKind::ConditionalDeletion(binding_id) => {
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
                        let reference_id = self.resolved_references.push(
                            self.scope_id,
                            self.node_id,
                            ExprContext::Load,
                            self.flags,
                            name.range,
                        );
                        self.bindings[binding_id].references.push(reference_id);

                        // Mark any submodule aliases as used.
                        if let Some(binding_id) =
                            self.resolve_submodule(name.id.as_str(), scope_id, binding_id)
                        {
                            let reference_id = self.resolved_references.push(
                                self.scope_id,
                                self.node_id,
                                ExprContext::Load,
                                self.flags,
                                name.range,
                            );
                            self.bindings[binding_id].references.push(reference_id);
                        }

                        self.resolved_names.insert(name.into(), binding_id);
                        return ReadResult::Resolved(binding_id);
                    }

                    BindingKind::Global(Some(binding_id))
                    | BindingKind::Nonlocal(binding_id, _) => {
                        // Mark the shadowed binding as used.
                        let reference_id = self.resolved_references.push(
                            self.scope_id,
                            self.node_id,
                            ExprContext::Load,
                            self.flags,
                            name.range,
                        );
                        self.bindings[binding_id].references.push(reference_id);

                        // Treat it as resolved.
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
                    BindingKind::ConditionalDeletion(binding_id) => return Some(binding_id),
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
    pub fn lookup_attribute(&self, value: &Expr) -> Option<BindingId> {
        let unqualified_name = UnqualifiedName::from_expr(value)?;

        // Find the symbol in the current scope.
        let (symbol, attribute) = unqualified_name.segments().split_first()?;
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
        let call_path = import.qualified_name();
        let segment = call_path.segments().last()?;
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

    /// Resolves the [`ast::ExprName`] to the [`BindingId`] of the symbol it refers to, if it's the
    /// only binding to that name in its scope.
    pub fn only_binding(&self, name: &ast::ExprName) -> Option<BindingId> {
        self.resolve_name(name).filter(|id| {
            let binding = self.binding(*id);
            let scope = &self.scopes[binding.scope];
            scope.shadowed_binding(*id).is_none()
        })
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
    /// ...then `resolve_qualified_name(${python_version})` will resolve to `sys.version_info`.
    pub fn resolve_qualified_name<'name, 'expr: 'name>(
        &self,
        value: &'expr Expr,
    ) -> Option<QualifiedName<'name>>
    where
        'a: 'name,
    {
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
            BindingKind::Import(Import { qualified_name }) => {
                let unqualified_name = UnqualifiedName::from_expr(value)?;
                let (_, tail) = unqualified_name.segments().split_first()?;
                let resolved: QualifiedName = qualified_name
                    .segments()
                    .iter()
                    .chain(tail.iter())
                    .copied()
                    .collect();
                Some(resolved)
            }
            BindingKind::SubmoduleImport(SubmoduleImport { qualified_name }) => {
                let value_name = UnqualifiedName::from_expr(value)?;
                let (_, tail) = value_name.segments().split_first()?;

                Some(
                    qualified_name
                        .segments()
                        .iter()
                        .take(1)
                        .chain(tail.iter())
                        .copied()
                        .collect(),
                )
            }
            BindingKind::FromImport(FromImport { qualified_name }) => {
                let value_name = UnqualifiedName::from_expr(value)?;
                let (_, tail) = value_name.segments().split_first()?;

                let resolved: QualifiedName = if qualified_name
                    .segments()
                    .first()
                    .map_or(false, |segment| *segment == ".")
                {
                    from_relative_import(
                        self.module.qualified_name()?,
                        qualified_name.segments(),
                        tail,
                    )?
                } else {
                    qualified_name
                        .segments()
                        .iter()
                        .chain(tail.iter())
                        .copied()
                        .collect()
                };
                Some(resolved)
            }
            BindingKind::Builtin => {
                if value.is_name_expr() {
                    // Ex) `dict`
                    Some(QualifiedName::builtin(head.id.as_str()))
                } else {
                    // Ex) `dict.__dict__`
                    let value_name = UnqualifiedName::from_expr(value)?;
                    Some(
                        std::iter::once("")
                            .chain(value_name.segments().iter().copied())
                            .collect(),
                    )
                }
            }
            BindingKind::ClassDefinition(_) | BindingKind::FunctionDefinition(_) => {
                // If we have a fully-qualified path for the module, use it.
                if let Some(path) = self.module.qualified_name() {
                    Some(
                        path.iter()
                            .map(String::as_str)
                            .chain(
                                UnqualifiedName::from_expr(value)?
                                    .segments()
                                    .iter()
                                    .copied(),
                            )
                            .collect(),
                    )
                } else {
                    // Otherwise, if we're in (e.g.) a script, use the module name.
                    Some(
                        std::iter::once(self.module.name()?)
                            .chain(
                                UnqualifiedName::from_expr(value)?
                                    .segments()
                                    .iter()
                                    .copied(),
                            )
                            .collect(),
                    )
                }
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
        // TODO(charlie): Pass in a slice.
        let module_path: Vec<&str> = module.split('.').collect();
        self.current_scopes()
            .enumerate()
            .find_map(|(scope_index, scope)| {
                let mut imported_names = scope.bindings().filter_map(|(name, binding_id)| {
                    let binding = &self.bindings[binding_id];
                    match &binding.kind {
                        // Ex) Given `module="sys"` and `object="exit"`:
                        // `import sys`         -> `sys.exit`
                        // `import sys as sys2` -> `sys2.exit`
                        BindingKind::Import(Import { qualified_name }) => {
                            if qualified_name.segments() == module_path.as_slice() {
                                if let Some(source) = binding.source {
                                    // Verify that `sys` isn't bound in an inner scope.
                                    if self
                                        .current_scopes()
                                        .take(scope_index)
                                        .all(|scope| !scope.has(name))
                                    {
                                        return Some(ImportedName {
                                            name: format!("{name}.{member}"),
                                            source,
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
                        BindingKind::FromImport(FromImport { qualified_name }) => {
                            if let Some((target_member, target_module)) =
                                qualified_name.segments().split_last()
                            {
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
                                                name: name.to_string(),
                                                source,
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
                        // Ex) Given `module="os.path"` and `object="join"`:
                        // `import os.path ` -> `os.path.join`
                        BindingKind::SubmoduleImport(SubmoduleImport { qualified_name }) => {
                            if qualified_name.segments().starts_with(&module_path) {
                                if let Some(source) = binding.source {
                                    // Verify that `os` isn't bound in an inner scope.
                                    if self
                                        .current_scopes()
                                        .take(scope_index)
                                        .all(|scope| !scope.has(name))
                                    {
                                        return Some(ImportedName {
                                            name: format!("{module}.{member}"),
                                            source,
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
                });

                let first = imported_names.next()?;
                if let Some(second) = imported_names.next() {
                    // Multiple candidates. We need to sort them because `scope.bindings()` is a HashMap
                    // which doesn't have a stable iteration order.

                    let mut imports: Vec<_> =
                        [first, second].into_iter().chain(imported_names).collect();
                    imports.sort_unstable_by_key(|import| import.range.start());

                    // Return the binding that was imported last.
                    imports.pop()
                } else {
                    Some(first)
                }
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
            .map_while(move |id| self.nodes[id].as_expression())
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

    /// Return the [`NodeId`] of the current [`Stmt`], if any.
    pub fn current_statement_id(&self) -> Option<NodeId> {
        self.current_statement_ids().next()
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
    pub fn current_scopes(&self) -> impl Iterator<Item = &Scope<'a>> {
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

    /// Returns the ID of the parent of the given [`ScopeId`], if any.
    pub fn parent_scope_id(&self, scope_id: ScopeId) -> Option<ScopeId> {
        self.scopes[scope_id].parent
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

    /// Returns the first parent of the given [`ScopeId`] that is not of [`ScopeKind::Type`], if any.
    pub fn first_non_type_parent_scope_id(&self, scope_id: ScopeId) -> Option<ScopeId> {
        let mut current_scope_id = scope_id;
        while let Some(parent_id) = self.parent_scope_id(current_scope_id) {
            if self.scopes[parent_id].kind.is_type() {
                current_scope_id = parent_id;
            } else {
                return Some(parent_id);
            }
        }
        None
    }

    /// Return the [`Stmt`] corresponding to the given [`NodeId`].
    #[inline]
    pub fn node(&self, node_id: NodeId) -> &NodeRef<'a> {
        &self.nodes[node_id]
    }

    /// Given a [`NodeId`], return its parent, if any.
    #[inline]
    pub fn parent_expression(&self, node_id: NodeId) -> Option<&'a Expr> {
        let parent_node_id = self.nodes.ancestor_ids(node_id).nth(1)?;
        self.nodes[parent_node_id].as_expression()
    }

    /// Given a [`NodeId`], return the [`NodeId`] of the parent expression, if any.
    pub fn parent_expression_id(&self, node_id: NodeId) -> Option<NodeId> {
        let parent_node_id = self.nodes.ancestor_ids(node_id).nth(1)?;
        self.nodes[parent_node_id]
            .is_expression()
            .then_some(parent_node_id)
    }

    /// Return the [`Stmt`] corresponding to the given [`NodeId`].
    #[inline]
    pub fn statement(&self, node_id: NodeId) -> &'a Stmt {
        self.nodes
            .ancestor_ids(node_id)
            .find_map(|id| self.nodes[id].as_statement())
            .expect("No statement found")
    }

    /// Returns an [`Iterator`] over the statements, starting from the given [`NodeId`].
    /// through to any parents.
    pub fn statements(&self, node_id: NodeId) -> impl Iterator<Item = &'a Stmt> + '_ {
        self.nodes
            .ancestor_ids(node_id)
            .filter_map(move |id| self.nodes[id].as_statement())
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

    /// Return the [`Expr`] corresponding to the given [`NodeId`].
    #[inline]
    pub fn expression(&self, node_id: NodeId) -> Option<&'a Expr> {
        self.nodes[node_id].as_expression()
    }

    /// Returns an [`Iterator`] over the expressions, starting from the given [`NodeId`].
    /// through to any parents.
    pub fn expressions(&self, node_id: NodeId) -> impl Iterator<Item = &'a Expr> + '_ {
        self.nodes
            .ancestor_ids(node_id)
            .map_while(move |id| self.nodes[id].as_expression())
    }

    /// Mark a Python module as "seen" by the semantic model. Future callers can quickly discount
    /// the need to resolve symbols from these modules if they haven't been seen.
    pub fn add_module(&mut self, module: &str) {
        match module {
            "_typeshed" => self.seen.insert(Modules::TYPESHED),
            "anyio" => self.seen.insert(Modules::ANYIO),
            "builtins" => self.seen.insert(Modules::BUILTINS),
            "collections" => self.seen.insert(Modules::COLLECTIONS),
            "contextvars" => self.seen.insert(Modules::CONTEXTVARS),
            "dataclasses" => self.seen.insert(Modules::DATACLASSES),
            "datetime" => self.seen.insert(Modules::DATETIME),
            "django" => self.seen.insert(Modules::DJANGO),
            "fastapi" => self.seen.insert(Modules::FASTAPI),
            "logging" => self.seen.insert(Modules::LOGGING),
            "mock" => self.seen.insert(Modules::MOCK),
            "numpy" => self.seen.insert(Modules::NUMPY),
            "os" => self.seen.insert(Modules::OS),
            "pandas" => self.seen.insert(Modules::PANDAS),
            "pytest" => self.seen.insert(Modules::PYTEST),
            "re" => self.seen.insert(Modules::RE),
            "six" => self.seen.insert(Modules::SIX),
            "subprocess" => self.seen.insert(Modules::SUBPROCESS),
            "tarfile" => self.seen.insert(Modules::TARFILE),
            "trio" => self.seen.insert(Modules::TRIO),
            "typing" => self.seen.insert(Modules::TYPING),
            "typing_extensions" => self.seen.insert(Modules::TYPING_EXTENSIONS),
            _ => {}
        }
    }

    /// Return `true` if the [`Module`] was "seen" anywhere in the semantic model. This is used as
    /// a fast path to avoid unnecessary work when resolving symbols.
    ///
    /// Callers should still verify that the module is available in the current scope, as visiting
    /// an import of the relevant module _anywhere_ in the file will cause this method to return
    /// `true`.
    pub fn seen_module(&self, module: Modules) -> bool {
        self.seen.intersects(module)
    }

    pub fn seen_typing(&self) -> bool {
        self.seen_module(Modules::TYPING | Modules::TYPESHED | Modules::TYPING_EXTENSIONS)
            || !self.typing_modules.is_empty()
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
        self.globals[global_id].get(name)
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

    /// Return `true` if the model is in a nested literal expression (e.g., the inner `Literal` in
    /// `Literal[Literal[int, str], float]`).
    pub fn in_nested_literal(&self) -> bool {
        // Ex) `Literal[Literal[int, str], float]`
        self.current_expression_grandparent()
            .and_then(Expr::as_subscript_expr)
            .is_some_and(|parent| self.match_typing_expr(&parent.value, "Literal"))
    }

    /// Returns `true` if `left` and `right` are in the same branches of an `if`, `match`, or
    /// `try` statement.
    ///
    /// This implementation assumes that the statements are in the same scope.
    pub fn same_branch(&self, left: NodeId, right: NodeId) -> bool {
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

        left == right
    }

    /// Returns `true` if the given expression is an unused variable, or consists solely of
    /// references to other unused variables. This method is conservative in that it considers a
    /// variable to be "used" if it's shadowed by another variable with usages.
    pub fn is_unused(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Tuple(tuple) => tuple.iter().all(|expr| self.is_unused(expr)),
            Expr::Name(ast::ExprName { id, .. }) => {
                // Treat a variable as used if it has any usages, _or_ it's shadowed by another variable
                // with usages.
                //
                // If we don't respect shadowing, we'll incorrectly flag `bar` as unused in:
                // ```python
                // from random import random
                //
                // for bar in range(10):
                //     if random() > 0.5:
                //         break
                // else:
                //     bar = 1
                //
                // print(bar)
                // ```
                self.current_scope()
                    .get_all(id)
                    .map(|binding_id| self.binding(binding_id))
                    .filter(|binding| binding.start() >= expr.start())
                    .all(Binding::is_unused)
            }
            _ => false,
        }
    }

    /// Add a reference to the given [`BindingId`] in the local scope.
    pub fn add_local_reference(
        &mut self,
        binding_id: BindingId,
        ctx: ExprContext,
        range: TextRange,
    ) {
        let reference_id =
            self.resolved_references
                .push(self.scope_id, self.node_id, ctx, self.flags, range);
        self.bindings[binding_id].references.push(reference_id);
    }

    /// Add a reference to the given [`BindingId`] in the global scope.
    pub fn add_global_reference(
        &mut self,
        binding_id: BindingId,
        ctx: ExprContext,
        range: TextRange,
    ) {
        let reference_id =
            self.resolved_references
                .push(ScopeId::global(), self.node_id, ctx, self.flags, range);
        self.bindings[binding_id].references.push(reference_id);
    }

    /// Add a [`BindingId`] to the list of delayed annotations for the given [`BindingId`].
    pub fn add_delayed_annotation(&mut self, binding_id: BindingId, annotation_id: BindingId) {
        self.delayed_annotations
            .entry(binding_id)
            .or_default()
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
            .or_default()
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

    /// Return `true` if the context is in a runtime-evaluated type annotation.
    pub const fn in_runtime_evaluated_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::RUNTIME_EVALUATED_ANNOTATION)
    }

    /// Return `true` if the context is in a runtime-required type annotation.
    pub const fn in_runtime_required_annotation(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::RUNTIME_REQUIRED_ANNOTATION)
    }

    /// Return `true` if the model is in a type definition.
    pub const fn in_type_definition(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::TYPE_DEFINITION)
    }

    /// Return `true` if the model is visiting a "string type definition"
    /// that was previously deferred when initially traversing the AST
    pub const fn in_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is visiting a "simple string type definition"
    /// that was previously deferred when initially traversing the AST
    pub const fn in_simple_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::SIMPLE_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is visiting a "complex string type definition"
    /// that was previously deferred when initially traversing the AST
    pub const fn in_complex_string_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::COMPLEX_STRING_TYPE_DEFINITION)
    }

    /// Return `true` if the model is visiting a "`__future__` type definition"
    /// that was previously deferred when initially traversing the AST
    pub const fn in_future_type_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::FUTURE_TYPE_DEFINITION)
    }

    /// Return `true` if the model is visiting any kind of type definition
    /// that was previously deferred when initially traversing the AST
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

    /// Return `true` if the model is in an f-string replacement field.
    pub const fn in_f_string_replacement_field(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::F_STRING_REPLACEMENT_FIELD)
    }

    /// Return `true` if the model is in boolean test.
    pub const fn in_boolean_test(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::BOOLEAN_TEST)
    }

    /// Return `true` if the model is in a `typing::Literal` annotation.
    pub const fn in_typing_literal(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::TYPING_LITERAL)
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

    /// Return `true` if the model is in a docstring as described in [PEP 257].
    ///
    /// [PEP 257]: https://peps.python.org/pep-0257/#what-is-a-docstring
    pub const fn in_pep_257_docstring(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::PEP_257_DOCSTRING)
    }

    /// Return `true` if the model is in an attribute docstring.
    pub const fn in_attribute_docstring(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::ATTRIBUTE_DOCSTRING)
    }

    /// Return `true` if the model has traversed past the "top-of-file" import boundary.
    pub const fn seen_import_boundary(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::IMPORT_BOUNDARY)
    }

    /// Return `true` if the model has traverse past the `__future__` import boundary.
    pub const fn seen_futures_boundary(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::FUTURES_BOUNDARY)
    }

    /// Return `true` if the model has traversed past the module docstring boundary.
    pub const fn seen_module_docstring_boundary(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::MODULE_DOCSTRING_BOUNDARY)
    }

    /// Return `true` if `__future__`-style type annotations are enabled.
    pub const fn future_annotations_or_stub(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::FUTURE_ANNOTATIONS_OR_STUB)
    }

    /// Return `true` if the model is in a stub file (i.e., a file with a `.pyi` extension).
    pub const fn in_stub_file(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::STUB_FILE)
    }

    /// Return `true` if the model is in a named expression assignment (e.g., `x := 1`).
    pub const fn in_named_expression_assignment(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::NAMED_EXPRESSION_ASSIGNMENT)
    }

    /// Return `true` if the model is in a comprehension assignment (e.g., `_ for x in y`).
    pub const fn in_comprehension_assignment(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::COMPREHENSION_ASSIGNMENT)
    }

    /// Return `true` if the model is visiting the r.h.s. of an `__all__` definition
    /// (e.g. `"foo"` in `__all__ = ["foo"]`)
    pub const fn in_dunder_all_definition(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::DUNDER_ALL_DEFINITION)
    }

    /// Return `true` if the model is visiting an item in a class's bases tuple
    /// (e.g. `Foo` in `class Bar(Foo): ...`)
    pub const fn in_class_base(&self) -> bool {
        self.flags.intersects(SemanticModelFlags::CLASS_BASE)
    }

    /// Return `true` if the model is visiting an item in a class's bases tuple
    /// that was initially deferred while traversing the AST.
    /// (This only happens in stub files.)
    pub const fn in_deferred_class_base(&self) -> bool {
        self.flags
            .intersects(SemanticModelFlags::DEFERRED_CLASS_BASE)
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
    /// A select list of Python modules that the semantic model can explicitly track.
    #[derive(Debug)]
    pub struct Modules: u32 {
        const COLLECTIONS = 1 << 0;
        const DATETIME = 1 << 1;
        const DJANGO = 1 << 2;
        const LOGGING = 1 << 3;
        const MOCK = 1 << 4;
        const NUMPY = 1 << 5;
        const OS = 1 << 6;
        const PANDAS = 1 << 7;
        const PYTEST = 1 << 8;
        const RE = 1 << 9;
        const SIX = 1 << 10;
        const SUBPROCESS = 1 << 11;
        const TARFILE = 1 << 12;
        const TRIO = 1 << 13;
        const TYPING = 1 << 14;
        const TYPING_EXTENSIONS = 1 << 15;
        const TYPESHED = 1 << 16;
        const DATACLASSES = 1 << 17;
        const BUILTINS = 1 << 18;
        const CONTEXTVARS = 1 << 19;
        const ANYIO = 1 << 20;
        const FASTAPI = 1 << 21;
    }
}

bitflags! {
    /// Flags indicating the current model state.
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct SemanticModelFlags: u32 {
        /// The model is in a type annotation that will only be evaluated when running a type
        /// checker.
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

        /// The model is in a type annotation that will be evaluated at runtime.
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
        const RUNTIME_EVALUATED_ANNOTATION = 1 << 1;

        /// The model is in a type annotation that is _required_ to be available at runtime.
        ///
        /// For example, the context could be visiting `int` in:
        /// ```python
        /// from pydantic import BaseModel
        ///
        /// class Foo(BaseModel):
        ///    x: int
        /// ```
        ///
        /// In this case, Pydantic requires that the type annotation be available at runtime
        /// in order to perform runtime type-checking.
        ///
        /// Unlike [`RUNTIME_EVALUATED_ANNOTATION`], annotations that are marked as
        /// [`RUNTIME_REQUIRED_ANNOTATION`] cannot be deferred to typing time via conversion to a
        /// forward reference (e.g., by wrapping the type in quotes), as the annotations are not
        /// only required by the Python interpreter, but by runtime type checkers too.
        const RUNTIME_REQUIRED_ANNOTATION = 1 << 2;

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
        const TYPE_DEFINITION = 1 << 3;

        /// The model is in a (deferred) "simple" string type definition.
        ///
        /// For example, the model could be visiting `list[int]` in:
        /// ```python
        /// x: "list[int]" = []
        /// ```
        ///
        /// "Simple" string type definitions are those that consist of a single string literal,
        /// as opposed to an implicitly concatenated string literal.
        ///
        /// Note that this flag is only set when we are actually *visiting* the deferred definition,
        /// not when we "pass by" it when initially traversing the source tree.
        const SIMPLE_STRING_TYPE_DEFINITION =  1 << 4;

        /// The model is in a (deferred) "complex" string type definition.
        ///
        /// For example, the model could be visiting `list[int]` in:
        /// ```python
        /// x: ("list" "[int]") = []
        /// ```
        ///
        /// "Complex" string type definitions are those that consist of a implicitly concatenated
        /// string literals. These are uncommon but valid.
        ///
        /// Note that this flag is only set when we are actually *visiting* the deferred definition,
        /// not when we "pass by" it when initially traversing the source tree.
        const COMPLEX_STRING_TYPE_DEFINITION = 1 << 5;

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
        ///
        /// This flag should only be set in contexts where PEP-563 semantics are relevant to
        /// resolution of the type definition. For example, the flag should not be set
        /// in the following context, because the type definition is not inside a type annotation,
        /// so whether or not `from __future__ import annotations` is active has no relevance:
        /// ```python
        /// from __future__ import annotations
        /// from typing import TypeAlias
        ///
        /// X: TypeAlias = list[int]
        /// ```
        ///
        /// Note also that this flag is only set when we are actually *visiting* the deferred definition,
        /// not when we "pass by" it when initially traversing the source tree.
        const FUTURE_TYPE_DEFINITION = 1 << 6;

        /// The model is in an exception handler.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// try:
        ///     ...
        /// except Exception:
        ///     x: int = 1
        /// ```
        const EXCEPTION_HANDLER = 1 << 7;

        /// The model is in an f-string.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// f'{x}'
        /// ```
        const F_STRING = 1 << 8;

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
        const BOOLEAN_TEST = 1 << 9;

        /// The model is in a `typing::Literal` annotation.
        ///
        /// For example, the model could be visiting any of `"A"`, `"B"`, or `"C"` in:
        /// ```python
        /// def f(x: Literal["A", "B", "C"]):
        ///     ...
        /// ```
        const TYPING_LITERAL = 1 << 10;

        /// The model is in a subscript expression.
        ///
        /// For example, the model could be visiting `x["a"]` in:
        /// ```python
        /// x["a"]["b"]
        /// ```
        const SUBSCRIPT = 1 << 11;

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
        const TYPE_CHECKING_BLOCK = 1 << 12;

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
        const IMPORT_BOUNDARY = 1 << 13;

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
        const FUTURES_BOUNDARY = 1 << 14;

        /// The model is in a file that has `from __future__ import annotations`
        /// at the top of the module.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// from __future__ import annotations
        ///
        ///
        /// def f(x: int) -> int:
        ///   ...
        /// ```
        const FUTURE_ANNOTATIONS = 1 << 15;

        /// The model is in a Python stub file (i.e., a `.pyi` file).
        const STUB_FILE = 1 << 16;

        /// `__future__`-style type annotations are enabled in this model.
        /// That could be because it's a stub file,
        /// or it could be because it's a non-stub file that has `from __future__ import annotations`
        /// at the top of the module.
        const FUTURE_ANNOTATIONS_OR_STUB = Self::FUTURE_ANNOTATIONS.bits() | Self::STUB_FILE.bits();

        /// The model has traversed past the module docstring.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// """Module docstring."""
        ///
        /// x: int = 1
        /// ```
        const MODULE_DOCSTRING_BOUNDARY = 1 << 17;

        /// The model is in a (deferred) [type parameter definition].
        ///
        /// For example, the model could be visiting `T`, `P` or `Ts` in:
        /// ```python
        /// class Foo[T, *Ts, **P]: pass
        /// ```
        ///
        /// Note that this flag is *not* set for "pre-PEP-695" TypeVars, ParamSpecs or TypeVarTuples.
        /// None of the following would lead to the flag being set:
        ///
        /// ```python
        /// from typing import TypeVar, ParamSpec, TypeVarTuple
        ///
        /// T = TypeVar("T")
        /// P = ParamSpec("P")
        /// Ts = TypeVarTuple("Ts")
        /// ```
        ///
        /// Note also that this flag is only set when we are actually *visiting* the deferred definition,
        /// not when we "pass by" it when initially traversing the source tree.
        ///
        /// [type parameter definition]: https://docs.python.org/3/reference/executionmodel.html#annotation-scopes
        const TYPE_PARAM_DEFINITION = 1 << 18;

        /// The model is in a named expression assignment.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// if (x := 1): ...
        /// ```
        const NAMED_EXPRESSION_ASSIGNMENT = 1 << 19;

        /// The model is in a comprehension variable assignment.
        ///
        /// For example, the model could be visiting `x` in:
        /// ```python
        /// [_ for x in range(10)]
        /// ```
        const COMPREHENSION_ASSIGNMENT = 1 << 20;

        /// The model is in a docstring as described in [PEP 257].
        ///
        /// For example, the model could be visiting either the module, class,
        /// or function docstring in:
        /// ```python
        /// """Module docstring."""
        ///
        ///
        /// class Foo:
        ///     """Class docstring."""
        ///     pass
        ///
        ///
        /// def foo():
        ///     """Function docstring."""
        ///     pass
        /// ```
        ///
        /// [PEP 257]: https://peps.python.org/pep-0257/#what-is-a-docstring
        const PEP_257_DOCSTRING = 1 << 21;

        /// The model is visiting the r.h.s. of a module-level `__all__` definition.
        ///
        /// This could be any module-level statement that assigns or alters `__all__`,
        /// for example:
        /// ```python
        /// __all__ = ["foo"]
        /// __all__: str = ["foo"]
        /// __all__ = ("bar",)
        /// __all__ += ("baz,")
        /// ```
        const DUNDER_ALL_DEFINITION = 1 << 22;

        /// The model is in an f-string replacement field.
        ///
        /// For example, the model could be visiting `x` or `y` in:
        ///
        /// ```python
        /// f"first {x} second {y}"
        /// ```
        const F_STRING_REPLACEMENT_FIELD = 1 << 23;

        /// The model is visiting the bases tuple of a class.
        ///
        /// For example, the model could be visiting `Foo` or `Bar` in:
        ///
        /// ```python
        /// class Baz(Foo, Bar):
        ///     pass
        /// ```
        const CLASS_BASE = 1 << 24;

        /// The model is visiting a class base that was initially deferred
        /// while traversing the AST. (This only happens in stub files.)
        const DEFERRED_CLASS_BASE = 1 << 25;

        /// The model is in an attribute docstring.
        ///
        /// An attribute docstring is a string literal immediately following an assignment or an
        /// annotated assignment statement. The context in which this is valid are:
        /// 1. At the top level of a module
        /// 2. At the top level of a class definition i.e., a class attribute
        ///
        /// For example:
        /// ```python
        /// a = 1
        /// """This is an attribute docstring for `a` variable"""
        ///
        ///
        /// class Foo:
        ///     b = 1
        ///     """This is an attribute docstring for `Foo.b` class variable"""
        /// ```
        ///
        /// Unlike other kinds of docstrings as described in [PEP 257], attribute docstrings are
        /// discarded at runtime. However, they are used by some documentation renderers and
        /// static-analysis tools.
        ///
        /// [PEP 257]: https://peps.python.org/pep-0257/#what-is-a-docstring
        const ATTRIBUTE_DOCSTRING = 1 << 26;

        /// The context is in any type annotation.
        const ANNOTATION = Self::TYPING_ONLY_ANNOTATION.bits() | Self::RUNTIME_EVALUATED_ANNOTATION.bits() | Self::RUNTIME_REQUIRED_ANNOTATION.bits();

        /// The context is in any string type definition.
        const STRING_TYPE_DEFINITION = Self::SIMPLE_STRING_TYPE_DEFINITION.bits()
            | Self::COMPLEX_STRING_TYPE_DEFINITION.bits();

        /// The context is in any deferred type definition.
        const DEFERRED_TYPE_DEFINITION = Self::SIMPLE_STRING_TYPE_DEFINITION.bits()
            | Self::COMPLEX_STRING_TYPE_DEFINITION.bits()
            | Self::FUTURE_TYPE_DEFINITION.bits()
            | Self::TYPE_PARAM_DEFINITION.bits();

        /// The context is in a typing-only context.
        const TYPING_CONTEXT = Self::TYPE_CHECKING_BLOCK.bits() | Self::TYPING_ONLY_ANNOTATION.bits() |
            Self::STRING_TYPE_DEFINITION.bits() | Self::TYPE_PARAM_DEFINITION.bits();
    }
}

impl SemanticModelFlags {
    pub fn new(path: &Path) -> Self {
        if is_python_stub_file(path) {
            Self::STUB_FILE
        } else {
            Self::default()
        }
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
    /// The statement from which the symbol is imported.
    source: NodeId,
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

    pub fn statement<'a>(&self, semantic: &SemanticModel<'a>) -> &'a Stmt {
        semantic.statement(self.source)
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
