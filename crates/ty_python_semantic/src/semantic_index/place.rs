use crate::semantic_index::member::{
    Member, MemberExpr, MemberExprRef, MemberSegment, MemberTable, MemberTableBuilder,
    ScopedMemberId,
};
use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::symbol::{ScopedSymbolId, Symbol, SymbolTable, SymbolTableBuilder};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use std::hash::Hash;
use std::iter::FusedIterator;

/// An expression that can be the target of a `Definition`.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub(crate) enum PlaceExpr {
    Symbol(Symbol),
    Member(Member),
}

impl PlaceExpr {
    pub(crate) fn from_expr_name(name: &ast::ExprName) -> Self {
        PlaceExpr::Symbol(Symbol::new(name.id.clone()))
    }

    pub(crate) fn try_from_expr<'e>(expr: impl Into<ast::ExprRef<'e>>) -> Option<Self> {
        let mut current = expr.into();
        let mut segments = smallvec::SmallVec::new_const();

        loop {
            match current {
                ast::ExprRef::Name(name) => {
                    if segments.is_empty() {
                        return Some(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
                    }

                    segments.push(MemberSegment::Symbol(name.id.clone()));
                    segments.reverse();

                    return Some(PlaceExpr::Member(Member::new(MemberExpr::new(segments))));
                }
                ast::ExprRef::Attribute(attribute) => {
                    segments.push(MemberSegment::Attribute(attribute.attr.id.clone()));
                    current = ast::ExprRef::from(&attribute.value);
                }
                ast::ExprRef::Subscript(subscript) => {
                    match &*subscript.slice {
                        ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                            value: ast::Number::Int(index),
                            ..
                        }) => {
                            segments.push(MemberSegment::IntSubscript(index.clone()));
                        }
                        ast::Expr::StringLiteral(string) => {
                            segments.push(MemberSegment::StringSubscript(string.value.to_string()));
                        }
                        _ => {
                            return None;
                        }
                    }

                    current = ast::ExprRef::from(&subscript.value);
                }
                _ => {
                    return None;
                }
            }
        }
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
    pub(crate) const fn as_symbol(&self) -> Option<&'a Symbol> {
        if let PlaceExprRef::Symbol(symbol) = self {
            Some(symbol)
        } else {
            None
        }
    }

    pub(crate) const fn is_symbol(&self) -> bool {
        matches!(self, PlaceExprRef::Symbol(_))
    }

    pub(crate) fn is_global(&self) -> bool {
        match self {
            Self::Symbol(symbol) => symbol.is_global(),
            Self::Member(_) => false,
        }
    }

    pub(crate) fn is_nonlocal(&self) -> bool {
        match self {
            Self::Symbol(symbol) => symbol.is_marked_nonlocal(),
            Self::Member(_) => false,
        }
    }

    pub(crate) fn is_declared(&self) -> bool {
        match self {
            Self::Symbol(symbol) => symbol.is_declared(),
            Self::Member(member) => member.is_declared(),
        }
    }

    pub(crate) const fn is_bound(&self) -> bool {
        match self {
            PlaceExprRef::Symbol(symbol) => symbol.is_bound(),
            PlaceExprRef::Member(member) => member.is_bound(),
        }
    }

    pub(crate) fn num_segments(&self) -> usize {
        match self {
            PlaceExprRef::Symbol(_) => 1,
            PlaceExprRef::Member(member) => member.expression().segments().len(),
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

/// ID that uniquely identifies a place inside a [`Scope`].
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
    pub(crate) fn parents<'a>(
        &'a self,
        place_expr: impl Into<PlaceExprRef<'a>>,
    ) -> AncestorPlaceIter<'a> {
        match place_expr.into() {
            PlaceExprRef::Symbol(_) => AncestorPlaceIter {
                table: self,
                segments: &[],
            },
            PlaceExprRef::Member(member) => AncestorPlaceIter {
                table: self,
                segments: member.expression().segments(),
            },
        }
    }

    pub(crate) fn symbols(&self) -> std::slice::Iter<Symbol> {
        self.symbols.iter()
    }

    pub(crate) fn members(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    #[track_caller]
    pub(crate) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
    }

    #[cfg(test)]
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbols.symbol_id(name).map(|id| self.symbol(id))
    }

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

    pub(crate) fn place_expr(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef {
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
}

impl PlaceTableBuilder {
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

    #[track_caller]
    pub(super) fn symbol_mut(&mut self, id: ScopedSymbolId) -> &mut Symbol {
        self.symbols.symbol_mut(id)
    }

    #[track_caller]
    pub(super) fn member_mut(&mut self, id: ScopedMemberId) -> &mut Member {
        self.member.member_mut(id)
    }

    pub(crate) fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef {
        match place_id.into() {
            ScopedPlaceId::Symbol(id) => PlaceExprRef::Symbol(self.symbols.symbol(id)),
            ScopedPlaceId::Member(id) => PlaceExprRef::Member(self.member.member(id)),
        }
    }

    pub(crate) fn iter_starts_with(
        &self,
        place: ScopedPlaceId,
    ) -> impl Iterator<Item = ScopedMemberId> {
        let place = self.place(place);

        let members = match place {
            PlaceExprRef::Symbol(symbol) => itertools::Either::Left(
                self.member
                    .iter_enumerated()
                    .filter(|(_, current)| current.symbol_name() == symbol.name()),
            ),
            PlaceExprRef::Member(member) => {
                let prefix = member.expression();
                itertools::Either::Right(
                    self.member
                        .iter_enumerated()
                        .filter(|(_, current)| current.expression().starts_with(prefix)),
                )
            }
        };

        members.map(|(id, _)| id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = PlaceExprRef> {
        self.symbols.iter().map(Into::into).chain(
            self.member
                .iter()
                .map(PlaceExprRef::Member),
        )
    }

    pub(crate) fn add_symbol(&mut self, name: Name) -> (ScopedSymbolId, bool) {
        self.symbols.add(name)
    }

    pub(crate) fn add_member(&mut self, member: Member) -> (ScopedMemberId, bool) {
        self.member.add(member)
    }

    pub(crate) fn add_place(&mut self, place: PlaceExpr) -> (ScopedPlaceId, bool) {
        match place {
            PlaceExpr::Symbol(symbol) => {
                let (id, is_new) = self.add_symbol(symbol.into_name());
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
        self.as_symbol().unwrap()
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

pub(crate) struct AncestorPlaceIter<'a> {
    table: &'a PlaceTable,
    segments: &'a [MemberSegment],
}

impl<'a> Iterator for AncestorPlaceIter<'a> {
    type Item = PlaceExprRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.segments {
            [] => None,
            [MemberSegment::Symbol(symbol)] => {
                self.segments = &[];
                let symbol = self.table.symbol_id(symbol)?;

                Some(self.table.symbol(symbol).into())
            }
            segments => {
                self.segments = &self.segments[..self.segments.len() - 1];

                let id = self
                    .table
                    .members
                    .member_id(MemberExprRef::from_raw(segments))?;

                Some(self.table.member(id).into())
            }
        }
    }
}

impl FusedIterator for AncestorPlaceIter<'_> {}
