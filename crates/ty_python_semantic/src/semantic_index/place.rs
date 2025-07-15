use std::convert::Infallible;
use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;
use smallvec::{SmallVec, smallvec};

use crate::Db;
use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::reachability_constraints::ScopedReachabilityConstraintId;
use crate::semantic_index::{PlaceSet, SemanticIndex, semantic_index};

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum PlaceExprSubSegment {
    /// A member access, e.g. `.y` in `x.y`
    Member(ast::name::Name),
    /// An integer-based index access, e.g. `[1]` in `x[1]`
    IntSubscript(ast::Int),
    /// A string-based index access, e.g. `["foo"]` in `x["foo"]`
    StringSubscript(String),
}

impl PlaceExprSubSegment {
    pub(crate) fn as_member(&self) -> Option<&ast::name::Name> {
        match self {
            PlaceExprSubSegment::Member(name) => Some(name),
            _ => None,
        }
    }
}

/// An expression that can be the target of a `Definition`.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub struct PlaceExpr {
    root_name: Name,
    sub_segments: SmallVec<[PlaceExprSubSegment; 1]>,
}

impl std::fmt::Display for PlaceExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.root_name)?;
        for segment in &self.sub_segments {
            match segment {
                PlaceExprSubSegment::Member(name) => write!(f, ".{name}")?,
                PlaceExprSubSegment::IntSubscript(int) => write!(f, "[{int}]")?,
                PlaceExprSubSegment::StringSubscript(string) => write!(f, "[\"{string}\"]")?,
            }
        }
        Ok(())
    }
}

impl TryFrom<&ast::name::Name> for PlaceExpr {
    type Error = Infallible;

    fn try_from(name: &ast::name::Name) -> Result<Self, Infallible> {
        Ok(PlaceExpr::name(name.clone()))
    }
}

impl TryFrom<ast::name::Name> for PlaceExpr {
    type Error = Infallible;

    fn try_from(name: ast::name::Name) -> Result<Self, Infallible> {
        Ok(PlaceExpr::name(name))
    }
}

impl TryFrom<&ast::ExprAttribute> for PlaceExpr {
    type Error = ();

    fn try_from(attr: &ast::ExprAttribute) -> Result<Self, ()> {
        let mut place = PlaceExpr::try_from(&*attr.value)?;
        place
            .sub_segments
            .push(PlaceExprSubSegment::Member(attr.attr.id.clone()));
        Ok(place)
    }
}

impl TryFrom<ast::ExprAttribute> for PlaceExpr {
    type Error = ();

    fn try_from(attr: ast::ExprAttribute) -> Result<Self, ()> {
        let mut place = PlaceExpr::try_from(&*attr.value)?;
        place
            .sub_segments
            .push(PlaceExprSubSegment::Member(attr.attr.id));
        Ok(place)
    }
}

impl TryFrom<&ast::ExprSubscript> for PlaceExpr {
    type Error = ();

    fn try_from(subscript: &ast::ExprSubscript) -> Result<Self, ()> {
        let mut place = PlaceExpr::try_from(&*subscript.value)?;
        match &*subscript.slice {
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(index),
                ..
            }) => {
                place
                    .sub_segments
                    .push(PlaceExprSubSegment::IntSubscript(index.clone()));
            }
            ast::Expr::StringLiteral(string) => {
                place
                    .sub_segments
                    .push(PlaceExprSubSegment::StringSubscript(
                        string.value.to_string(),
                    ));
            }
            _ => {
                return Err(());
            }
        }
        Ok(place)
    }
}

impl TryFrom<ast::ExprSubscript> for PlaceExpr {
    type Error = ();

    fn try_from(subscript: ast::ExprSubscript) -> Result<Self, ()> {
        PlaceExpr::try_from(&subscript)
    }
}

impl TryFrom<&ast::Expr> for PlaceExpr {
    type Error = ();

    fn try_from(expr: &ast::Expr) -> Result<Self, ()> {
        match expr {
            ast::Expr::Name(name) => Ok(PlaceExpr::name(name.id.clone())),
            ast::Expr::Attribute(attr) => PlaceExpr::try_from(attr),
            ast::Expr::Subscript(subscript) => PlaceExpr::try_from(subscript),
            _ => Err(()),
        }
    }
}

impl TryFrom<ast::ExprRef<'_>> for PlaceExpr {
    type Error = ();

    fn try_from(expr: ast::ExprRef) -> Result<Self, ()> {
        match expr {
            ast::ExprRef::Name(name) => Ok(PlaceExpr::name(name.id.clone())),
            ast::ExprRef::Attribute(attr) => PlaceExpr::try_from(attr),
            ast::ExprRef::Subscript(subscript) => PlaceExpr::try_from(subscript),
            _ => Err(()),
        }
    }
}

impl PlaceExpr {
    pub(crate) fn name(name: Name) -> Self {
        Self {
            root_name: name,
            sub_segments: smallvec![],
        }
    }

    pub(crate) fn root_name(&self) -> &Name {
        &self.root_name
    }

    pub(crate) fn sub_segments(&self) -> &[PlaceExprSubSegment] {
        &self.sub_segments
    }

    pub(crate) fn as_name(&self) -> Option<&Name> {
        if self.is_name() {
            Some(&self.root_name)
        } else {
            None
        }
    }

    /// Assumes that the place expression is a name.
    #[track_caller]
    pub(crate) fn expect_name(&self) -> &Name {
        debug_assert_eq!(self.sub_segments.len(), 0);
        &self.root_name
    }

    /// Is the place just a name?
    pub fn is_name(&self) -> bool {
        self.sub_segments.is_empty()
    }

    pub fn is_name_and(&self, f: impl FnOnce(&str) -> bool) -> bool {
        self.is_name() && f(&self.root_name)
    }

    /// Does the place expression have the form `<object>.member`?
    pub fn is_member(&self) -> bool {
        self.sub_segments
            .last()
            .is_some_and(|last| last.as_member().is_some())
    }

    fn root_exprs(&self) -> RootExprs<'_> {
        RootExprs {
            expr_ref: self.into(),
            len: self.sub_segments.len(),
        }
    }
}

/// A [`PlaceExpr`] with flags, e.g. whether it is used, bound, an instance attribute, etc.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub struct PlaceExprWithFlags {
    pub(crate) expr: PlaceExpr,
    flags: PlaceFlags,
}

impl std::fmt::Display for PlaceExprWithFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.expr.fmt(f)
    }
}

impl PlaceExprWithFlags {
    pub(crate) fn new(expr: PlaceExpr) -> Self {
        PlaceExprWithFlags {
            expr,
            flags: PlaceFlags::empty(),
        }
    }

    fn name(name: Name) -> Self {
        PlaceExprWithFlags {
            expr: PlaceExpr::name(name),
            flags: PlaceFlags::empty(),
        }
    }

    fn insert_flags(&mut self, flags: PlaceFlags) {
        self.flags.insert(flags);
    }

    pub(super) fn mark_instance_attribute(&mut self) {
        self.flags.insert(PlaceFlags::IS_INSTANCE_ATTRIBUTE);
    }

    /// If the place expression has the form `<NAME>.<MEMBER>`
    /// (meaning it *may* be an instance attribute),
    /// return `Some(<MEMBER>)`. Else, return `None`.
    ///
    /// This method is internal to the semantic-index submodule.
    /// It *only* checks that the AST structure of the `Place` is
    /// correct. It does not check whether the `Place` actually occurred in
    /// a method context, or whether the `<NAME>` actually refers to the first
    /// parameter of the method (i.e. `self`). To answer those questions,
    /// use [`Self::as_instance_attribute`].
    pub(super) fn as_instance_attribute_candidate(&self) -> Option<&Name> {
        if self.expr.sub_segments.len() == 1 {
            self.expr.sub_segments[0].as_member()
        } else {
            None
        }
    }

    /// Return `true` if the place expression has the form `<NAME>.<MEMBER>`,
    /// indicating that it *may* be an instance attribute if we are in a method context.
    ///
    /// This method is internal to the semantic-index submodule.
    /// It *only* checks that the AST structure of the `Place` is
    /// correct. It does not check whether the `Place` actually occurred in
    /// a method context, or whether the `<NAME>` actually refers to the first
    /// parameter of the method (i.e. `self`). To answer those questions,
    /// use [`Self::is_instance_attribute`].
    pub(super) fn is_instance_attribute_candidate(&self) -> bool {
        self.as_instance_attribute_candidate().is_some()
    }

    /// Does the place expression have the form `self.{name}` (`self` is the first parameter of the method)?
    pub(super) fn is_instance_attribute_named(&self, name: &str) -> bool {
        self.as_instance_attribute().map(Name::as_str) == Some(name)
    }

    /// Return `Some(<ATTRIBUTE>)` if the place expression is an instance attribute.
    pub(crate) fn as_instance_attribute(&self) -> Option<&Name> {
        if self.is_instance_attribute() {
            debug_assert!(self.as_instance_attribute_candidate().is_some());
            self.as_instance_attribute_candidate()
        } else {
            None
        }
    }

    /// Is the place an instance attribute?
    pub(crate) fn is_instance_attribute(&self) -> bool {
        let is_instance_attribute = self.flags.contains(PlaceFlags::IS_INSTANCE_ATTRIBUTE);
        if is_instance_attribute {
            debug_assert!(self.is_instance_attribute_candidate());
        }
        is_instance_attribute
    }

    pub(crate) fn is_name(&self) -> bool {
        self.expr.is_name()
    }

    pub(crate) fn is_member(&self) -> bool {
        self.expr.is_member()
    }

    /// Is the place used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(PlaceFlags::IS_USED)
    }

    /// Is the place given a value in its containing scope?
    pub fn is_bound(&self) -> bool {
        self.flags.contains(PlaceFlags::IS_BOUND)
    }

    /// Is the place declared in its containing scope?
    pub fn is_declared(&self) -> bool {
        self.flags.contains(PlaceFlags::IS_DECLARED)
    }

    /// Is the place `global` its containing scope?
    pub fn is_marked_global(&self) -> bool {
        self.flags.contains(PlaceFlags::MARKED_GLOBAL)
    }

    /// Is the place `nonlocal` its containing scope?
    pub fn is_marked_nonlocal(&self) -> bool {
        self.flags.contains(PlaceFlags::MARKED_NONLOCAL)
    }

    pub(crate) fn as_name(&self) -> Option<&Name> {
        self.expr.as_name()
    }

    pub(crate) fn expect_name(&self) -> &Name {
        self.expr.expect_name()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub struct PlaceExprRef<'a> {
    pub(crate) root_name: &'a Name,
    /// Sub-segments is empty for a simple target (e.g. `foo`).
    pub(crate) sub_segments: &'a [PlaceExprSubSegment],
}

impl PartialEq<PlaceExpr> for PlaceExprRef<'_> {
    fn eq(&self, other: &PlaceExpr) -> bool {
        self.root_name == &other.root_name && self.sub_segments == &other.sub_segments[..]
    }
}

impl PartialEq<PlaceExprRef<'_>> for PlaceExpr {
    fn eq(&self, other: &PlaceExprRef<'_>) -> bool {
        &self.root_name == other.root_name && &self.sub_segments[..] == other.sub_segments
    }
}

impl<'e> From<&'e PlaceExpr> for PlaceExprRef<'e> {
    fn from(expr: &'e PlaceExpr) -> Self {
        PlaceExprRef {
            root_name: &expr.root_name,
            sub_segments: &expr.sub_segments,
        }
    }
}

struct RootExprs<'e> {
    expr_ref: PlaceExprRef<'e>,
    len: usize,
}

impl<'e> Iterator for RootExprs<'e> {
    type Item = PlaceExprRef<'e>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(PlaceExprRef {
            root_name: self.expr_ref.root_name,
            sub_segments: &self.expr_ref.sub_segments[..self.len],
        })
    }
}

bitflags! {
    /// Flags that can be queried to obtain information about a place in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a place to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct PlaceFlags: u8 {
        const IS_USED               = 1 << 0;
        const IS_BOUND              = 1 << 1;
        const IS_DECLARED           = 1 << 2;
        const MARKED_GLOBAL         = 1 << 3;
        const MARKED_NONLOCAL       = 1 << 4;
        const IS_INSTANCE_ATTRIBUTE = 1 << 5;
    }
}

impl get_size2::GetSize for PlaceFlags {}

/// ID that uniquely identifies a place in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FilePlaceId {
    scope: FileScopeId,
    scoped_place_id: ScopedPlaceId,
}

impl FilePlaceId {
    pub fn scope(self) -> FileScopeId {
        self.scope
    }

    pub(crate) fn scoped_place_id(self) -> ScopedPlaceId {
        self.scoped_place_id
    }
}

impl From<FilePlaceId> for ScopedPlaceId {
    fn from(val: FilePlaceId) -> Self {
        val.scoped_place_id()
    }
}

/// ID that uniquely identifies a place inside a [`Scope`].
#[newtype_index]
#[derive(salsa::Update, get_size2::GetSize)]
pub struct ScopedPlaceId;

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked(debug)]
pub struct ScopeId<'db> {
    pub file: File,

    pub file_scope_id: FileScopeId,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ScopeId<'_> {}

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
    pub(crate) fn name<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast str {
        match self.node(db) {
            NodeWithScopeKind::Module => "<module>",
            NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
                class.node(module).name.as_str()
            }
            NodeWithScopeKind::Function(function)
            | NodeWithScopeKind::FunctionTypeParameters(function) => {
                function.node(module).name.as_str()
            }
            NodeWithScopeKind::TypeAlias(type_alias)
            | NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => type_alias
                .node(module)
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
#[derive(salsa::Update, get_size2::GetSize)]
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

#[derive(Debug, salsa::Update, get_size2::GetSize)]
pub struct Scope {
    parent: Option<FileScopeId>,
    node: NodeWithScopeKind,
    descendants: Range<FileScopeId>,
    reachability: ScopedReachabilityConstraintId,
}

impl Scope {
    pub(super) fn new(
        parent: Option<FileScopeId>,
        node: NodeWithScopeKind,
        descendants: Range<FileScopeId>,
        reachability: ScopedReachabilityConstraintId,
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

    pub(crate) fn reachability(&self) -> ScopedReachabilityConstraintId {
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
    pub(crate) const fn is_eager(self) -> bool {
        match self {
            ScopeKind::Module | ScopeKind::Class | ScopeKind::Comprehension => true,
            ScopeKind::Annotation
            | ScopeKind::Function
            | ScopeKind::Lambda
            | ScopeKind::TypeAlias => false,
        }
    }

    pub(crate) const fn is_function_like(self) -> bool {
        // Type parameter scopes behave like function scopes in terms of name resolution; CPython
        // place table also uses the term "function-like" for these scopes.
        matches!(
            self,
            ScopeKind::Annotation
                | ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::TypeAlias
                | ScopeKind::Comprehension
        )
    }

    pub(crate) const fn is_class(self) -> bool {
        matches!(self, ScopeKind::Class)
    }

    pub(crate) const fn is_type_parameter(self) -> bool {
        matches!(self, ScopeKind::Annotation | ScopeKind::TypeAlias)
    }

    pub(crate) const fn is_non_lambda_function(self) -> bool {
        matches!(self, ScopeKind::Function)
    }
}

/// [`PlaceExpr`] table for a specific [`Scope`].
#[derive(Default, get_size2::GetSize)]
pub struct PlaceTable {
    /// The place expressions in this scope.
    places: IndexVec<ScopedPlaceId, PlaceExprWithFlags>,

    /// The set of places.
    place_set: PlaceSet,
}

impl PlaceTable {
    fn shrink_to_fit(&mut self) {
        self.places.shrink_to_fit();
    }

    pub(crate) fn place_expr(&self, place_id: impl Into<ScopedPlaceId>) -> &PlaceExprWithFlags {
        &self.places[place_id.into()]
    }

    /// Iterate over the "root" expressions of the place (e.g. `x.y.z`, `x.y`, `x` for `x.y.z[0]`).
    pub(crate) fn root_place_exprs(
        &self,
        place_expr: &PlaceExpr,
    ) -> impl Iterator<Item = &PlaceExprWithFlags> {
        place_expr
            .root_exprs()
            .filter_map(|place_expr| self.place_by_expr(place_expr))
    }

    #[expect(unused)]
    pub(crate) fn place_ids(&self) -> impl Iterator<Item = ScopedPlaceId> {
        self.places.indices()
    }

    pub fn places(&self) -> impl Iterator<Item = &PlaceExprWithFlags> {
        self.places.iter()
    }

    pub fn symbols(&self) -> impl Iterator<Item = &PlaceExprWithFlags> {
        self.places().filter(|place_expr| place_expr.is_name())
    }

    pub fn instance_attributes(&self) -> impl Iterator<Item = &Name> {
        self.places()
            .filter_map(|place_expr| place_expr.as_instance_attribute())
    }

    /// Returns the place named `name`.
    #[cfg(test)]
    pub(crate) fn place_by_name(&self, name: &str) -> Option<&PlaceExprWithFlags> {
        let id = self.place_id_by_name(name)?;
        Some(self.place_expr(id))
    }

    /// Returns the flagged place.
    pub(crate) fn place_by_expr<'e>(
        &self,
        place_expr: impl Into<PlaceExprRef<'e>>,
    ) -> Option<&PlaceExprWithFlags> {
        let id = self.place_id_by_expr(place_expr)?;
        Some(self.place_expr(id))
    }

    /// Returns the [`ScopedPlaceId`] of the place named `name`.
    pub(crate) fn place_id_by_name(&self, name: &str) -> Option<ScopedPlaceId> {
        self.place_set
            .find(Self::hash_name(name), |id| {
                self.place_expr(*id).as_name().map(Name::as_str) == Some(name)
            })
            .copied()
    }

    /// Returns the [`ScopedPlaceId`] of the place expression.
    pub(crate) fn place_id_by_expr<'e>(
        &self,
        place_expr: impl Into<PlaceExprRef<'e>>,
    ) -> Option<ScopedPlaceId> {
        let place_expr = place_expr.into();
        self.place_set
            .find(Self::hash_place_expr(place_expr), |id| {
                self.place_expr(*id).expr == place_expr
            })
            .copied()
    }

    pub(crate) fn place_id_by_instance_attribute_name(&self, name: &str) -> Option<ScopedPlaceId> {
        self.places
            .indices()
            .find(|id| self.places[*id].is_instance_attribute_named(name))
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }

    fn hash_place_expr<'e>(place_expr: impl Into<PlaceExprRef<'e>>) -> u64 {
        let place_expr: PlaceExprRef = place_expr.into();

        let mut hasher = FxHasher::default();

        // Special case for simple names (e.g. "foo"). Only hash the name so
        // that a lookup by name can find it (see `place_by_name`).
        if place_expr.sub_segments.is_empty() {
            place_expr.root_name.as_str().hash(&mut hasher);
        } else {
            place_expr.hash(&mut hasher);
        }
        hasher.finish()
    }
}

impl PartialEq for PlaceTable {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the place_set because the place is already captured in `PlaceExpr`.
        self.places == other.places
    }
}

impl Eq for PlaceTable {}

impl std::fmt::Debug for PlaceTable {
    /// Exclude the `place_set` field from the debug output.
    /// It's very noisy and not useful for debugging.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PlaceTable")
            .field(&self.places)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(super) struct PlaceTableBuilder {
    table: PlaceTable,

    associated_place_ids: IndexVec<ScopedPlaceId, Vec<ScopedPlaceId>>,
}

impl PlaceTableBuilder {
    pub(super) fn add_symbol(&mut self, name: Name) -> (ScopedPlaceId, bool) {
        let hash = PlaceTable::hash_name(&name);
        let entry = self.table.place_set.entry(
            hash,
            |id| self.table.places[*id].as_name() == Some(&name),
            |id| PlaceTable::hash_place_expr(&self.table.places[*id].expr),
        );

        match entry {
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => {
                let symbol = PlaceExprWithFlags::name(name);

                let id = self.table.places.push(symbol);
                entry.insert(id);
                let new_id = self.associated_place_ids.push(vec![]);
                debug_assert_eq!(new_id, id);
                (id, true)
            }
        }
    }

    pub(super) fn add_place(&mut self, place_expr: PlaceExprWithFlags) -> (ScopedPlaceId, bool) {
        let hash = PlaceTable::hash_place_expr(&place_expr.expr);
        let entry = self.table.place_set.entry(
            hash,
            |id| self.table.places[*id].expr == place_expr.expr,
            |id| PlaceTable::hash_place_expr(&self.table.places[*id].expr),
        );

        match entry {
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => {
                let id = self.table.places.push(place_expr);
                entry.insert(id);
                let new_id = self.associated_place_ids.push(vec![]);
                debug_assert_eq!(new_id, id);
                for root in self.table.places[id].expr.root_exprs() {
                    if let Some(root_id) = self.table.place_id_by_expr(root) {
                        self.associated_place_ids[root_id].push(id);
                    }
                }
                (id, true)
            }
        }
    }

    pub(super) fn mark_place_bound(&mut self, id: ScopedPlaceId) {
        self.table.places[id].insert_flags(PlaceFlags::IS_BOUND);
    }

    pub(super) fn mark_place_declared(&mut self, id: ScopedPlaceId) {
        self.table.places[id].insert_flags(PlaceFlags::IS_DECLARED);
    }

    pub(super) fn mark_place_used(&mut self, id: ScopedPlaceId) {
        self.table.places[id].insert_flags(PlaceFlags::IS_USED);
    }

    pub(super) fn mark_place_global(&mut self, id: ScopedPlaceId) {
        self.table.places[id].insert_flags(PlaceFlags::MARKED_GLOBAL);
    }

    pub(super) fn mark_place_nonlocal(&mut self, id: ScopedPlaceId) {
        self.table.places[id].insert_flags(PlaceFlags::MARKED_NONLOCAL);
    }

    pub(super) fn places(&self) -> impl Iterator<Item = &PlaceExprWithFlags> {
        self.table.places()
    }

    pub(super) fn place_id_by_expr(&self, place_expr: &PlaceExpr) -> Option<ScopedPlaceId> {
        self.table.place_id_by_expr(place_expr)
    }

    pub(super) fn place_expr(&self, place_id: impl Into<ScopedPlaceId>) -> &PlaceExprWithFlags {
        self.table.place_expr(place_id)
    }

    /// Returns the place IDs associated with the place (e.g. `x.y`, `x.y.z`, `x.y.z[0]` for `x`).
    pub(super) fn associated_place_ids(
        &self,
        place: ScopedPlaceId,
    ) -> impl Iterator<Item = ScopedPlaceId> {
        self.associated_place_ids[place].iter().copied()
    }

    pub(super) fn finish(mut self) -> PlaceTable {
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
    /// Note that node wrapped by `self` must be a child of `module`.
    pub(super) fn to_kind(self, module: &ParsedModuleRef) -> NodeWithScopeKind {
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
#[derive(Clone, Debug, salsa::Update, get_size2::GetSize)]
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

    pub fn expect_class<'ast>(&self, module: &'ast ParsedModuleRef) -> &'ast ast::StmtClassDef {
        match self {
            Self::Class(class) => class.node(module),
            _ => panic!("expected class"),
        }
    }

    pub(crate) fn as_class<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> Option<&'ast ast::StmtClassDef> {
        match self {
            Self::Class(class) => Some(class.node(module)),
            _ => None,
        }
    }

    pub fn expect_function<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtFunctionDef {
        self.as_function(module).expect("expected function")
    }

    pub fn expect_type_alias<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> &'ast ast::StmtTypeAlias {
        match self {
            Self::TypeAlias(type_alias) => type_alias.node(module),
            _ => panic!("expected type alias"),
        }
    }

    pub fn as_function<'ast>(
        &self,
        module: &'ast ParsedModuleRef,
    ) -> Option<&'ast ast::StmtFunctionDef> {
        match self {
            Self::Function(function) => Some(function.node(module)),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
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
