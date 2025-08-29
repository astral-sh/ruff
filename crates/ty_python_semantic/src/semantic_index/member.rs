use bitflags::bitflags;
use hashbrown::hash_table::Entry;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{TextLen as _, TextRange, TextSize};
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use std::hash::{Hash, Hasher as _};
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
    pub(crate) fn symbol_name(&self) -> &str {
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
    pub(super) fn as_instance_attribute_candidate(&self) -> Option<&str> {
        let mut segments = self.expression().segments();
        let first_segment = segments.next()?;

        if first_segment.kind == SegmentKind::Attribute && segments.next().is_none() {
            Some(first_segment.text)
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
        self.as_instance_attribute() == Some(name)
    }

    /// Return `Some(<ATTRIBUTE>)` if the place expression is an instance attribute.
    pub(crate) fn as_instance_attribute(&self) -> Option<&str> {
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
/// Uses a compact representation where the entire expression is stored as a single path.
/// For example, `foo.bar[0]["baz"]` is stored as:
/// - path: `foobar0baz`
/// - segments: stores where each segment starts and its kind (attribute, int subscript, string subscript)
///
/// The symbol name can be extracted from the path by taking the text up to the first segment's start offset.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct MemberExpr {
    /// The entire path as a single Name
    path: Name,
    /// Metadata for each segment (in forward order)
    segments: Segments,
}

impl MemberExpr {
    pub(super) fn try_from_expr(expression: ast::ExprRef<'_>) -> Option<Self> {
        fn visit(expr: ast::ExprRef) -> Option<(Name, SmallVec<[SegmentInfo; 8]>)> {
            use std::fmt::Write as _;

            match expr {
                ast::ExprRef::Name(name) => {
                    Some((name.id.clone(), smallvec::SmallVec::new_const()))
                }
                ast::ExprRef::Attribute(attribute) => {
                    let (mut path, mut segments) = visit(ast::ExprRef::from(&attribute.value))?;

                    let start_offset = path.text_len();
                    let _ = write!(path, "{}", attribute.attr.id);
                    segments.push(SegmentInfo::new(SegmentKind::Attribute, start_offset));

                    Some((path, segments))
                }
                ast::ExprRef::Subscript(subscript) => {
                    let (mut path, mut segments) = visit((&subscript.value).into())?;
                    let start_offset = path.text_len();

                    match &*subscript.slice {
                        ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                            value: ast::Number::Int(index),
                            ..
                        }) => {
                            let _ = write!(path, "{index}");
                            segments
                                .push(SegmentInfo::new(SegmentKind::IntSubscript, start_offset));
                        }
                        ast::Expr::StringLiteral(string) => {
                            let _ = write!(path, "{}", string.value);
                            segments
                                .push(SegmentInfo::new(SegmentKind::StringSubscript, start_offset));
                        }
                        _ => {
                            return None;
                        }
                    }

                    Some((path, segments))
                }
                _ => None,
            }
        }

        let (path, segments) = visit(expression)?;

        if segments.is_empty() {
            None
        } else {
            Some(Self {
                path,
                segments: Segments::from_vec(segments),
            })
        }
    }

    fn segment_infos(&self) -> impl Iterator<Item = SegmentInfo> + '_ {
        self.segments.iter()
    }

    fn segments(&self) -> impl Iterator<Item = Segment<'_>> + '_ {
        SegmentsIterator::new(self.path.as_str(), self.segment_infos())
    }

    fn shrink_to_fit(&mut self) {
        self.path.shrink_to_fit();
    }

    /// Returns the left most part of the member expression, e.g. `x` in `x.y.z`.
    ///
    /// This is the symbol on which the member access is performed.
    pub(crate) fn symbol_name(&self) -> &str {
        self.as_ref().symbol_name()
    }

    pub(super) fn num_segments(&self) -> usize {
        self.segments.len()
    }

    pub(crate) fn as_ref(&self) -> MemberExprRef<'_> {
        MemberExprRef {
            path: self.path.as_str(),
            segments: SegmentsRef::from(&self.segments),
        }
    }
}

impl std::fmt::Display for MemberExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol_name())?;

        for segment in self.segments() {
            match segment.kind {
                SegmentKind::Attribute => write!(f, ".{}", segment.text)?,
                SegmentKind::IntSubscript => write!(f, "[{}]", segment.text)?,
                SegmentKind::StringSubscript => write!(f, "[\"{}\"]", segment.text)?,
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

/// Reference to a member expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemberExprRef<'a> {
    path: &'a str,
    segments: SegmentsRef<'a>,
}

impl<'a> MemberExprRef<'a> {
    pub(super) fn symbol_name(&self) -> &'a str {
        let end = self
            .segments
            .iter()
            .next()
            .map(SegmentInfo::offset)
            .unwrap_or(self.path.text_len());

        let range = TextRange::new(TextSize::default(), end);

        &self.path[range]
    }

    #[cfg(test)]
    fn segments(&self) -> impl Iterator<Item = Segment<'_>> + '_ {
        SegmentsIterator::new(self.path, self.segments.iter())
    }

    pub(super) fn parent(&self) -> Option<MemberExprRef<'a>> {
        let parent_segments = self.segments.parent()?;

        // The removed segment is always the last one. Find its start offset.
        let last_segment = self.segments.iter().last()?;
        let path_end = last_segment.offset();

        Some(MemberExprRef {
            path: &self.path[TextRange::new(TextSize::default(), path_end)],
            segments: parent_segments,
        })
    }
}

impl<'a> From<&'a MemberExpr> for MemberExprRef<'a> {
    fn from(value: &'a MemberExpr) -> Self {
        value.as_ref()
    }
}

impl Hash for MemberExprRef<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Path on its own isn't 100% unique, but it should avoid
        // most collisions and avoids iterating all segments.
        self.path.hash(state);
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
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Member> {
        self.members.iter()
    }

    fn hash_member_expression_ref(member: &MemberExprRef) -> u64 {
        hash_single(member)
    }

    /// Returns the ID of the member with the given expression, if it exists.
    pub(crate) fn member_id<'a>(
        &self,
        member: impl Into<MemberExprRef<'a>>,
    ) -> Option<ScopedMemberId> {
        let member = member.into();
        let hash = Self::hash_member_expression_ref(&member);
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
        let member_ref = member.expression.as_ref();
        let hash = MemberTable::hash_member_expression_ref(&member_ref);
        let entry = self.table.map.entry(
            hash,
            |id| self.table.members[*id].expression.as_ref() == member.expression.as_ref(),
            |id| {
                let ref_expr = self.table.members[*id].expression.as_ref();
                MemberTable::hash_member_expression_ref(&ref_expr)
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
            let ref_expr = table.members[*id].expression.as_ref();
            MemberTable::hash_member_expression_ref(&ref_expr)
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

/// Representation of segments that can be either inline or heap-allocated.
///
/// Design choices:
/// - Uses `Box<[SegmentInfo]>` instead of `ThinVec` because even with a `ThinVec`, the size of `Segments` is still 128 bytes.
/// - Uses u64 for inline storage. That's the largest size without increasing the overall size of `Segments` and allows to encode up to 7 segments.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
enum Segments {
    /// Inline storage for up to 7 segments with 6-bit relative offsets (max 63 bytes per segment)
    Small(SmallSegments),
    /// Heap storage for expressions that don't fit inline
    Heap(Box<[SegmentInfo]>),
}

static_assertions::assert_eq_size!(SmallSegments, u64);
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Segments, [u64; 2]);

impl Segments {
    fn from_vec(segments: SmallVec<[SegmentInfo; 8]>) -> Self {
        debug_assert!(
            !segments.is_empty(),
            "Segments cannot be empty. A member without segments is a symbol"
        );
        if let Some(small) = SmallSegments::try_from_slice(&segments) {
            Self::Small(small)
        } else {
            Self::Heap(segments.into_vec().into_boxed_slice())
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Small(small) => small.len(),
            Self::Heap(segments) => segments.len(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = SegmentInfo> + '_ {
        match self {
            Self::Small(small) => itertools::Either::Left(small.iter()),
            Self::Heap(heap) => itertools::Either::Right(heap.iter().copied()),
        }
    }
}

/// Segment metadata - packed into a single u32
/// Layout: [kind: 2 bits][offset: 30 bits]
/// - Bits 0-1: `SegmentKind` (0=Attribute, 1=IntSubscript, 2=StringSubscript)
/// - Bits 2-31: Absolute offset from start of path (up to 1,073,741,823 bytes)
#[derive(Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
struct SegmentInfo(u32);

const KIND_MASK: u32 = 0b11;
const OFFSET_SHIFT: u32 = 2;
const MAX_OFFSET: u32 = (1 << 30) - 1; // 2^30 - 1

impl SegmentInfo {
    const fn new(kind: SegmentKind, offset: TextSize) -> Self {
        assert!(offset.to_u32() < MAX_OFFSET);

        let value = (offset.to_u32() << OFFSET_SHIFT) | (kind as u32);
        Self(value)
    }

    const fn kind(self) -> SegmentKind {
        match self.0 & KIND_MASK {
            0 => SegmentKind::Attribute,
            1 => SegmentKind::IntSubscript,
            2 => SegmentKind::StringSubscript,
            _ => panic!("Invalid SegmentKind bits"),
        }
    }

    const fn offset(self) -> TextSize {
        TextSize::new(self.0 >> OFFSET_SHIFT)
    }
}

impl std::fmt::Debug for SegmentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SegmentInfo")
            .field("kind", &self.kind())
            .field("offset", &self.offset())
            .finish()
    }
}

struct Segment<'a> {
    kind: SegmentKind,
    text: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
#[repr(u8)]
enum SegmentKind {
    Attribute = 0,
    IntSubscript = 1,
    StringSubscript = 2,
}

/// Iterator over segments that converts `SegmentInfo` to `Segment` with text slices.
struct SegmentsIterator<'a, I> {
    path: &'a str,
    segment_infos: I,
    current: Option<SegmentInfo>,
    next: Option<SegmentInfo>,
}

impl<'a, I> SegmentsIterator<'a, I>
where
    I: Iterator<Item = SegmentInfo>,
{
    fn new(path: &'a str, mut segment_infos: I) -> Self {
        let current = segment_infos.next();
        let next = segment_infos.next();

        Self {
            path,
            segment_infos,
            current,
            next,
        }
    }
}

impl<'a, I> Iterator for SegmentsIterator<'a, I>
where
    I: Iterator<Item = SegmentInfo>,
{
    type Item = Segment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let info = self.current.take()?;
        let end = self
            .next
            .map(SegmentInfo::offset)
            .unwrap_or(self.path.text_len());

        self.current = self.next;
        self.next = self.segment_infos.next();

        Some(Segment {
            kind: info.kind(),
            text: &self.path[TextRange::new(info.offset(), end)],
        })
    }
}

const INLINE_COUNT_BITS: u32 = 3;
const INLINE_COUNT_MASK: u64 = (1 << INLINE_COUNT_BITS) - 1;
const INLINE_SEGMENT_BITS: u32 = 8;
const INLINE_SEGMENT_MASK: u64 = (1 << INLINE_SEGMENT_BITS) - 1;
const INLINE_KIND_BITS: u32 = 2;
const INLINE_KIND_MASK: u64 = (1 << INLINE_KIND_BITS) - 1;
const INLINE_PREV_LEN_BITS: u32 = 6;
const INLINE_PREV_LEN_MASK: u64 = (1 << INLINE_PREV_LEN_BITS) - 1;
const INLINE_MAX_SEGMENTS: usize = 7;
const INLINE_MAX_RELATIVE_OFFSET: u32 = (1 << INLINE_PREV_LEN_BITS) - 1; // 63

/// Compact representation that can store up to 7 segments inline in a u64.
///
/// Layout:
/// - Bits 0-2: Number of segments minus 1 (0-6, representing 1-7 segments)
/// - Bits 3-10: Segment 0 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 11-18: Segment 1 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 19-26: Segment 2 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 27-34: Segment 3 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 35-42: Segment 4 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 43-50: Segment 5 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 51-58: Segment 6 (2 bits kind + 6 bits relative offset, max 63 bytes)
/// - Bits 59-63: Unused (5 bits)
///
/// Constraints:
/// - Maximum 7 segments (realistic limit for member access chains)
/// - Maximum 63-byte relative offset per segment (sufficient for most identifiers)
/// - Never empty (`segments.len()` >= 1)
///
#[derive(Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
#[repr(transparent)]
struct SmallSegments(u64);

impl SmallSegments {
    fn try_from_slice(segments: &[SegmentInfo]) -> Option<Self> {
        if segments.is_empty() || segments.len() > INLINE_MAX_SEGMENTS {
            return None;
        }

        // Pack into inline representation
        // Store count minus 1 (since segments are never empty, range 0-6 represents 1-7 segments)
        let mut packed = (segments.len() - 1) as u64;
        let mut prev_offset = TextSize::new(0);

        for (i, segment) in segments.iter().enumerate() {
            // Compute relative offset on-the-fly
            let relative_offset = segment.offset() - prev_offset;
            if relative_offset > TextSize::from(INLINE_MAX_RELATIVE_OFFSET) {
                return None;
            }

            let kind = segment.kind() as u64;
            let relative_offset_val = u64::from(relative_offset.to_u32());
            let segment_data = (relative_offset_val << INLINE_KIND_BITS) | kind;
            let shift = INLINE_COUNT_BITS
                + (u32::try_from(i).expect("i is bounded by INLINE_MAX_SEGMENTS")
                    * INLINE_SEGMENT_BITS);
            packed |= segment_data << shift;

            prev_offset = segment.offset();
        }

        Some(Self(packed))
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "INLINE_COUNT_MASK ensures value is at most 7"
    )]
    const fn len(self) -> usize {
        // Add 1 because we store count minus 1
        ((self.0 & INLINE_COUNT_MASK) + 1) as usize
    }

    fn iter(self) -> SmallSegmentsInfoIterator {
        SmallSegmentsInfoIterator {
            segments: self,
            index: 0,
            next_offset: TextSize::new(0),
        }
    }

    /// Returns the parent member expression, e.g. `x.b` from `x.b.c`, or `None` if the parent is
    /// the `symbol` itself (e, g. parent of `x.a` is just `x`).
    const fn parent(self) -> Option<Self> {
        let len = self.len();
        if len <= 1 {
            return None;
        }

        // Simply copy the packed value but update the count
        let mut new_packed = self.0;

        // Clear the count bits and set the new count (len - 2, since we store count - 1)
        new_packed &= !INLINE_COUNT_MASK;
        new_packed |= (len - 2) as u64;

        Some(Self(new_packed))
    }
}

impl std::fmt::Debug for SmallSegments {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

struct SmallSegmentsInfoIterator {
    segments: SmallSegments,
    index: usize,
    next_offset: TextSize,
}

impl Iterator for SmallSegmentsInfoIterator {
    type Item = SegmentInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let count = self.segments.len();
        if self.index >= count {
            return None;
        }

        // Extract the relative offset and kind for the current segment
        let shift = INLINE_COUNT_BITS
            + (u32::try_from(self.index).expect("index is bounded by INLINE_MAX_SEGMENTS")
                * INLINE_SEGMENT_BITS);
        let segment_data = (self.segments.0 >> shift) & INLINE_SEGMENT_MASK;
        let kind = (segment_data & INLINE_KIND_MASK) as u8;
        let relative_offset = ((segment_data >> INLINE_KIND_BITS) & INLINE_PREV_LEN_MASK) as u32;

        // Update the running absolute offset
        self.next_offset += TextSize::new(relative_offset);

        let kind = match kind {
            0 => SegmentKind::Attribute,
            1 => SegmentKind::IntSubscript,
            2 => SegmentKind::StringSubscript,
            _ => panic!("Invalid SegmentKind bits"),
        };

        self.index += 1;
        Some(SegmentInfo::new(kind, self.next_offset))
    }
}

/// Reference view of segments, can be either small (inline) or heap-allocated.
#[derive(Clone, Copy, Debug)]
enum SegmentsRef<'a> {
    Small(SmallSegments),
    Heap(&'a [SegmentInfo]),
}

impl<'a> SegmentsRef<'a> {
    fn len(&self) -> usize {
        match self {
            Self::Small(small) => small.len(),
            Self::Heap(segments) => segments.len(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = SegmentInfo> + '_ {
        match self {
            Self::Small(small) => itertools::Either::Left(small.iter()),
            Self::Heap(heap) => itertools::Either::Right(heap.iter().copied()),
        }
    }

    /// Returns a parent view with one fewer segment, or None if <= 1 segment
    fn parent(&self) -> Option<SegmentsRef<'a>> {
        match self {
            Self::Small(small) => small.parent().map(SegmentsRef::Small),
            Self::Heap(segments) => {
                let len = segments.len();
                if len <= 1 {
                    None
                } else {
                    Some(SegmentsRef::Heap(&segments[..len - 1]))
                }
            }
        }
    }
}

impl<'a> From<&'a Segments> for SegmentsRef<'a> {
    fn from(segments: &'a Segments) -> Self {
        match segments {
            Segments::Small(small) => SegmentsRef::Small(*small),
            Segments::Heap(heap) => SegmentsRef::Heap(heap),
        }
    }
}

impl PartialEq for SegmentsRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        let len = self.len();
        if len != other.len() {
            return false;
        }
        self.iter().eq(other.iter())
    }
}

impl Eq for SegmentsRef<'_> {}

/// Helper function to hash a single value and return the hash.
fn hash_single<T: Hash>(value: &T) -> u64 {
    let mut hasher = FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_expr_ref_hash_and_eq_small_heap() {
        // For expression: foo.bar[0]["baz"]
        // The path would be: "foobar0baz" (no dots or brackets in the path)
        let path = "foobar0baz";

        let segments = vec![
            SegmentInfo::new(SegmentKind::Attribute, TextSize::new(3)), // .bar at offset 3
            SegmentInfo::new(SegmentKind::IntSubscript, TextSize::new(6)), // [0] at offset 6
            SegmentInfo::new(SegmentKind::StringSubscript, TextSize::new(7)), // ["baz"] at offset 7
        ];

        // Create Small version.
        let small_segments = SmallSegments::try_from_slice(&segments).unwrap();
        let member_ref_small = MemberExprRef {
            path,
            segments: SegmentsRef::Small(small_segments),
        };

        // Create Heap version with the same data.
        let heap_segments: Box<[SegmentInfo]> = segments.into_boxed_slice();
        let member_ref_heap = MemberExprRef {
            path,
            segments: SegmentsRef::Heap(&heap_segments),
        };

        // Test hash equality (MemberExprRef only hashes the path).
        assert_eq!(
            hash_single(&member_ref_small),
            hash_single(&member_ref_heap)
        );

        // Test equality in both directions.
        assert_eq!(member_ref_small, member_ref_heap);
        assert_eq!(member_ref_heap, member_ref_small);
    }

    #[test]
    fn test_member_expr_ref_different_segments() {
        // For expressions: foo.bar[0] vs foo.bar["0"]
        // Both have the same path "foobar0" but different segment types
        let path = "foobar0";

        // First expression: foo.bar[0]
        let segments1 = vec![
            SegmentInfo::new(SegmentKind::Attribute, TextSize::new(3)), // .bar at offset 3
            SegmentInfo::new(SegmentKind::IntSubscript, TextSize::new(6)), // [0] at offset 6
        ];

        // Second expression: foo.bar["0"]
        let segments2 = vec![
            SegmentInfo::new(SegmentKind::Attribute, TextSize::new(3)), // .bar at offset 3
            SegmentInfo::new(SegmentKind::StringSubscript, TextSize::new(6)), // ["0"] at offset 6
        ];

        // Create MemberExprRef instances
        let small1 = SmallSegments::try_from_slice(&segments1).unwrap();
        let member_ref1 = MemberExprRef {
            path,
            segments: SegmentsRef::Small(small1),
        };

        let small2 = SmallSegments::try_from_slice(&segments2).unwrap();
        let member_ref2 = MemberExprRef {
            path,
            segments: SegmentsRef::Small(small2),
        };

        // Test inequality
        assert_ne!(member_ref1, member_ref2);
        assert_ne!(member_ref2, member_ref1);

        // Test hash equality (MemberExprRef only hashes the path, not segments)
        assert_eq!(hash_single(&member_ref1), hash_single(&member_ref2));
    }

    #[test]
    fn test_member_expr_ref_parent() {
        use ruff_python_parser::parse_expression;

        // Parse a real Python expression
        let parsed = parse_expression(r#"foo.bar[0]["baz"]"#).unwrap();
        let expr = parsed.expr();

        // Convert to MemberExpr
        let member_expr = MemberExpr::try_from_expr(ast::ExprRef::from(expr)).unwrap();
        let member_ref = member_expr.as_ref();

        // Verify the initial state: foo.bar[0]["baz"]
        assert_eq!(member_ref.symbol_name(), "foo");
        let segments: Vec<_> = member_ref.segments().map(|s| (s.kind, s.text)).collect();
        assert_eq!(
            segments,
            vec![
                (SegmentKind::Attribute, "bar"),
                (SegmentKind::IntSubscript, "0"),
                (SegmentKind::StringSubscript, "baz")
            ]
        );

        // Test parent() removes the last segment ["baz"] -> foo.bar[0]
        let parent1 = member_ref.parent().unwrap();
        assert_eq!(parent1.symbol_name(), "foo");
        let parent1_segments: Vec<_> = parent1.segments().map(|s| (s.kind, s.text)).collect();
        assert_eq!(
            parent1_segments,
            vec![
                (SegmentKind::Attribute, "bar"),
                (SegmentKind::IntSubscript, "0")
            ]
        );

        // Test parent of parent removes [0] -> foo.bar
        let parent2 = parent1.parent().unwrap();
        assert_eq!(parent2.symbol_name(), "foo");
        let parent2_segments: Vec<_> = parent2.segments().map(|s| (s.kind, s.text)).collect();
        assert_eq!(parent2_segments, vec![(SegmentKind::Attribute, "bar")]);

        // Test parent of single segment is a symbol and not a member.
        let parent3 = parent2.parent();
        assert!(parent3.is_none());
    }

    #[test]
    fn test_member_expr_small_vs_heap_allocation() {
        use ruff_python_parser::parse_expression;

        // Test Small allocation: 7 segments (maximum for inline storage)
        // Create expression with exactly 7 segments: x.a.b.c.d.e.f.g
        let small_expr = parse_expression("x.a.b.c.d.e.f.g").unwrap();
        let small_member =
            MemberExpr::try_from_expr(ast::ExprRef::from(small_expr.expr())).unwrap();

        // Should use Small allocation
        assert!(matches!(small_member.segments, Segments::Small(_)));
        assert_eq!(small_member.num_segments(), 7);

        // Test Heap allocation: 8 segments (exceeds inline capacity)
        // Create expression with 8 segments: x.a.b.c.d.e.f.g.h
        let heap_expr = parse_expression("x.a.b.c.d.e.f.g.h").unwrap();
        let heap_member = MemberExpr::try_from_expr(ast::ExprRef::from(heap_expr.expr())).unwrap();

        // Should use Heap allocation
        assert!(matches!(heap_member.segments, Segments::Heap(_)));
        assert_eq!(heap_member.num_segments(), 8);

        // Test Small allocation with relative offset limit
        // Create expression where relative offsets are small enough: a.b[0]["c"]
        let small_offset_expr = parse_expression(r#"a.b[0]["c"]"#).unwrap();
        let small_offset_member =
            MemberExpr::try_from_expr(ast::ExprRef::from(small_offset_expr.expr())).unwrap();

        // Should use Small allocation (3 segments, small offsets)
        assert!(matches!(small_offset_member.segments, Segments::Small(_)));
        assert_eq!(small_offset_member.num_segments(), 3);

        // Test Small allocation with maximum 63-byte relative offset limit
        // Create expression where one segment has exactly 63 bytes (the limit)
        let segment_63_bytes = "a".repeat(63);
        let max_offset_expr_code = format!("x.{segment_63_bytes}.y");
        let max_offset_expr = parse_expression(&max_offset_expr_code).unwrap();
        let max_offset_member =
            MemberExpr::try_from_expr(ast::ExprRef::from(max_offset_expr.expr())).unwrap();
        // Should still use Small allocation (exactly at the limit)
        assert!(matches!(max_offset_member.segments, Segments::Small(_)));
        assert_eq!(max_offset_member.num_segments(), 2);

        // Test that heap allocation works for segment content that would exceed relative offset limits
        // This would require very long identifiers (>63 bytes between segments), which is uncommon
        // but we can test by creating an expression with long attribute names
        let long_name = "a".repeat(64); // 64 bytes (exceeds 63-byte limit)
        let long_expr_code = format!("x.{long_name}.y");
        let long_expr = parse_expression(&long_expr_code).unwrap();
        let long_member = MemberExpr::try_from_expr(ast::ExprRef::from(long_expr.expr())).unwrap();
        // Should use Heap allocation due to large relative offset
        assert!(matches!(long_member.segments, Segments::Heap(_)));
        assert_eq!(long_member.num_segments(), 2);
    }
}
