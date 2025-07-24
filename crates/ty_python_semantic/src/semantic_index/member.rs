use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{self as ast, name::Name};
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use std::borrow::Borrow;
use std::hash::{Hash as _, Hasher as _};
use std::ops::{Deref, DerefMut};

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

    pub(crate) fn symbol_name(&self) -> &Name {
        self.expression.symbol_name()
    }

    pub(crate) fn expression(&self) -> &MemberExprRef {
        self.expression.as_ref()
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

    pub(super) fn mark_instance_attribute(&mut self) {
        self.flags.insert(MemberFlags::IS_INSTANCE_ATTRIBUTE);
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
        match self.expression.0.as_slice() {
            [MemberSegment::Attribute(name), _] => {
                // The last segment is a symbol, the second is an attribute.
                Some(name)
            }
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
        const IS_REASSIGNED         = 1 << 3;
    }
}

impl get_size2::GetSize for MemberFlags {}

/// The member expression consists of different segments, e.g. `x.y.z` is represented
/// as `Symbol(x), Attribute(y), Attribute(z)` segments (in that order).
///
/// The first segment is always a `Symbol`, followed by at least one attribute segment.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize, Hash)]
#[repr(transparent)]
pub(crate) struct MemberExpr(SmallVec<[MemberSegment; 2]>);

impl MemberExpr {
    pub(super) fn new(segments: SmallVec<[MemberSegment; 2]>) -> Self {
        debug_assert!(
            segments
                .last()
                .is_some_and(|segment| matches!(segment, MemberSegment::Symbol(_)))
        );

        Self(segments)
    }

    fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit();
    }

    fn as_ref(&self) -> &MemberExprRef {
        MemberExprRef::from_raw(self.0.as_slice())
    }
}

impl std::fmt::Display for MemberExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_ref(), f)
    }
}

impl Borrow<MemberExprRef> for MemberExpr {
    fn borrow(&self) -> &MemberExprRef {
        self.as_ref()
    }
}

impl AsRef<MemberExprRef> for MemberExpr {
    fn as_ref(&self) -> &MemberExprRef {
        self.as_ref()
    }
}

impl PartialEq<MemberExprRef> for MemberExpr {
    fn eq(&self, other: &MemberExprRef) -> bool {
        self.as_ref() == other
    }
}

impl PartialEq<MemberExpr> for MemberExprRef {
    fn eq(&self, other: &MemberExpr) -> bool {
        other == self
    }
}

impl Deref for MemberExpr {
    type Target = MemberExprRef;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum MemberSegment {
    /// The root symbol, e.g. `x` in `x.y`
    Symbol(Name),
    /// An attribute access, e.g. `.y` in `x.y`
    Attribute(Name),
    /// An integer-based index access, e.g. `[1]` in `x[1]`
    IntSubscript(ast::Int),
    /// A string-based index access, e.g. `["foo"]` in `x["foo"]`
    StringSubscript(String),
}

/// Reference to a member expression.
///
/// `MemberExprRef` is to a `MemberExpr` what `str` is to `String`.
#[derive(Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(crate) struct MemberExprRef([MemberSegment]);

impl MemberExprRef {
    #[inline]
    #[allow(unsafe_code)]
    pub(super) const fn from_raw(raw: &[MemberSegment]) -> &Self {
        let ptr: *const [MemberSegment] = raw;

        #[expect(unsafe_code)]
        // SAFETY: `MemberExprRef` is `repr(transparent)` over a normal slice
        unsafe {
            &*(ptr as *const Self)
        }
    }

    pub(crate) fn symbol_name(&self) -> &Name {
        match self.0.last().unwrap() {
            MemberSegment::Symbol(name) => name,
            MemberSegment::Attribute(_)
            | MemberSegment::IntSubscript(_)
            | MemberSegment::StringSubscript(_) => {
                unreachable!("The last segment is always a symbol segment")
            }
        }
    }

    pub(crate) fn segments(
        &self,
    ) -> impl ExactSizeIterator<Item = &MemberSegment> + DoubleEndedIterator {
        self.0.iter().rev()
    }

    pub(super) const fn rev_segments_slice(&self) -> &[MemberSegment] {
        &self.0
    }
}

impl std::fmt::Display for MemberExprRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for segment in self.segments() {
            match segment {
                MemberSegment::Symbol(name) => write!(f, "{name}")?,
                MemberSegment::Attribute(name) => write!(f, ".{name}")?,
                MemberSegment::IntSubscript(int) => write!(f, "[{int}]")?,
                MemberSegment::StringSubscript(string) => write!(f, "[\"{string}\"]")?,
            }
        }

        Ok(())
    }
}

#[newtype_index]
#[derive(get_size2::GetSize, salsa::Update)]
pub struct ScopedMemberId;

#[derive(Default, get_size2::GetSize)]
pub(super) struct MemberTable {
    members: IndexVec<ScopedMemberId, Member>,

    /// Map from member path to its ID.
    ///
    /// Uses a hash table to avoid storing the path twice.
    map: hashbrown::HashTable<ScopedMemberId>,
}

impl MemberTable {
    #[track_caller]
    pub(crate) fn member(&self, id: ScopedMemberId) -> &Member {
        &self.members[id]
    }

    #[track_caller]
    pub(super) fn member_mut(&mut self, id: ScopedMemberId) -> &mut Member {
        &mut self.members[id]
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    fn hash_member_expression(member: &MemberExprRef) -> u64 {
        let mut h = FxHasher::default();
        member.hash(&mut h);
        h.finish()
    }

    pub(crate) fn member_id(&self, member: &MemberExprRef) -> Option<ScopedMemberId> {
        let hash = Self::hash_member_expression(member);
        self.map
            .find(hash, |id| &self.members[*id].expression == member)
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
        // It's sufficient to compare the symbols as the map is only a reverse lookup.
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
    pub(super) fn add(&mut self, mut member: Member) -> (ScopedMemberId, bool) {
        let hash = MemberTable::hash_member_expression(&member.expression);
        let entry = self.table.map.entry(
            hash,
            |id| self.table.members[*id].expression == member.expression,
            |id| MemberTable::hash_member_expression(&self.table.members[*id].expression),
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
            MemberTable::hash_member_expression(&table.members[*id].expression)
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
