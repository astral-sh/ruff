use crate::expression::Expression;
use crate::member::{
    Member, MemberExpr, MemberExprBuilder, MemberExprRef, MemberTable, MemberTableBuilder,
    ScopedMemberId,
};
use crate::predicate::{PatternPredicate, PatternPredicateKind};
use crate::scope::FileScopeId;
use crate::symbol::{ScopedSymbolId, Symbol, SymbolTable, SymbolTableBuilder};
use crate::{Db, PossiblyNarrowedPlaces};
use ruff_db::parsed::ParsedModuleRef;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use smallvec::SmallVec;
use std::hash::Hash;
use std::iter::FusedIterator;

/// An expression that can be the target of a `Definition`.
#[derive(Eq, PartialEq, Debug, get_size2::GetSize)]
pub enum PlaceExpr {
    /// A simple symbol, e.g. `x`.
    Symbol(Symbol),

    /// A member expression, e.g. `x.y.z[0]`.
    Member(Member),
}

impl PlaceExpr {
    /// Create a new `PlaceExpr` from a name.
    ///
    /// This always returns a `PlaceExpr::Symbol` with empty flags and `name`.
    pub fn from_expr_name(name: &ast::ExprName) -> Self {
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
    pub fn try_from_expr<'e>(expr: impl Into<ast::ExprRef<'e>>) -> Option<Self> {
        let expr = expr.into();

        // For named expressions (walrus operator), extract the target.
        let expr = match expr {
            ast::ExprRef::Named(named) => named.target.as_ref().into(),
            _ => expr,
        };

        if let ast::ExprRef::Name(name) = expr {
            return Some(PlaceExpr::Symbol(Symbol::new(name.id.clone())));
        }

        MemberExprBuilder::visit_expr(expr).and_then(Self::try_from_member_expr)
    }

    /// Tries to create a `PlaceExpr` from a member expression.
    ///
    /// Returns `None` if the expression is not a valid place expression and `Some` otherwise.
    pub(super) fn try_from_member_expr(builder: MemberExprBuilder) -> Option<Self> {
        let member_expression = MemberExpr::try_from_builder(builder)?;
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
pub enum PlaceExprRef<'a> {
    Symbol(&'a Symbol),
    Member(&'a Member),
}

impl<'a> PlaceExprRef<'a> {
    /// Returns `Some` if the reference is a `Symbol`, otherwise `None`.
    pub const fn as_symbol(self) -> Option<&'a Symbol> {
        if let PlaceExprRef::Symbol(symbol) = self {
            Some(symbol)
        } else {
            None
        }
    }

    /// Returns `true` if the reference is a `Symbol`, otherwise `false`.
    pub const fn is_symbol(self) -> bool {
        matches!(self, PlaceExprRef::Symbol(_))
    }

    pub fn is_declared(self) -> bool {
        match self {
            Self::Symbol(symbol) => symbol.is_declared(),
            Self::Member(member) => member.is_declared(),
        }
    }

    pub const fn is_bound(self) -> bool {
        match self {
            PlaceExprRef::Symbol(symbol) => symbol.is_bound(),
            PlaceExprRef::Member(member) => member.is_bound(),
        }
    }

    pub fn num_member_segments(self) -> usize {
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
pub struct PlaceTable {
    symbols: SymbolTable,
    members: MemberTable,
}

impl PlaceTable {
    /// Iterate over the "root" expressions of the place (e.g. `x.y.z`, `x.y`, `x` for `x.y.z[0]`).
    ///
    /// Note, this iterator may skip some parents if they are not defined in the current scope.
    pub fn parents<'a>(&'a self, place_expr: impl Into<PlaceExprRef<'a>>) -> ParentPlaceIter<'a> {
        match place_expr.into() {
            PlaceExprRef::Symbol(_) => ParentPlaceIter::for_symbol(),
            PlaceExprRef::Member(member) => {
                ParentPlaceIter::for_member(member.expression(), &self.symbols, &self.members)
            }
        }
    }

    /// Iterator over all symbols in this scope.
    pub fn symbols(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
    }

    /// Iterator over all members in this scope.
    pub fn members(&self) -> std::slice::Iter<'_, Member> {
        self.members.iter()
    }

    /// Looks up a symbol by its ID and returns a reference to it.
    ///
    /// ## Panics
    /// If the symbol ID is not found in the table.
    #[track_caller]
    pub fn symbol(&self, id: ScopedSymbolId) -> &Symbol {
        self.symbols.symbol(id)
    }

    /// Looks up a symbol by its name and returns a reference to it, if it exists.
    ///
    /// This should only be used in diagnostics and tests.
    pub fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbols.symbol_id(name).map(|id| self.symbol(id))
    }

    /// Looks up a member by its ID and returns a reference to it.
    ///
    /// ## Panics
    /// If the member ID is not found in the table.
    #[track_caller]
    pub fn member(&self, id: ScopedMemberId) -> &Member {
        self.members.member(id)
    }

    /// Returns the [`ScopedSymbolId`] of the place named `name`.
    pub fn symbol_id(&self, name: &str) -> Option<ScopedSymbolId> {
        self.symbols.symbol_id(name)
    }

    /// Returns the [`ScopedPlaceId`] of the place expression.
    pub fn place_id<'e>(&self, place_expr: impl Into<PlaceExprRef<'e>>) -> Option<ScopedPlaceId> {
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
    pub fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef<'_> {
        match place_id.into() {
            ScopedPlaceId::Symbol(symbol) => self.symbol(symbol).into(),
            ScopedPlaceId::Member(member) => self.member(member).into(),
        }
    }

    pub fn member_id_by_instance_attribute_name(&self, name: &str) -> Option<ScopedMemberId> {
        self.members.place_id_by_instance_attribute_name(name)
    }
}

#[derive(Default)]
pub struct PlaceTableBuilder {
    symbols: SymbolTableBuilder,
    member: MemberTableBuilder,

    associated_symbol_members: IndexVec<ScopedSymbolId, SmallVec<[ScopedMemberId; 4]>>,
    associated_sub_members: IndexVec<ScopedMemberId, SmallVec<[ScopedMemberId; 4]>>,
}

impl PlaceTableBuilder {
    /// Looks up a place ID by its expression.
    pub fn place_id(&self, expression: PlaceExprRef) -> Option<ScopedPlaceId> {
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

    pub(super) fn member(&self, id: ScopedMemberId) -> &Member {
        self.member.member(id)
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
    pub fn place(&self, place_id: impl Into<ScopedPlaceId>) -> PlaceExprRef<'_> {
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

    pub fn iter(&self) -> impl Iterator<Item = PlaceExprRef<'_>> {
        self.symbols
            .iter()
            .map(Into::into)
            .chain(self.member.iter().map(PlaceExprRef::Member))
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    pub fn add_symbol(&mut self, symbol: Symbol) -> (ScopedSymbolId, bool) {
        let (id, is_new) = self.symbols.add(symbol);

        if is_new {
            let new_id = self.associated_symbol_members.push(SmallVec::new_const());
            debug_assert_eq!(new_id, id);
        }

        (id, is_new)
    }

    pub fn add_member(&mut self, member: Member) -> (ScopedMemberId, bool) {
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

    pub fn finish(self) -> PlaceTable {
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

pub struct ParentPlaceIter<'a> {
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

/// Builder for computing the conservative set of places that could possibly be narrowed.
///
/// This mirrors the structure of `NarrowingConstraintsBuilder` but only computes which places
/// *could* be narrowed, without performing type inference to determine the actual constraints.
pub(crate) struct PossiblyNarrowedPlacesBuilder<'db, 'a> {
    db: &'db dyn Db,
    places: &'a PlaceTableBuilder,
}

impl<'db, 'a> PossiblyNarrowedPlacesBuilder<'db, 'a> {
    pub(crate) fn new(db: &'db dyn Db, places: &'a PlaceTableBuilder) -> Self {
        Self { db, places }
    }

    /// Compute possibly narrowed places for an expression predicate.
    pub(crate) fn expression(self, expr: &ast::Expr) -> PossiblyNarrowedPlaces {
        self.expression_node(expr)
    }

    /// Compute possibly narrowed places for a pattern predicate.
    pub(crate) fn pattern(
        self,
        pattern: PatternPredicate<'db>,
        module: &ParsedModuleRef,
    ) -> PossiblyNarrowedPlaces {
        self.pattern_kind(pattern.kind(self.db), pattern.subject(self.db), module)
    }

    fn expression_node(&self, expr: &ast::Expr) -> PossiblyNarrowedPlaces {
        match expr {
            // Simple expressions that directly narrow a place
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.simple_expr(expr)
            }
            // Compare expressions can narrow places on either side
            ast::Expr::Compare(expr_compare) => self.expr_compare(expr_compare),
            // Call expressions (isinstance, issubclass, hasattr, TypeGuard, len, bool, etc.)
            ast::Expr::Call(expr_call) => self.expr_call(expr_call),
            // Unary not just delegates to its operand
            ast::Expr::UnaryOp(unary_op) if unary_op.op == ast::UnaryOp::Not => {
                self.expression_node(&unary_op.operand)
            }
            // Boolean operations combine places from all sub-expressions
            ast::Expr::BoolOp(bool_op) => self.expr_bool_op(bool_op),
            // Conditional expressions combine places from all branches and the test.
            ast::Expr::If(expr_if) => self.expr_if(expr_if),
            // Named expressions narrow both the target and the value
            ast::Expr::Named(expr_named) => {
                let mut places = self.simple_expr(&expr_named.target);
                places.extend(self.expression_node(&expr_named.value));
                places
            }
            _ => PossiblyNarrowedPlaces::default(),
        }
    }

    /// Simple expressions that directly narrow a single place.
    fn simple_expr(&self, expr: &ast::Expr) -> PossiblyNarrowedPlaces {
        let mut places = PossiblyNarrowedPlaces::default();
        if let Some(place_expr) = PlaceExpr::try_from_expr(expr) {
            if let Some(place) = self.places.place_id((&place_expr).into()) {
                places.insert(place);
            }
        }
        places
    }

    /// Compare expressions can narrow places on either side of the comparison,
    /// and can also narrow subscript bases (for `TypedDict` and tuple narrowing).
    fn expr_compare(&self, expr_compare: &ast::ExprCompare) -> PossiblyNarrowedPlaces {
        let mut places = PossiblyNarrowedPlaces::default();

        // The left side can be narrowed
        self.add_narrowing_target(&expr_compare.left, &mut places);

        // Each comparator can also be narrowed
        for comparator in &expr_compare.comparators {
            self.add_narrowing_target(comparator, &mut places);
        }

        // For subscript expressions on either side, the subscript base can also be narrowed.
        // (TypedDict and tuple discriminated union narrowing.)
        for expr in std::iter::once(&*expr_compare.left).chain(&expr_compare.comparators) {
            if let ast::Expr::Subscript(subscript) = expr.expression_value()
                && let Some(place_expr) = PlaceExpr::try_from_expr(&subscript.value)
                && let Some(place) = self.places.place_id((&place_expr).into())
            {
                places.insert(place);
            }
        }

        places
    }

    /// Call expressions can narrow their first argument (isinstance, issubclass, hasattr, len)
    /// or narrow based on TypeGuard/TypeIs return types.
    fn expr_call(&self, expr_call: &ast::ExprCall) -> PossiblyNarrowedPlaces {
        let mut places = PossiblyNarrowedPlaces::default();

        // Under the current narrowing semantics, we only ever use the first two positional
        // arguments: argument 0 for most narrowing calls, and argument 1 for unbound
        // TypeGuard/TypeIs methods (e.g. `C.f(C(), x)`).
        // This set is only a conservative upper bound, so if later positional arguments ever
        // become narrowable we can widen this scan again.
        for argument in expr_call.arguments.args.iter().take(2) {
            if let Some(place_expr) = PlaceExpr::try_from_expr(argument) {
                if let Some(place) = self.places.place_id((&place_expr).into()) {
                    places.insert(place);
                }
            }
        }

        // `bool(expr)` can delegate to narrowing `expr` itself, e.g. `bool(x is not None)`
        if let Some(first_arg) = expr_call.arguments.args.first() {
            if expr_call.arguments.args.len() == 1 && expr_call.arguments.keywords.is_empty() {
                places.extend(self.expression_node(first_arg));
            }
        }

        places
    }

    /// Boolean operations combine places from all sub-expressions.
    fn expr_bool_op(&self, bool_op: &ast::ExprBoolOp) -> PossiblyNarrowedPlaces {
        let mut places = PossiblyNarrowedPlaces::default();
        for value in &bool_op.values {
            places.extend(self.expression_node(value));
        }
        places
    }

    fn expr_if(&self, expr_if: &ast::ExprIf) -> PossiblyNarrowedPlaces {
        let mut places = self.expression_node(&expr_if.test);
        places.extend(self.expression_node(&expr_if.body));
        places.extend(self.expression_node(&expr_if.orelse));
        places
    }

    /// Helper to add a potential narrowing target expression to the set.
    fn add_narrowing_target(&self, expr: &ast::Expr, places: &mut PossiblyNarrowedPlaces) {
        if let Some(place_expr) = PlaceExpr::try_from_expr(expr)
            && let Some(place) = self.places.place_id((&place_expr).into())
        {
            places.insert(place);
        }

        match expr.expression_value() {
            // type(x) is Y can narrow x
            ast::Expr::Call(call) if call.arguments.args.len() == 1 => {
                if let Some(first_arg) = call.arguments.args.first()
                    && let Some(place_expr) = PlaceExpr::try_from_expr(first_arg)
                    && let Some(place) = self.places.place_id((&place_expr).into())
                {
                    places.insert(place);
                }
            }
            // x.__class__ is Y can narrow x
            ast::Expr::Attribute(attribute) if attribute.attr.as_str() == "__class__" => {
                if let Some(place_expr) = PlaceExpr::try_from_expr(&attribute.value)
                    && let Some(place) = self.places.place_id((&place_expr).into())
                {
                    places.insert(place);
                }
            }
            _ => {}
        }
    }

    /// Pattern predicates narrow the match subject.
    fn pattern_kind(
        &self,
        kind: &PatternPredicateKind<'db>,
        subject: Expression<'db>,
        module: &ParsedModuleRef,
    ) -> PossiblyNarrowedPlaces {
        let mut places = PossiblyNarrowedPlaces::default();

        // The match subject can always be narrowed by a pattern
        let subject_node = subject.node_ref(self.db).node(module);
        if let Some(subject_place_expr) = PlaceExpr::try_from_expr(subject_node) {
            if let Some(place) = self.places.place_id((&subject_place_expr).into()) {
                places.insert(place);
            }
        }

        // For subscript subjects, the subscript base can also be narrowed (TypedDict/tuple narrowing)
        if let ast::Expr::Subscript(subscript) = subject_node {
            if let Some(place_expr) = PlaceExpr::try_from_expr(&subscript.value) {
                if let Some(place) = self.places.place_id((&place_expr).into()) {
                    places.insert(place);
                }
            }
        }

        // Handle Or patterns by recursing into each alternative
        if let PatternPredicateKind::Or(predicates) = kind {
            for predicate in predicates {
                places.extend(self.pattern_kind(predicate, subject, module));
            }
        }

        // Handle As patterns by recursing into the inner pattern
        if let PatternPredicateKind::As(Some(inner), _) = kind {
            places.extend(self.pattern_kind(inner, subject, module));
        }

        places
    }
}
