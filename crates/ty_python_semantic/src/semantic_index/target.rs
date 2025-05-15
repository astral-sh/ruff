use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::visibility_constraints::ScopedVisibilityConstraintId;
use crate::semantic_index::{semantic_index, SemanticIndex, TargetSet};
use crate::Db;

#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update)]
pub(crate) enum TargetSubSegment {
    /// A member access, e.g. `y` in `x.y`.
    Member(ast::name::Name),
    /// An integer index access, e.g. `1` in `x[1]`.
    IntSubscript(ast::Int),
    /// A string index access, e.g. `"foo"` in `x["foo"]`.
    StringSubscript(String),
}

impl TargetSubSegment {
    fn as_member(&self) -> Option<&ast::name::Name> {
        match self {
            TargetSubSegment::Member(name) => Some(name),
            _ => None,
        }
    }
}

/// A single target value that can be the left side of a `Definition`.
/// If you want to perform a comparison based on the equality of segments (without including flags), use [`TargetSegments`].
#[derive(Eq, PartialEq, Debug)]
pub struct Target {
    root_name: Name,
    sub_segments: Vec<TargetSubSegment>,
    flags: TargetFlags,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.root_name)?;
        for segment in &self.sub_segments {
            match segment {
                TargetSubSegment::Member(name) => write!(f, ".{name}")?,
                TargetSubSegment::IntSubscript(int) => write!(f, "[{int}]")?,
                TargetSubSegment::StringSubscript(string) => write!(f, "[\"{string}\"]")?,
            }
        }
        Ok(())
    }
}

impl TryFrom<&ast::name::Name> for Target {
    type Error = Infallible;

    fn try_from(name: &ast::name::Name) -> Result<Self, Infallible> {
        Ok(Target::name(name.clone()))
    }
}

impl TryFrom<ast::name::Name> for Target {
    type Error = Infallible;

    fn try_from(name: ast::name::Name) -> Result<Self, Infallible> {
        Ok(Target::name(name))
    }
}

impl TryFrom<&ast::ExprAttribute> for Target {
    type Error = ();

    fn try_from(attr: &ast::ExprAttribute) -> Result<Self, ()> {
        let mut target = Target::try_from(&*attr.value)?;
        target
            .sub_segments
            .push(TargetSubSegment::Member(attr.attr.id.clone()));
        Ok(target)
    }
}

impl TryFrom<ast::ExprAttribute> for Target {
    type Error = ();

    fn try_from(attr: ast::ExprAttribute) -> Result<Self, ()> {
        let mut target = Target::try_from(&*attr.value)?;
        target
            .sub_segments
            .push(TargetSubSegment::Member(attr.attr.id));
        Ok(target)
    }
}

impl TryFrom<&ast::ExprSubscript> for Target {
    type Error = ();

    fn try_from(subscript: &ast::ExprSubscript) -> Result<Self, ()> {
        let mut target = Target::try_from(&*subscript.value)?;
        match &*subscript.slice {
            ast::Expr::NumberLiteral(number) if number.value.is_int() => {
                target.sub_segments.push(TargetSubSegment::IntSubscript(
                    number.value.as_int().unwrap().clone(),
                ));
            }
            ast::Expr::StringLiteral(string) => {
                target
                    .sub_segments
                    .push(TargetSubSegment::StringSubscript(string.value.to_string()));
            }
            _ => {
                return Err(());
            }
        }
        Ok(target)
    }
}

impl TryFrom<ast::ExprSubscript> for Target {
    type Error = ();

    fn try_from(subscript: ast::ExprSubscript) -> Result<Self, ()> {
        Target::try_from(&subscript)
    }
}

impl TryFrom<&ast::Expr> for Target {
    type Error = ();

    fn try_from(expr: &ast::Expr) -> Result<Self, ()> {
        match expr {
            ast::Expr::Name(name) => Ok(Target::name(name.id.clone())),
            ast::Expr::Attribute(attr) => Target::try_from(attr),
            ast::Expr::Subscript(subscript) => Target::try_from(subscript),
            _ => Err(()),
        }
    }
}

impl Target {
    pub(super) fn name(name: Name) -> Self {
        Self {
            root_name: name,
            sub_segments: Vec::new(),
            flags: TargetFlags::empty(),
        }
    }

    fn insert_flags(&mut self, flags: TargetFlags) {
        self.flags.insert(flags);
    }

    pub(super) fn mark_instance_attribute(&mut self) {
        self.flags.insert(TargetFlags::IS_INSTANCE_ATTRIBUTE);
    }

    pub(crate) fn root_name(&self) -> &Name {
        &self.root_name
    }

    pub(crate) fn sub_segments(&self) -> &[TargetSubSegment] {
        &self.sub_segments
    }

    pub(crate) fn as_name(&self) -> Option<&Name> {
        if self.is_name() {
            Some(&self.root_name)
        } else {
            None
        }
    }

    /// The target's name.
    #[track_caller]
    pub(crate) fn expect_name(&self) -> &Name {
        debug_assert_eq!(self.sub_segments, vec![]);
        &self.root_name
    }

    pub(super) fn is_instance_attribute_named(&self, name: &str) -> bool {
        self.flags.contains(TargetFlags::IS_INSTANCE_ATTRIBUTE)
            && self.sub_segments.len() == 1
            && self.sub_segments[0].as_member().unwrap().as_str() == name
    }

    pub fn is_instance_attribute(&self) -> bool {
        self.flags.contains(TargetFlags::IS_INSTANCE_ATTRIBUTE)
    }

    /// Is the target used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(TargetFlags::IS_USED)
    }

    /// Is the target defined in its containing scope?
    pub fn is_bound(&self) -> bool {
        self.flags.contains(TargetFlags::IS_BOUND)
    }

    /// Is the target declared in its containing scope?
    pub fn is_declared(&self) -> bool {
        self.flags.contains(TargetFlags::IS_DECLARED)
    }

    pub fn is_name(&self) -> bool {
        self.sub_segments.is_empty()
    }

    pub fn is_member(&self) -> bool {
        self.sub_segments
            .last()
            .is_some_and(|last| last.as_member().is_some())
    }

    pub(crate) fn segments(&self) -> TargetSegments {
        TargetSegments {
            root_name: Some(&self.root_name),
            sub_segments: &self.sub_segments,
        }
    }
}

bitflags! {
    /// Flags that can be queried to obtain information about a target in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a target to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct TargetFlags: u8 {
        const IS_USED               = 1 << 0;
        const IS_BOUND              = 1 << 1;
        const IS_DECLARED           = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL         = 1 << 3;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL       = 1 << 4;
        const IS_INSTANCE_ATTRIBUTE = 1 << 5;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSegment<'db> {
    Name(&'db ast::name::Name),
    Member(&'db ast::name::Name),
    IntSubscript(&'db ast::Int),
    StringSubscript(&'db str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct TargetSegments<'db> {
    root_name: Option<&'db ast::name::Name>,
    sub_segments: &'db [TargetSubSegment],
}

impl<'db> Iterator for TargetSegments<'db> {
    type Item = TargetSegment<'db>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(name) = self.root_name.take() {
            return Some(TargetSegment::Name(name));
        }
        if self.sub_segments.is_empty() {
            return None;
        }
        let segment = &self.sub_segments[0];
        self.sub_segments = &self.sub_segments[1..];
        Some(match segment {
            TargetSubSegment::Member(name) => TargetSegment::Member(name),
            TargetSubSegment::IntSubscript(int) => TargetSegment::IntSubscript(int),
            TargetSubSegment::StringSubscript(string) => TargetSegment::StringSubscript(string),
        })
    }
}

/// ID that uniquely identifies a symbol in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FileTargetId {
    scope: FileScopeId,
    scoped_target_id: ScopedTargetId,
}

impl FileTargetId {
    pub fn scope(self) -> FileScopeId {
        self.scope
    }

    pub(crate) fn scoped_target_id(self) -> ScopedTargetId {
        self.scoped_target_id
    }
}

impl From<FileTargetId> for ScopedTargetId {
    fn from(val: FileTargetId) -> Self {
        val.scoped_target_id()
    }
}

/// ID that uniquely identifies a target inside a [`Scope`].
#[newtype_index]
#[derive(salsa::Update)]
pub struct ScopedTargetId;

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked(debug)]
pub struct ScopeId<'db> {
    pub file: File,

    pub file_scope_id: FileScopeId,

    count: countme::Count<ScopeId<'static>>,
}

impl<'db> ScopeId<'db> {
    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_function_like()
    }

    pub(crate) fn is_type_parameter(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_type_parameter()
    }

    pub(crate) fn node(self, db: &dyn Db) -> &NodeWithScopeKind {
        self.scope(db).node()
    }

    pub(crate) fn scope(self, db: &dyn Db) -> &Scope {
        semantic_index(db, self.file(db)).scope(self.file_scope_id(db))
    }

    #[cfg(test)]
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self.node(db) {
            NodeWithScopeKind::Module => "<module>",
            NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
                class.name.as_str()
            }
            NodeWithScopeKind::Function(function)
            | NodeWithScopeKind::FunctionTypeParameters(function) => function.name.as_str(),
            NodeWithScopeKind::TypeAlias(type_alias)
            | NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => type_alias
                .name
                .as_name_expr()
                .map(|name| name.id.as_str())
                .unwrap_or("<type alias>"),
            NodeWithScopeKind::Lambda(_) => "<lambda>",
            NodeWithScopeKind::ListComprehension(_) => "<listcomp>",
            NodeWithScopeKind::SetComprehension(_) => "<setcomp>",
            NodeWithScopeKind::DictComprehension(_) => "<dictcomp>",
            NodeWithScopeKind::GeneratorExpression(_) => "<generator>",
        }
    }
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
#[derive(salsa::Update)]
pub struct FileScopeId;

impl FileScopeId {
    /// Returns the scope id of the module-global scope.
    pub fn global() -> Self {
        FileScopeId::from_u32(0)
    }

    pub fn is_global(self) -> bool {
        self == FileScopeId::global()
    }

    pub fn to_scope_id(self, db: &dyn Db, file: File) -> ScopeId<'_> {
        let index = semantic_index(db, file);
        index.scope_ids_by_scope[self]
    }

    pub(crate) fn is_generator_function(self, index: &SemanticIndex) -> bool {
        index.generator_functions.contains(&self)
    }
}

#[derive(Debug, salsa::Update)]
pub struct Scope {
    parent: Option<FileScopeId>,
    node: NodeWithScopeKind,
    descendants: Range<FileScopeId>,
    reachability: ScopedVisibilityConstraintId,
}

impl Scope {
    pub(super) fn new(
        parent: Option<FileScopeId>,
        node: NodeWithScopeKind,
        descendants: Range<FileScopeId>,
        reachability: ScopedVisibilityConstraintId,
    ) -> Self {
        Scope {
            parent,
            node,
            descendants,
            reachability,
        }
    }

    pub fn parent(&self) -> Option<FileScopeId> {
        self.parent
    }

    pub fn node(&self) -> &NodeWithScopeKind {
        &self.node
    }

    pub fn kind(&self) -> ScopeKind {
        self.node().scope_kind()
    }

    pub fn descendants(&self) -> Range<FileScopeId> {
        self.descendants.clone()
    }

    pub(super) fn extend_descendants(&mut self, children_end: FileScopeId) {
        self.descendants = self.descendants.start..children_end;
    }

    pub(crate) fn is_eager(&self) -> bool {
        self.kind().is_eager()
    }

    pub(crate) fn reachability(&self) -> ScopedVisibilityConstraintId {
        self.reachability
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
    Lambda,
    Comprehension,
    TypeAlias,
}

impl ScopeKind {
    pub(crate) fn is_eager(self) -> bool {
        match self {
            ScopeKind::Module | ScopeKind::Class | ScopeKind::Comprehension => true,
            ScopeKind::Annotation
            | ScopeKind::Function
            | ScopeKind::Lambda
            | ScopeKind::TypeAlias => false,
        }
    }

    pub(crate) fn is_function_like(self) -> bool {
        // Type parameter scopes behave like function scopes in terms of name resolution; CPython
        // symbol table also uses the term "function-like" for these scopes.
        matches!(
            self,
            ScopeKind::Annotation
                | ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::TypeAlias
                | ScopeKind::Comprehension
        )
    }

    pub(crate) fn is_class(self) -> bool {
        matches!(self, ScopeKind::Class)
    }

    pub(crate) fn is_type_parameter(self) -> bool {
        matches!(self, ScopeKind::Annotation | ScopeKind::TypeAlias)
    }
}

/// [`Target`] table for a specific [`Scope`].
#[derive(Default, salsa::Update)]
pub struct TargetTable {
    /// The targets in this scope.
    targets: IndexVec<ScopedTargetId, Target>,

    /// The set of targets.
    target_set: TargetSet,
}

impl TargetTable {
    fn shrink_to_fit(&mut self) {
        self.targets.shrink_to_fit();
    }

    pub(crate) fn target(&self, target_id: impl Into<ScopedTargetId>) -> &Target {
        &self.targets[target_id.into()]
    }

    #[expect(unused)]
    pub(crate) fn target_ids(&self) -> impl Iterator<Item = ScopedTargetId> {
        self.targets.indices()
    }

    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.targets.iter()
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Target> {
        self.targets().filter(|target| target.is_name())
    }

    /// Returns the symbol named `name`.
    #[allow(unused)]
    pub(crate) fn target_by_name(&self, name: &str) -> Option<&Target> {
        let id = self.target_id_by_name(name)?;
        Some(self.target(id))
    }

    pub(crate) fn marked_target(&self, target: &Target) -> Option<&Target> {
        let id = self.target_id_by_target(target)?;
        Some(self.target(id))
    }

    /// Returns the [`ScopedTargetId`] of the target named `name`.
    pub(crate) fn target_id_by_name(&self, name: &str) -> Option<ScopedTargetId> {
        let (id, ()) = self
            .target_set
            .raw_entry()
            .from_hash(Self::hash_name(name), |id| {
                self.target(*id).as_name().map(Name::as_str) == Some(name)
            })?;

        Some(*id)
    }

    /// Returns the [`ScopedTargetId`] of the target.
    pub(crate) fn target_id_by_target(&self, target: &Target) -> Option<ScopedTargetId> {
        let (id, ()) = self
            .target_set
            .raw_entry()
            .from_hash(Self::hash_target(target), |id| {
                self.target(*id).segments() == target.segments()
            })?;

        Some(*id)
    }

    pub(crate) fn target_id_by_instance_attribute_name(
        &self,
        name: &str,
    ) -> Option<ScopedTargetId> {
        self.targets
            .indices()
            .find(|id| self.targets[*id].is_instance_attribute_named(name))
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }

    fn hash_target(target: &Target) -> u64 {
        let mut hasher = FxHasher::default();
        target.root_name().as_str().hash(&mut hasher);
        for segment in &target.sub_segments {
            match segment {
                TargetSubSegment::Member(name) => name.hash(&mut hasher),
                TargetSubSegment::IntSubscript(int) => int.hash(&mut hasher),
                TargetSubSegment::StringSubscript(string) => string.hash(&mut hasher),
            }
        }
        hasher.finish()
    }
}

impl PartialEq for TargetTable {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the target_set because the target is already captured in `Target`.
        self.targets == other.targets
    }
}

impl Eq for TargetTable {}

impl std::fmt::Debug for TargetTable {
    /// Exclude the `target_set` field from the debug output.
    /// It's very noisy and not useful for debugging.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TargetTable")
            .field(&self.targets)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(super) struct TargetTableBuilder {
    table: TargetTable,
}

impl TargetTableBuilder {
    pub(super) fn add_symbol(&mut self, name: Name) -> (ScopedTargetId, bool) {
        let hash = TargetTable::hash_name(&name);
        let entry = self
            .table
            .target_set
            .raw_entry_mut()
            .from_hash(hash, |id| self.table.targets[*id].as_name() == Some(&name));

        match entry {
            RawEntryMut::Occupied(entry) => (*entry.key(), false),
            RawEntryMut::Vacant(entry) => {
                let symbol = Target::name(name);

                let id = self.table.targets.push(symbol);
                entry.insert_with_hasher(hash, id, (), |id| {
                    TargetTable::hash_target(&self.table.targets[*id])
                });
                (id, true)
            }
        }
    }

    pub(super) fn add_target(&mut self, target: Target) -> (ScopedTargetId, bool) {
        let hash = TargetTable::hash_target(&target);
        let entry = self.table.target_set.raw_entry_mut().from_hash(hash, |id| {
            self.table.targets[*id].segments() == target.segments()
        });

        match entry {
            RawEntryMut::Occupied(entry) => (*entry.key(), false),
            RawEntryMut::Vacant(entry) => {
                let id = self.table.targets.push(target);
                entry.insert_with_hasher(hash, id, (), |id| {
                    TargetTable::hash_target(&self.table.targets[*id])
                });
                (id, true)
            }
        }
    }

    pub(super) fn mark_target_bound(&mut self, id: ScopedTargetId) {
        self.table.targets[id].insert_flags(TargetFlags::IS_BOUND);
    }

    pub(super) fn mark_target_declared(&mut self, id: ScopedTargetId) {
        self.table.targets[id].insert_flags(TargetFlags::IS_DECLARED);
    }

    pub(super) fn mark_target_used(&mut self, id: ScopedTargetId) {
        self.table.targets[id].insert_flags(TargetFlags::IS_USED);
    }

    pub(super) fn targets(&self) -> impl Iterator<Item = &Target> {
        self.table.targets()
    }

    pub(super) fn target_id_by_target(&self, target: &Target) -> Option<ScopedTargetId> {
        self.table.target_id_by_target(target)
    }

    pub(super) fn target(&self, target_id: impl Into<ScopedTargetId>) -> &Target {
        self.table.target(target_id)
    }

    pub(super) fn finish(mut self) -> TargetTable {
        self.table.shrink_to_fit();
        self.table
    }
}

/// Reference to a node that introduces a new scope.
#[derive(Copy, Clone, Debug)]
pub(crate) enum NodeWithScopeRef<'a> {
    Module,
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
    Lambda(&'a ast::ExprLambda),
    FunctionTypeParameters(&'a ast::StmtFunctionDef),
    ClassTypeParameters(&'a ast::StmtClassDef),
    TypeAlias(&'a ast::StmtTypeAlias),
    TypeAliasTypeParameters(&'a ast::StmtTypeAlias),
    ListComprehension(&'a ast::ExprListComp),
    SetComprehension(&'a ast::ExprSetComp),
    DictComprehension(&'a ast::ExprDictComp),
    GeneratorExpression(&'a ast::ExprGenerator),
}

impl NodeWithScopeRef<'_> {
    /// Converts the unowned reference to an owned [`NodeWithScopeKind`].
    ///
    /// # Safety
    /// The node wrapped by `self` must be a child of `module`.
    #[expect(unsafe_code)]
    pub(super) unsafe fn to_kind(self, module: ParsedModule) -> NodeWithScopeKind {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKind::Module,
            NodeWithScopeRef::Class(class) => {
                NodeWithScopeKind::Class(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKind::Function(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::TypeAlias(type_alias) => {
                NodeWithScopeKind::TypeAlias(AstNodeRef::new(module, type_alias))
            }
            NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                NodeWithScopeKind::TypeAliasTypeParameters(AstNodeRef::new(module, type_alias))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKind::Lambda(AstNodeRef::new(module, lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKind::FunctionTypeParameters(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKind::ClassTypeParameters(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKind::ListComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKind::SetComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKind::DictComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKind::GeneratorExpression(AstNodeRef::new(module, generator))
            }
        }
    }

    pub(crate) fn node_key(self) -> NodeWithScopeKey {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKey::Module,
            NodeWithScopeRef::Class(class) => NodeWithScopeKey::Class(NodeKey::from_node(class)),
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKey::Function(NodeKey::from_node(function))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKey::Lambda(NodeKey::from_node(lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKey::FunctionTypeParameters(NodeKey::from_node(function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKey::ClassTypeParameters(NodeKey::from_node(class))
            }
            NodeWithScopeRef::TypeAlias(type_alias) => {
                NodeWithScopeKey::TypeAlias(NodeKey::from_node(type_alias))
            }
            NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                NodeWithScopeKey::TypeAliasTypeParameters(NodeKey::from_node(type_alias))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKey::ListComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKey::SetComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKey::DictComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKey::GeneratorExpression(NodeKey::from_node(generator))
            }
        }
    }
}

/// Node that introduces a new scope.
#[derive(Clone, Debug, salsa::Update)]
pub enum NodeWithScopeKind {
    Module,
    Class(AstNodeRef<ast::StmtClassDef>),
    ClassTypeParameters(AstNodeRef<ast::StmtClassDef>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    FunctionTypeParameters(AstNodeRef<ast::StmtFunctionDef>),
    TypeAliasTypeParameters(AstNodeRef<ast::StmtTypeAlias>),
    TypeAlias(AstNodeRef<ast::StmtTypeAlias>),
    Lambda(AstNodeRef<ast::ExprLambda>),
    ListComprehension(AstNodeRef<ast::ExprListComp>),
    SetComprehension(AstNodeRef<ast::ExprSetComp>),
    DictComprehension(AstNodeRef<ast::ExprDictComp>),
    GeneratorExpression(AstNodeRef<ast::ExprGenerator>),
}

impl NodeWithScopeKind {
    pub(crate) const fn scope_kind(&self) -> ScopeKind {
        match self {
            Self::Module => ScopeKind::Module,
            Self::Class(_) => ScopeKind::Class,
            Self::Function(_) => ScopeKind::Function,
            Self::Lambda(_) => ScopeKind::Lambda,
            Self::FunctionTypeParameters(_)
            | Self::ClassTypeParameters(_)
            | Self::TypeAliasTypeParameters(_) => ScopeKind::Annotation,
            Self::TypeAlias(_) => ScopeKind::TypeAlias,
            Self::ListComprehension(_)
            | Self::SetComprehension(_)
            | Self::DictComprehension(_)
            | Self::GeneratorExpression(_) => ScopeKind::Comprehension,
        }
    }

    pub fn expect_class(&self) -> &ast::StmtClassDef {
        match self {
            Self::Class(class) => class.node(),
            _ => panic!("expected class"),
        }
    }

    pub fn expect_function(&self) -> &ast::StmtFunctionDef {
        self.as_function().expect("expected function")
    }

    pub fn expect_type_alias(&self) -> &ast::StmtTypeAlias {
        match self {
            Self::TypeAlias(type_alias) => type_alias.node(),
            _ => panic!("expected type alias"),
        }
    }

    pub const fn as_function(&self) -> Option<&ast::StmtFunctionDef> {
        match self {
            Self::Function(function) => Some(function.node()),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NodeWithScopeKey {
    Module,
    Class(NodeKey),
    ClassTypeParameters(NodeKey),
    Function(NodeKey),
    FunctionTypeParameters(NodeKey),
    TypeAlias(NodeKey),
    TypeAliasTypeParameters(NodeKey),
    Lambda(NodeKey),
    ListComprehension(NodeKey),
    SetComprehension(NodeKey),
    DictComprehension(NodeKey),
    GeneratorExpression(NodeKey),
}
