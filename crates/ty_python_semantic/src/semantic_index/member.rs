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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
#[repr(u8)]
enum SegmentKind {
    Attribute = 0,
    IntSubscript = 1,
    StringSubscript = 2,
}

struct Segment<'a> {
    kind: SegmentKind,
    text: &'a str,
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
const INLINE_MAX_PREV_LEN: u32 = (1 << INLINE_PREV_LEN_BITS) - 1; // 63

/// Compact representation that can store up to 7 segments inline in a u64.
///
/// Layout:
/// - Bits 0-2: Number of segments minus 1 (0-6, representing 1-7 segments)
/// - Bits 3-10: Segment 0 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 11-18: Segment 1 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 19-26: Segment 2 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 27-34: Segment 3 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 35-42: Segment 4 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 43-50: Segment 5 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 51-58: Segment 6 (2 bits kind + 6 bits relative offset, max 64 bytes)
/// - Bits 59-63: Unused (5 bits)
///
/// Constraints:
/// - Maximum 7 segments (realistic limit for member access chains)
/// - Maximum 64-byte relative offset per segment (sufficient for most identifiers)
/// - Never empty (`segments.len()` >= 1, enforced by `MemberExpr` construction)
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
            if relative_offset > TextSize::from(INLINE_MAX_PREV_LEN) {
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

        // Check if we can fit in inline storage
        Some(Self(packed))
    }

    #[allow(clippy::cast_possible_truncation)] // Safe: INLINE_COUNT_MASK ensures value is at most 7
    const fn len(self) -> usize {
        // Add 1 because we store count minus 1
        ((self.0 & INLINE_COUNT_MASK) + 1) as usize
    }

    fn iter(self) -> SmallSegmentsIter {
        SmallSegmentsIter {
            segments: self,
            index: 0,
            next_offset: TextSize::new(0),
        }
    }

    /// Returns a new `SmallSegments` with the last segment removed, or None if only one segment remains
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

struct SmallSegmentsIter {
    segments: SmallSegments,
    index: usize,
    next_offset: TextSize,
}

impl Iterator for SmallSegmentsIter {
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

/// Representation of segments that can be either inline or heap-allocated.
///
/// Design choices:
/// - Uses `Box<[SegmentInfo]>` instead of `ThinVec` for heap allocation to avoid dependency overhead
/// - Uses u64 for inline storage
/// - `ThinVec` + u32 combination doesn't provide enum niche optimization since neither has a zero bit pattern
/// - Final enum size is 16 bytes: optimal given Rust's layout constraints with fat pointer (Box<[T]>)
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
enum Segments {
    /// Inline storage for up to 7 segments with 6-bit relative offsets (max 64 bytes per segment)
    Small(SmallSegments),
    /// Heap storage for expressions that don't fit inline
    Heap(Box<[SegmentInfo]>),
}

// Size assertions for optimization verification
static_assertions::assert_eq_size!(SmallSegments, u64);
static_assertions::assert_eq_size!(Segments, [usize; 2]);

// Final state: Segments enum is 16 bytes
// - SmallSegments: 8 bytes (u64 with 7 segments, 64-byte max relative offset per segment)
// - Box<[SegmentInfo]>: 16 bytes (fat pointer with data ptr + length)
// - Total enum: 16 bytes (no niche optimization possible with these sizes)
//
// Benefits achieved:
// 1. Increased inline capacity (7 segments vs previous 5 segments)
// 2. Realistic relative offset range (64 bytes per segment, sufficient for most identifiers)
// 3. Efficient bit usage (58/64 bits used, 5 bits spare for future use)
// 4. Cleaner heap allocation with Box<[T]> (exact size, no capacity waste)
static_assertions::assert_eq_size!(Segments, u128);

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

/// An expression accessing a member on a symbol named `symbol_name`, e.g. `x.y.z`.
///
/// The parts after the symbol name are called segments, and they can be either:
/// * An attribute access, e.g. `.y` in `x.y`
/// * An integer-based subscript, e.g. `[1]` in `x[1]`
/// * A string-based subscript, e.g. `["foo"]` in `x["foo"]`
///
/// Uses a compact representation where the entire expression is stored as a single path.
/// For example, `foo.bar[0]["baz"]` is stored as:
/// - path: Name("foo.bar.0.baz")
/// - segments: metadata describing each segment's type and length (stored as relative offset)
///
/// The symbol name length is computed by subtracting all segment lengths from the total path length.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct MemberExpr {
    /// The entire path as a single Name (uses `CompactString` internally)
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
        let mut infos = self.segment_infos();
        let mut current = infos.next();
        let mut next = infos.next();

        std::iter::from_fn(move || {
            let info = current.take()?;
            let end = next
                .map(SegmentInfo::offset)
                .unwrap_or(self.path.text_len());

            current = next;
            next = infos.next();

            Some(Segment {
                kind: info.kind(),
                text: &self.path[TextRange::new(info.offset(), end)],
            })
        })
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

    pub(crate) fn as_ref(&self) -> MemberExprRef {
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
    pub(crate) fn iter(&self) -> std::slice::Iter<Member> {
        self.members.iter()
    }

    fn hash_member_expression_ref(member: &MemberExprRef) -> u64 {
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
