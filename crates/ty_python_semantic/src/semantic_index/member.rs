use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{self as ast, name::Name};
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use std::hash::{Hash as _, Hasher as _};
use std::ops::{Deref, DerefMut};

use crate::semantic_index::symbol::ScopedSymbolId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum MemberSegment {
    /// An attribute access, e.g. `.y` in `x.y`
    Attribute(ast::name::Name),
    /// An integer-based index access, e.g. `[1]` in `x[1]`
    IntSubscript(ast::Int),
    /// A string-based index access, e.g. `["foo"]` in `x["foo"]`
    StringSubscript(String),
}

impl MemberSegment {
    pub(crate) fn as_attribute(&self) -> Option<&ast::name::Name> {
        match self {
            MemberSegment::Attribute(name) => Some(name),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize, Hash)]
pub(crate) struct MemberExpr {
    root: ScopedSymbolId,
    segments: SmallVec<[MemberSegment; 1]>,
}

impl MemberExpr {
    pub(crate) fn symbol(&self) -> ScopedSymbolId {
        self.root
    }

    pub(crate) fn segments(&self) -> &[MemberSegment] {
        &self.segments
    }

    /// Does the place expression have the form `<object>.attribute`?
    pub fn is_attribute(&self) -> bool {
        self.segments
            .last()
            .is_some_and(|last| last.as_attribute().is_some())
    }

    fn shrink_to_fit(&mut self) {
        self.segments.shrink_to_fit();
    }
}

#[derive(Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct Member {
    expression: MemberExpr,
    flags: MemberFlags,
}

impl Member {
    pub(crate) fn is_attribute(&self) -> bool {
        self.expression.is_attribute()
    }

    /// Is the place used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(MemberFlags::IS_USED)
    }

    /// Is the place given a value in its containing scope?
    pub const fn is_bound(&self) -> bool {
        self.flags.contains(MemberFlags::IS_BOUND)
    }

    /// Is the place declared in its containing scope?
    pub fn is_declared(&self) -> bool {
        self.flags.contains(MemberFlags::IS_DECLARED)
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
        if self.expression.segments.len() == 1 {
            self.expression.segments[0].as_attribute()
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
}

bitflags! {
    /// Flags that can be queried to obtain information about a member in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a member to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct MemberFlags: u8 {
        const IS_USED               = 1 << 0;
        const IS_BOUND              = 1 << 1;
        const IS_DECLARED           = 1 << 2;
        const IS_INSTANCE_ATTRIBUTE = 1 << 3;
        const IS_REASSIGNED         = 1 << 4;
    }
}

impl get_size2::GetSize for MemberFlags {}

#[newtype_index]
#[derive(get_size2::GetSize, salsa::Update)]
pub(crate) struct ScopedMemberId;

#[derive(Default, get_size2::GetSize)]
pub(super) struct MemberTable {
    members: IndexVec<ScopedMemberId, Member>,

    /// Map from member path to it's ID.
    ///
    /// Uses a hash table to avoid storing the path twice.
    map: hashbrown::HashTable<ScopedMemberId>,
}

impl MemberTable {
    #[track_caller]
    pub(crate) fn member(&self, id: ScopedMemberId) -> &Member {
        &self.members[id]
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    pub(crate) fn iter_enumerated(&self) -> impl Iterator<Item = (ScopedMemberId, &Member)> {
        self.members.iter_enumerated()
    }

    fn hash_member_expression(member: &MemberExpr) -> u64 {
        let mut h = FxHasher::default();
        member.hash(&mut h);
        h.finish()
    }

    pub(crate) fn member_id(&self, member: &Member) -> Option<ScopedMemberId> {
        let hash = Self::hash_member_expression(&member.expression);
        self.map
            .find(hash, |id| self.members[*id].expression == member.expression)
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
    // associated_place_ids: IndexVec<ScopedMemberId, SmallVec<[ScopedPlaceId; 4]>>,
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
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => {
                member.expression.shrink_to_fit();

                let id = self.table.members.push(member);
                entry.insert(id);
                // FIXME
                // let new_id = self.associated_place_ids.push(SmallVec::new_const());
                // debug_assert_eq!(new_id, id);

                // FIXME
                // for root in self.table.members[id].expression.root_exprs() {
                //     if let Some(root_id) = self.table.place_id_by_expr(root) {
                //         self.associated_place_ids[root_id].push(id);
                //     }
                // }
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
