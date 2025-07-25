use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{self as ast, name::Name};
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use std::hash::{Hash as _, Hasher as _};
use std::ops::{Deref, DerefMut};

/// A member access, e.g. `x.y` or `x[1]` or `x["foo"]`.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct Member {
    expression: MemberExpr,
    flags: MemberFlags,
}

impl Member {
    pub(crate) fn new(expression: MemberExpr) -> Self {
        Self {
            expression,
            flags: MemberFlags::empty(),
        }
    }

    /// Returns the left most part of the member expression, e.g. `x` in `x.y.z`.
    ///
    /// This is the symbol on which the member access is performed.
    pub(crate) fn symbol_name(&self) -> &Name {
        self.expression.symbol_name()
    }

    pub(crate) fn expression(&self) -> &MemberExpr {
        &self.expression
    }

    /// Is the place given a value in its containing scope?
    pub(crate) const fn is_bound(&self) -> bool {
        self.flags.contains(MemberFlags::IS_BOUND)
    }

    /// Is the place declared in its containing scope?
    pub(crate) fn is_declared(&self) -> bool {
        self.flags.contains(MemberFlags::IS_DECLARED)
    }

    pub(super) fn mark_bound(&mut self) {
        self.insert_flags(MemberFlags::IS_BOUND);
    }

    pub(super) fn mark_declared(&mut self) {
        self.insert_flags(MemberFlags::IS_DECLARED);
    }

    pub(super) fn mark_instance_attribute(&mut self) {
        self.flags.insert(MemberFlags::IS_INSTANCE_ATTRIBUTE);
    }

    /// Is the place an instance attribute?
    pub(crate) fn is_instance_attribute(&self) -> bool {
        let is_instance_attribute = self.flags.contains(MemberFlags::IS_INSTANCE_ATTRIBUTE);
        if is_instance_attribute {
            debug_assert!(self.is_instance_attribute_candidate());
        }
        is_instance_attribute
    }

    fn insert_flags(&mut self, flags: MemberFlags) {
        self.flags.insert(flags);
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
        match &*self.expression.segments {
            [MemberSegment::Attribute(name)] => Some(name),
            _ => None,
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
}

impl std::fmt::Display for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.expression, f)
    }
}

bitflags! {
    /// Flags that can be queried to obtain information about a member in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a member to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
     struct MemberFlags: u8 {
        const IS_BOUND              = 1 << 0;
        const IS_DECLARED           = 1 << 1;
        const IS_INSTANCE_ATTRIBUTE = 1 << 2;
    }
}

impl get_size2::GetSize for MemberFlags {}

/// An expression accessing a member on a symbol named `symbol_name`, e.g. `x.y.z`.
///
/// The parts after the symbol name are called segments, and they can be either:
/// * An attribute access, e.g. `.y` in `x.y`
/// * An integer-based subscript, e.g. `[1]` in `x[1]`
/// * A string-based subscript, e.g. `["foo"]` in `x["foo"]`
///
/// Internally, the segments are stored in reverse order. This allows constructing
/// a `MemberExpr` from an ast expression without having to reverse the segments.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize, Hash)]
pub(crate) struct MemberExpr {
    symbol_name: Name,
    segments: SmallVec<[MemberSegment; 1]>,
}

impl MemberExpr {
    pub(super) fn new(symbol_name: Name, segments: SmallVec<[MemberSegment; 1]>) -> Self {
        debug_assert!(
            !segments.is_empty(),
            "A member without segments is a symbol."
        );

        Self {
            symbol_name,
            segments,
        }
    }

    fn shrink_to_fit(&mut self) {
        self.segments.shrink_to_fit();
        self.segments.shrink_to_fit();
    }

    /// Returns the left most part of the member expression, e.g. `x` in `x.y.z`.
    ///
    /// This is the symbol on which the member access is performed.
    pub(crate) fn symbol_name(&self) -> &Name {
        &self.symbol_name
    }

    /// Returns the segments of the member expression, e.g. `[MemberSegment::Attribute("y"), MemberSegment::IntSubscript(1)]` for `x.y[1]`.
    pub(crate) fn member_segments(
        &self,
    ) -> impl ExactSizeIterator<Item = &MemberSegment> + DoubleEndedIterator {
        self.segments.iter().rev()
    }

    pub(crate) fn as_ref(&self) -> MemberExprRef {
        MemberExprRef {
            name: self.symbol_name.as_str(),
            segments: self.segments.as_slice(),
        }
    }
}

impl std::fmt::Display for MemberExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol_name.as_str())?;

        for segment in self.member_segments() {
            match segment {
                MemberSegment::Attribute(name) => write!(f, ".{name}")?,
                MemberSegment::IntSubscript(int) => write!(f, "[{int}]")?,
                MemberSegment::StringSubscript(string) => write!(f, "[\"{string}\"]")?,
            }
        }

        Ok(())
    }
}

impl PartialEq<MemberExprRef<'_>> for MemberExpr {
    fn eq(&self, other: &MemberExprRef) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<MemberExprRef<'_>> for &MemberExpr {
    fn eq(&self, other: &MemberExprRef) -> bool {
        self.as_ref() == *other
    }
}

impl PartialEq<MemberExpr> for MemberExprRef<'_> {
    fn eq(&self, other: &MemberExpr) -> bool {
        other == self
    }
}

impl PartialEq<&MemberExpr> for MemberExprRef<'_> {
    fn eq(&self, other: &&MemberExpr) -> bool {
        *other == self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum MemberSegment {
    /// An attribute access, e.g. `.y` in `x.y`
    Attribute(Name),
    /// An integer-based index access, e.g. `[1]` in `x[1]`
    IntSubscript(ast::Int),
    /// A string-based index access, e.g. `["foo"]` in `x["foo"]`
    StringSubscript(String),
}

/// Reference to a member expression.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub(crate) struct MemberExprRef<'a> {
    name: &'a str,
    segments: &'a [MemberSegment],
}

impl<'a> MemberExprRef<'a> {
    pub(super) fn symbol_name(&self) -> &'a str {
        self.name
    }

    /// Create a new `MemberExprRef` from a name and segments.
    ///
    /// Note that the segments are expected to be in reverse order, i.e. the last segment is the first one in the expression.
    pub(super) fn from_raw(name: &'a str, segments: &'a [MemberSegment]) -> Self {
        debug_assert!(
            !segments.is_empty(),
            "A member without segments is a symbol."
        );
        Self { name, segments }
    }

    /// Returns a slice over the member segments. The segments are in reverse order,
    pub(super) fn rev_member_segments(&self) -> &'a [MemberSegment] {
        self.segments
    }
}

impl<'a> From<&'a MemberExpr> for MemberExprRef<'a> {
    fn from(value: &'a MemberExpr) -> Self {
        value.as_ref()
    }
}

/// Uniquely identifies a member in a scope.
#[newtype_index]
#[derive(get_size2::GetSize, salsa::Update)]
pub struct ScopedMemberId;

/// The members of a scope. Allows lookup by member path and [`ScopedMemberId`].
#[derive(Default, get_size2::GetSize)]
pub(super) struct MemberTable {
    members: IndexVec<ScopedMemberId, Member>,

    /// Map from member path to its ID.
    ///
    /// Uses a hash table to avoid storing the path twice.
    map: hashbrown::HashTable<ScopedMemberId>,
}

impl MemberTable {
    /// Returns the member with the given ID.
    ///
    /// ## Panics
    /// If the ID is not valid for this table.
    #[track_caller]
    pub(crate) fn member(&self, id: ScopedMemberId) -> &Member {
        &self.members[id]
    }

    /// Returns a mutable reference to the member with the given ID.
    ///
    /// ## Panics
    /// If the ID is not valid for this table.
    #[track_caller]
    pub(super) fn member_mut(&mut self, id: ScopedMemberId) -> &mut Member {
        &mut self.members[id]
    }

    /// Returns an iterator over all members in the table.
    pub(crate) fn iter(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    fn hash_member_expression_ref(member: MemberExprRef) -> u64 {
        let mut h = FxHasher::default();
        member.hash(&mut h);
        h.finish()
    }

    /// Returns the ID of the member with the given expression, if it exists.
    pub(crate) fn member_id<'a>(
        &self,
        member: impl Into<MemberExprRef<'a>>,
    ) -> Option<ScopedMemberId> {
        let member = member.into();
        let hash = Self::hash_member_expression_ref(member);
        self.map
            .find(hash, |id| self.members[*id].expression == member)
            .copied()
    }

    pub(crate) fn place_id_by_instance_attribute_name(&self, name: &str) -> Option<ScopedMemberId> {
        for (id, member) in self.members.iter_enumerated() {
            if member.is_instance_attribute_named(name) {
                return Some(id);
            }
        }

        None
    }
}

impl PartialEq for MemberTable {
    fn eq(&self, other: &Self) -> bool {
        // It's sufficient to compare the members as the map is only a reverse lookup.
        self.members == other.members
    }
}

impl Eq for MemberTable {}

impl std::fmt::Debug for MemberTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MemberTable").field(&self.members).finish()
    }
}

#[derive(Debug, Default)]
pub(super) struct MemberTableBuilder {
    table: MemberTable,
}

impl MemberTableBuilder {
    /// Adds a member to the table or updates the flags of an existing member if it already exists.
    ///
    /// Members are identified by their expression, which is hashed to find the entry in the table.
    pub(super) fn add(&mut self, mut member: Member) -> (ScopedMemberId, bool) {
        let hash = MemberTable::hash_member_expression_ref(member.expression.as_ref());
        let entry = self.table.map.entry(
            hash,
            |id| self.table.members[*id].expression == member.expression,
            |id| {
                MemberTable::hash_member_expression_ref(self.table.members[*id].expression.as_ref())
            },
        );

        match entry {
            Entry::Occupied(entry) => {
                let id = *entry.get();

                if !member.flags.is_empty() {
                    self.members[id].flags.insert(member.flags);
                }

                (id, false)
            }
            Entry::Vacant(entry) => {
                member.expression.shrink_to_fit();

                let id = self.table.members.push(member);
                entry.insert(id);
                (id, true)
            }
        }
    }

    pub(super) fn build(self) -> MemberTable {
        let mut table = self.table;
        table.members.shrink_to_fit();
        table.map.shrink_to_fit(|id| {
            MemberTable::hash_member_expression_ref(table.members[*id].expression.as_ref())
        });
        table
    }
}

impl Deref for MemberTableBuilder {
    type Target = MemberTable;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl DerefMut for MemberTableBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.table
    }
}
