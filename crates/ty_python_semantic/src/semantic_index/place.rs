use crate::semantic_index::member::{
    Member, MemberExpr, MemberExprRef, MemberTable, MemberTableBuilder, ScopedMemberId,
};
use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::symbol::{ScopedSymbolId, Symbol, SymbolTable, SymbolTableBuilder};
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use smallvec::SmallVec;
use std::hash::Hash;
use std::iter::FusedIterator;

/// An expression that can be the target of a `Definition`.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub(crate) enum PlaceExpr {
    /// A simple symbol, e.g. `x`.
    Symbol(Symbol),

    /// A member expression, e.g. `x.y.z[0]`.
    Member(Member),
}

impl PlaceExpr {
    /// Create a new `PlaceExpr` from a name.
    ///
    /// This always returns a `PlaceExpr::Symbol` with empty flags and `name`.
    pub(crate) fn from_expr_name(name: &ast::ExprName) -> Self {
        PlaceExpr::Symbol(Symbol::new(name.id.clone()))
    }

    /// Tries to create a `PlaceExpr` from an expression.
    ///
    /// Returns `None` if the expression is not a valid place expression and `Some` otherwise.
    ///
    /// Valid expressions are:
    /// * name: `x`
    /// * attribute: `x.y`
    /// * subscripts with integer or string literals: `x[0]`, `x['key']`
    pub(crate) fn try_from_expr<'e>(expr: impl Into<ast::ExprRef<'e>>) -> Option<Self> {
        let expr = expr.into();

        if let ast::ExprRef::Name(name) = expr {
            return Some(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
        }

        let member_expression = MemberExpr::try_from_expr(expr)?;
        Some(Self::Member(Member::new(member_expression)))
    }
}

impl std::fmt::Display for PlaceExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symbol(symbol) => std::fmt::Display::fmt(symbol, f),
            Self::Member(member) => std::fmt::Display::fmt(member, f),
        }
    }
}

/// Reference to a place expression, which can be a symbol or a member expression.
///
/// Needed so that we can iterate over all places without cloning them.
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub(crate) enum PlaceExprRef<'a> {
    Symbol(&'a Symbol),
    Member(&'a Member),
}

impl<'a> PlaceExprRef<'a> {
    /// Returns `Some` if the reference is a `Symbol`, otherwise `None`.
    pub(crate) const fn as_symbol(self) -> Option<&'a Symbol> {
        if let PlaceExprRef::Symbol(symbol) = self {
            Some(symbol)
        } else {
            None
        }
    }

    /// Returns `true` if the reference is a `Symbol`, otherwise `false`.
    pub(crate) const fn is_symbol(self) -> bool {
        matches!(self, PlaceExprRef::Symbol(_))
    }

    pub(crate) fn is_declared(self) -> bool {
        match self {
            Self::Symbol(symbol) => symbol.is_declared(),
            Self::Member(member) => member.is_declared(),
        }
    }

    pub(crate) const fn is_bound(self) -> bool {
        match self {
            PlaceExprRef::Symbol(symbol) => symbol.is_bound(),
            PlaceExprRef::Member(member) => member.is_bound(),
        }
    }

    pub(crate) fn num_member_segments(self) -> usize {
        match self {
            PlaceExprRef::Symbol(_) => 0,
            PlaceExprRef::Member(member) => member.expression().num_segments(),
        }
    }
}

impl<'a> From<&'a Symbol> for PlaceExprRef<'a> {
    fn from(value: &'a Symbol) -> Self {
        Self::Symbol(value)
    }
}

impl<'a> From<&'a Member> for PlaceExprRef<'a> {
    fn from(value: &'a Member) -> Self {
        Self::Member(value)
    }
}

impl<'a> From<&'a PlaceExpr> for PlaceExprRef<'a> {
    fn from(value: &'a PlaceExpr) -> Self {
        match value {
            PlaceExpr::Symbol(symbol) => PlaceExprRef::Symbol(symbol),
            PlaceExpr::Member(member) => PlaceExprRef::Member(member),
        }
    }
}

impl std::fmt::Display for PlaceExprRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symbol(symbol) => std::fmt::Display::fmt(symbol, f),
            Self::Member(member) => std::fmt::Display::fmt(member, f),
        }
    }
}

/// ID that uniquely identifies a place inside a [`Scope`](super::FileScopeId).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize, salsa::Update)]
pub enum ScopedPlaceId {
    Symbol(ScopedSymbolId),
    Member(ScopedMemberId),
}

#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct PlaceTable {
    symbols: SymbolTable,
    members: MemberTable,
}

impl PlaceTable {
    /// Iterate over the "root" expressions of the place (e.g. `x.y.z`, `x.y`, `x` for `x.y.z[0]`).
    ///
    /// Note, this iterator may skip some parents if they are not defined in the current scope.
    pub(crate) fn parents<'a>(
        &'a self,
        place_expr: impl Into<PlaceExprRef<'a>>,
    ) -> ParentPlaceIter<'a> {
        match place_expr.into() {
            PlaceExprRef::Symbol(_) => ParentPlaceIter::for_symbol(),
            PlaceExprRef::Member(member) => {
                ParentPlaceIter::for_member(member.expression(), &self.symbols, &self.members)
            }
        }
    }

    /// Iterator over all symbols in this scope.
    pub(crate) fn symbols(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
    }

    /// Iterator over all members in this scope.
    pub(crate) fn members(&self) -> std::slice::Iter<'_, Member> {
        self.members.iter()
    }

    /// Looks up a symbol by its ID and returns a reference to it.
    ///
    /// ## Panics
    /// If the symbol ID is not found in the table.
    #[track_caller]
    pub(crate) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
    }

    /// Looks up a symbol by its name and returns a reference to it, if it exists.
    ///
    /// This should only be used in diagnostics and tests.
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbols.symbol_id(name).map(|id| self.symbol(id))
    }

    /// Looks up a member by its ID and returns a reference to it.
    ///
    /// ## Panics
    /// If the member ID is not found in the table.
    #[track_caller]
    pub(crate) fn member(&self, id: ScopedMemberId) -> &Member {
        self.members.member(id)
    }

    /// Returns the [`ScopedSymbolId`] of the place named `name`.
    pub(crate) fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        self.symbols.symbol_id(name)
    }

    /// Returns the [`ScopedPlaceId`] of the place expression.
    pub(crate) fn place_id<'e>(
        &self,
        place_expr: impl Into<PlaceExprRef<'e>>,
    ) -> Option<ScopedPlaceId> {
        let place_expr = place_expr.into();

        match place_expr {
            PlaceExprRef::Symbol(symbol) => self.symbols.symbol_id(symbol.name()).map(Into::into),
            PlaceExprRef::Member(member) => {
                self.members.member_id(member.expression()).map(Into::into)
            }
        }
    }

    /// Returns the place expression for the given place ID.
    ///
    /// ## Panics
    /// If the place ID is not found in the table.
    #[track_caller]
    pub(crate) fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef<'_> {
        match place_id.into() {
            ScopedPlaceId::Symbol(symbol) => self.symbol(symbol).into(),
            ScopedPlaceId::Member(member) => self.member(member).into(),
        }
    }

    pub(crate) fn member_id_by_instance_attribute_name(
        &self,
        name: &str,
    ) -> Option<ScopedMemberId> {
        self.members.place_id_by_instance_attribute_name(name)
    }
}

#[derive(Default)]
pub(crate) struct PlaceTableBuilder {
    symbols: SymbolTableBuilder,
    member: MemberTableBuilder,

    associated_symbol_members: IndexVec<ScopedSymbolId, SmallVec<[ScopedMemberId; 4]>>,
    associated_sub_members: IndexVec<ScopedMemberId, SmallVec<[ScopedMemberId; 4]>>,
}

impl PlaceTableBuilder {
    /// Looks up a place ID by its expression.
    pub(super) fn place_id(&self, expression: PlaceExprRef) -> Option<ScopedPlaceId> {
        match expression {
            PlaceExprRef::Symbol(symbol) => self.symbols.symbol_id(symbol.name()).map(Into::into),
            PlaceExprRef::Member(member) => {
                self.member.member_id(member.expression()).map(Into::into)
            }
        }
    }

    #[track_caller]
    pub(super) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
    }

    pub(super) fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        self.symbols.symbol_id(name)
    }

    #[track_caller]
    pub(super) fn symbol_mut(&mut self, id: ScopedSymbolId) -> &mut Symbol {
        self.symbols.symbol_mut(id)
    }

    #[track_caller]
    pub(super) fn member_mut(&mut self, id: ScopedMemberId) -> &mut Member {
        self.member.member_mut(id)
    }

    #[track_caller]
    pub(crate) fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef<'_> {
        match place_id.into() {
            ScopedPlaceId::Symbol(id) => PlaceExprRef::Symbol(self.symbols.symbol(id)),
            ScopedPlaceId::Member(id) => PlaceExprRef::Member(self.member.member(id)),
        }
    }

    pub(crate) fn associated_place_ids(&self, place: ScopedPlaceId) -> &[ScopedMemberId] {
        match place {
            ScopedPlaceId::Symbol(symbol) => &self.associated_symbol_members[symbol],
            ScopedPlaceId::Member(member) => &self.associated_sub_members[member],
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = PlaceExprRef<'_>> {
        self.symbols
            .iter()
            .map(Into::into)
            .chain(self.member.iter().map(PlaceExprRef::Member))
    }

    pub(crate) fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    pub(crate) fn add_symbol(&mut self, symbol: Symbol) -> (ScopedSymbolId, bool) {
        let (id, is_new) = self.symbols.add(symbol);

        if is_new {
            let new_id = self.associated_symbol_members.push(SmallVec::new_const());
            debug_assert_eq!(new_id, id);
        }

        (id, is_new)
    }

    pub(crate) fn add_member(&mut self, member: Member) -> (ScopedMemberId, bool) {
        let (id, is_new) = self.member.add(member);

        if is_new {
            let new_id = self.associated_sub_members.push(SmallVec::new_const());
            debug_assert_eq!(new_id, id);

            let member = self.member.member(id);

            // iterate over parents
            for parent_id in
                ParentPlaceIter::for_member(member.expression(), &self.symbols, &self.member)
            {
                match parent_id {
                    ScopedPlaceId::Symbol(scoped_symbol_id) => {
                        self.associated_symbol_members[scoped_symbol_id].push(id);
                    }
                    ScopedPlaceId::Member(scoped_member_id) => {
                        self.associated_sub_members[scoped_member_id].push(id);
                    }
                }
            }
        }

        (id, is_new)
    }

    pub(crate) fn add_place(&mut self, place: PlaceExpr) -> (ScopedPlaceId, bool) {
        match place {
            PlaceExpr::Symbol(symbol) => {
                let (id, is_new) = self.add_symbol(symbol);
                (ScopedPlaceId::Symbol(id), is_new)
            }
            PlaceExpr::Member(member) => {
                let (id, is_new) = self.add_member(member);
                (ScopedPlaceId::Member(id), is_new)
            }
        }
    }

    #[track_caller]
    pub(super) fn mark_bound(&mut self, id: ScopedPlaceId) {
        match id {
            ScopedPlaceId::Symbol(symbol_id) => {
                self.symbol_mut(symbol_id).mark_bound();
            }
            ScopedPlaceId::Member(member_id) => {
                self.member_mut(member_id).mark_bound();
            }
        }
    }

    #[track_caller]
    pub(super) fn mark_declared(&mut self, id: ScopedPlaceId) {
        match id {
            ScopedPlaceId::Symbol(symbol_id) => {
                self.symbol_mut(symbol_id).mark_declared();
            }
            ScopedPlaceId::Member(member_id) => {
                self.member_mut(member_id).mark_declared();
            }
        }
    }

    pub(crate) fn finish(self) -> PlaceTable {
        PlaceTable {
            symbols: self.symbols.build(),
            members: self.member.build(),
        }
    }
}

impl ScopedPlaceId {
    pub const fn is_symbol(self) -> bool {
        matches!(self, ScopedPlaceId::Symbol(_))
    }

    pub const fn is_member(self) -> bool {
        matches!(self, ScopedPlaceId::Member(_))
    }

    pub const fn as_symbol(self) -> Option<ScopedSymbolId> {
        if let ScopedPlaceId::Symbol(id) = self {
            Some(id)
        } else {
            None
        }
    }

    pub const fn expect_symbol(self) -> ScopedSymbolId {
        match self {
            ScopedPlaceId::Symbol(symbol) => symbol,
            ScopedPlaceId::Member(_) => {
                panic!("Expected ScopedPlaceId::Symbol, found ScopedPlaceId::Member")
            }
        }
    }

    pub const fn as_member(self) -> Option<ScopedMemberId> {
        if let ScopedPlaceId::Member(id) = self {
            Some(id)
        } else {
            None
        }
    }
}

impl<T> std::ops::Index<ScopedPlaceId> for Vec<T> {
    type Output = T;

    fn index(&self, index: ScopedPlaceId) -> &Self::Output {
        match index {
            ScopedPlaceId::Symbol(id) => &self[id.index()],
            ScopedPlaceId::Member(id) => &self[id.index()],
        }
    }
}

impl From<ScopedMemberId> for ScopedPlaceId {
    fn from(value: ScopedMemberId) -> Self {
        Self::Member(value)
    }
}

impl From<ScopedSymbolId> for ScopedPlaceId {
    fn from(value: ScopedSymbolId) -> Self {
        Self::Symbol(value)
    }
}

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

pub(crate) struct ParentPlaceIter<'a> {
    state: Option<ParentPlaceIterState<'a>>,
}

enum ParentPlaceIterState<'a> {
    Symbol {
        symbol_name: &'a str,
        symbols: &'a SymbolTable,
    },
    Member {
        symbols: &'a SymbolTable,
        members: &'a MemberTable,
        next_member: MemberExprRef<'a>,
    },
}

impl<'a> ParentPlaceIterState<'a> {
    fn parent_state(
        expression: &MemberExprRef<'a>,
        symbols: &'a SymbolTable,
        members: &'a MemberTable,
    ) -> Self {
        match expression.parent() {
            Some(parent) => Self::Member {
                next_member: parent,
                symbols,
                members,
            },
            None => Self::Symbol {
                symbol_name: expression.symbol_name(),
                symbols,
            },
        }
    }
}

impl<'a> ParentPlaceIter<'a> {
    pub(super) fn for_symbol() -> Self {
        ParentPlaceIter { state: None }
    }

    pub(super) fn for_member(
        expression: &'a MemberExpr,
        symbol_table: &'a SymbolTable,
        member_table: &'a MemberTable,
    ) -> Self {
        let expr_ref = expression.as_ref();
        ParentPlaceIter {
            state: Some(ParentPlaceIterState::parent_state(
                &expr_ref,
                symbol_table,
                member_table,
            )),
        }
    }
}

impl Iterator for ParentPlaceIter<'_> {
    type Item = ScopedPlaceId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state.take()? {
                ParentPlaceIterState::Symbol {
                    symbol_name,
                    symbols,
                } => {
                    let id = symbols.symbol_id(symbol_name)?;
                    break Some(id.into());
                }
                ParentPlaceIterState::Member {
                    symbols,
                    members,
                    next_member,
                } => {
                    self.state = Some(ParentPlaceIterState::parent_state(
                        &next_member,
                        symbols,
                        members,
                    ));

                    if let Some(id) = members.member_id(next_member) {
                        break Some(id.into());
                    }
                }
            }
        }
    }
}

impl FusedIterator for ParentPlaceIter<'_> {}
