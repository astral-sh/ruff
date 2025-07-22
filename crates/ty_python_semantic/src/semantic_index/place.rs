use std::convert::Infallible;
use std::hash::Hash;

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;

use crate::semantic_index::member::{
    Member, MemberExpr, MemberTable, MemberTableBuilder, ScopedMemberId,
};
use crate::semantic_index::scope::FileScopeId;
use crate::semantic_index::symbol::{ScopedSymbolId, Symbol, SymbolTable, SymbolTableBuilder};

/// An expression that can be the target of a `Definition`.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub enum PlaceExpr {
    Symbol(Symbol),
    Member(Member),
}

impl PlaceExpr {
    pub const fn is_symbol(&self) -> bool {
        matches!(self, PlaceExpr::Symbol(_))
    }

    pub const fn is_member(&self) -> bool {
        matches!(self, PlaceExpr::Member(_))
    }
}

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum PlaceExprRef<'a> {
    Symbol(&'a Symbol),
    Member(&'a Member),
}

impl PlaceExprRef<'_> {
    pub(crate) const fn as_symbol(&self) -> Option<&Symbol> {
        if let PlaceExprRef::Symbol(symbol) = self {
            Some(symbol)
        } else {
            None
        }
    }

    pub(crate) const fn is_symbol(&self) -> bool {
        matches!(self, PlaceExprRef::Symbol(_))
    }

    pub(super) fn into_place_expr(self) -> PlaceExpr {
        match self {
            PlaceExprRef::Symbol(symbol) => PlaceExpr::Symbol(symbol.clone()),
            PlaceExprRef::Member(member) => PlaceExpr::Member(member.clone()),
        }
    }

    pub(super) const fn is_bound(&self) -> bool {
        match self {
            PlaceExprRef::Symbol(symbol) => symbol.is_bound(),
            PlaceExprRef::Member(member) => member.is_bound(),
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

impl TryFrom<&ast::name::Name> for PlaceExpr {
    type Error = Infallible;

    fn try_from(name: &ast::name::Name) -> Result<Self, Infallible> {
        Ok(PlaceExpr::Symbol(name.clone()))
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
            .push(MemberSegment::Attribute(attr.attr.id.clone()));
        Ok(place)
    }
}

impl TryFrom<ast::ExprAttribute> for PlaceExpr {
    type Error = ();

    fn try_from(attr: ast::ExprAttribute) -> Result<Self, ()> {
        let mut place = PlaceExpr::try_from(&*attr.value)?;
        place
            .sub_segments
            .push(MemberSegment::Attribute(attr.attr.id));
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
                    .push(MemberSegment::IntSubscript(index.clone()));
            }
            ast::Expr::StringLiteral(string) => {
                place
                    .sub_segments
                    .push(MemberSegment::StringSubscript(string.value.to_string()));
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

/// ID that uniquely identifies a place inside a [`Scope`].
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, get_size2::GetSize, salsa::Update)]
pub(crate) enum ScopedPlaceId {
    Symbol(ScopedSymbolId),
    Member(ScopedMemberId),
}

pub(crate) enum PlaceExprWithFlags {
    Symbol(Symbol),
    Member(Member),
}

#[derive(Debug, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct PlaceTable {
    symbols: SymbolTable,
    members: MemberTable,
}

impl PlaceTable {
    pub(crate) fn place_expr(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef {
        match place_id.into() {
            ScopedPlaceId::Symbol(symbol) => self.symbol(symbol).into(),
            ScopedPlaceId::Member(member) => self.member(member).into(),
        }
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

    pub(crate) fn places(&self) -> impl Iterator<Item = PlaceExprRef> {
        self.symbols
            .iter()
            .map(|symbol| PlaceExprRef::Symbol(symbol))
            .chain(
                self.members
                    .iter()
                    .map(|member| PlaceExprRef::Member(member)),
            )
    }

    pub fn symbols(&self) -> std::slice::Iter<Symbol> {
        self.symbols.iter()
    }

    pub fn members(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    pub fn instance_attributes(&self) -> impl Iterator<Item = &Name> {
        self.members
            .iter()
            .filter_map(|place_expr| place_expr.as_instance_attribute())
    }

    /// Returns the place named `name`.
    #[cfg(test)]
    pub(crate) fn get_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    #[track_caller]
    pub(crate) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
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
            PlaceExprRef::Member(member) => self.members.member_id(member).map(Into::into),
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
    pub(crate) fn try_add_place(&mut self, expr: &ast::Expr) -> Option<(ScopedPlaceId, bool)> {
        match expr {
            ast::Expr::Name(name) => {
                let (id, is_new) = self.add_symbol(name.id.clone());
                Some((id.into(), is_new))
            }
            ast::Expr::Attribute(attr) => {
                let (id, is_new) = self.add_member(Member::from_expr(attr));
                if is_new {
                    Some(ScopedPlaceId::Member(id))
                } else {
                    None
                }
            }
            ast::Expr::Subscript(subscript) => {
                let (id, is_new) = self.add_member(Member::from_expr(subscript));
                if is_new {
                    Some(ScopedPlaceId::Member(id))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(super) fn place_id(&self, expression: PlaceExprRef) -> Option<ScopedPlaceId> {
        match expression {
            PlaceExprRef::Symbol(symbol) => self.symbols.symbol_id(symbol.name()).map(Into::into),
            PlaceExprRef::Member(member) => self.member.member_id(member).map(Into::into),
        }
    }

    pub(super) fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
    }

    pub(super) fn symbol_mut(&mut self, id: ScopedSymbolId) -> &mut Symbol {
        self.symbols.symbol_mut(id)
    }

    pub(crate) fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef {
        match place_id.into() {
            ScopedPlaceId::Symbol(id) => PlaceExprRef::Symbol(self.symbols.symbol(id)),
            ScopedPlaceId::Member(id) => PlaceExprRef::Member(self.member.member(id)),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = PlaceExprRef> {
        self.symbols
            .iter()
            .map(Into::into)
            .chain(self.member.iter().map(PlaceExprRef::Member))
    }

    pub(crate) fn iter_enumerated(&self) -> impl Iterator<Item = (ScopedPlaceId, PlaceExprRef)> {
        self.symbols
            .iter_enumerated()
            .map(|(id, symbol)| (id.into(), symbol.into()))
            .chain(
                self.member
                    .iter_enumerated()
                    .map(|(id, member)| (id.into(), PlaceExprRef::Member(member))),
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
