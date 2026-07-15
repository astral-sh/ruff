use std::borrow::{Borrow, Cow};
use std::fmt::{Debug, Display, Formatter, Write};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use arrayvec::ArrayVec;
use char_str::{CharStr, CharString};

use crate::Expr;
use crate::generated::ExprName;

/// An immutable name.
///
/// # Choosing a string representation
///
/// On 64-bit targets, [`CharStr`] occupies 16 bytes and stores up to 16 UTF-8 bytes inline. Longer
/// values use an exactly-sized, reference-counted allocation, so cloning a heap-backed value
/// reuses its allocation. [`compact_str::CompactString`] occupies 24 bytes, stores up to 24 bytes
/// inline, and remains mutable; cloning a heap-backed value copies its contents into a new
/// allocation.
///
/// Prefer `CharStr` for immutable text that is retained densely or passed between owners, when
/// either the smaller handle or structural sharing offsets the extra heap allocations for values
/// between 17 and 24 bytes. Prefer `CompactString` for uniquely owned text, especially when it is
/// built incrementally, mutated, or commonly falls in that 17-to-24-byte range.
///
/// `Name` uses `CharStr` because names appear throughout the AST and repeated heap-backed parser
/// names share an allocation. By contrast, [`crate::DebugText`] uses `CompactString` because it
/// builds a uniquely owned buffer incrementally, and `ty_module_resolver::ModuleName` uses
/// `CompactString` because module names can be extended in place.
///
/// Converting a borrowed `&str` into `CharStr` creates a new value and does not preserve structural
/// sharing. When an API retains text already held in a `CharStr` (including a `Name`), pass or clone
/// the owned value rather than converting it through `&str`. This is especially relevant at Salsa
/// interning boundaries.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "salsa", derive(salsa::SalsaValue))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "get-size", derive(get_size2::GetSize))]
#[cfg_attr(
    feature = "schemars",
    derive(schemars::JsonSchema),
    schemars(with = "String")
)]
pub struct Name(CharStr);

impl Name {
    /// The maximum number of UTF-8 bytes stored inline in a name.
    pub const INLINE_CAPACITY: usize = CharStr::INLINE_CAPACITY;

    #[inline]
    pub fn empty() -> Self {
        Self(CharStr::new())
    }

    #[inline]
    pub fn new(name: impl AsRef<str>) -> Self {
        Self(CharStr::from(name.as_ref()))
    }

    #[inline]
    pub const fn new_static(name: &'static str) -> Self {
        Self(CharStr::from_static_str(name))
    }

    /// Creates a name by joining string slices with a separator.
    #[inline]
    pub fn join<T: AsRef<str>>(slices: &[T], separator: &str) -> Self {
        Self(CharStr::join(slices, separator))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Debug for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Name({:?})", self.as_str())
    }
}

impl AsRef<str> for Name {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Name {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Borrow<str> for Name {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a> From<&'a str> for Name {
    #[inline]
    fn from(s: &'a str) -> Self {
        Name::new(s)
    }
}

impl From<String> for Name {
    #[inline]
    fn from(s: String) -> Self {
        Name(s.into())
    }
}

impl<'a> From<&'a String> for Name {
    #[inline]
    fn from(s: &'a String) -> Self {
        Name::new(s)
    }
}

impl<'a> From<Cow<'a, str>> for Name {
    #[inline]
    fn from(cow: Cow<'a, str>) -> Self {
        Name(cow.into())
    }
}

impl From<Box<str>> for Name {
    #[inline]
    fn from(b: Box<str>) -> Self {
        Name(b.into())
    }
}

#[cfg(feature = "salsa")]
impl salsa::Lookup<Name> for &str {
    #[inline]
    fn into_owned(self) -> Name {
        Name::new(self)
    }
}

#[cfg(feature = "salsa")]
impl salsa::HashEqLike<&str> for Name {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }

    #[inline]
    fn eq(&self, data: &&str) -> bool {
        self.as_str() == *data
    }
}

impl From<Name> for String {
    #[inline]
    fn from(name: Name) -> Self {
        name.0.into()
    }
}

impl From<Name> for CharStr {
    #[inline]
    fn from(name: Name) -> Self {
        name.0
    }
}

impl From<CharString> for Name {
    #[inline]
    fn from(name: CharString) -> Self {
        Self(name.freeze())
    }
}

impl FromIterator<char> for Name {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(feature = "cache")]
impl ruff_cache::CacheKey for Name {
    fn cache_key(&self, state: &mut ruff_cache::CacheKeyHasher) {
        ruff_cache::CacheKey::cache_key(self.as_str(), state);
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for Name {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<Name> for str {
    #[inline]
    fn eq(&self, other: &Name) -> bool {
        other == self
    }
}

impl PartialEq<&str> for Name {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Name> for &str {
    #[inline]
    fn eq(&self, other: &Name) -> bool {
        other == self
    }
}

impl PartialEq<String> for Name {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<Name> for String {
    #[inline]
    fn eq(&self, other: &Name) -> bool {
        other == self
    }
}

impl PartialEq<&String> for Name {
    #[inline]
    fn eq(&self, other: &&String) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Name> for &String {
    #[inline]
    fn eq(&self, other: &Name) -> bool {
        other == self
    }
}

/// A representation of a qualified name, like `typing.List`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedName<'a>(SegmentsVec<'a>);

impl<'a> QualifiedName<'a> {
    /// Create a [`QualifiedName`] from a dotted name.
    ///
    /// ```rust
    /// # use ruff_python_ast::name::QualifiedName;
    ///
    /// assert_eq!(QualifiedName::from_dotted_name("typing.List").segments(), ["typing", "List"]);
    /// assert_eq!(QualifiedName::from_dotted_name("list").segments(), ["", "list"]);
    /// ```
    #[inline]
    pub fn from_dotted_name(name: &'a str) -> Self {
        if let Some(dot) = name.find('.') {
            let mut builder = QualifiedNameBuilder::default();
            builder.push(&name[..dot]);
            builder.extend(name[dot + 1..].split('.'));
            builder.build()
        } else {
            Self::builtin(name)
        }
    }

    /// Creates a name that's guaranteed not be a built in
    #[inline]
    pub fn user_defined(name: &'a str) -> Self {
        name.split('.').collect()
    }

    /// Creates a qualified name for a built in
    #[inline]
    pub fn builtin(name: &'a str) -> Self {
        debug_assert!(!name.contains('.'));
        Self(SegmentsVec::from_slice(&["", name]))
    }

    #[inline]
    pub fn segments(&self) -> &[&'a str] {
        self.0.as_slice()
    }

    /// If the first segment is empty, the `CallPath` represents a "builtin binding".
    ///
    /// A builtin binding is the binding that a symbol has if it was part of Python's
    /// global scope without any imports taking place. However, if builtin members are
    /// accessed explicitly via the `builtins` module, they will not have a
    /// "builtin binding", so this method will return `false`.
    ///
    /// Ex) `["", "bool"]` -> `"bool"`
    fn is_builtin(&self) -> bool {
        matches!(self.segments(), ["", ..])
    }

    /// If the call path is dot-prefixed, it's an unresolved relative import.
    /// Ex) `[".foo", "bar"]` -> `".foo.bar"`
    pub fn is_unresolved_import(&self) -> bool {
        matches!(self.segments(), [".", ..])
    }

    pub fn starts_with(&self, other: &QualifiedName<'_>) -> bool {
        self.segments().starts_with(other.segments())
    }

    /// Appends a member to the qualified name.
    #[must_use]
    pub fn append_member(self, member: &'a str) -> Self {
        let mut inner = self.0;
        inner.push(member);
        Self(inner)
    }

    /// Extends the qualified name using the given members.
    #[must_use]
    pub fn extend_members<T: IntoIterator<Item = &'a str>>(self, members: T) -> Self {
        let mut inner = self.0;
        inner.extend(members);
        Self(inner)
    }
}

impl Display for QualifiedName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let segments = self.segments();

        if self.is_unresolved_import() {
            let mut iter = segments.iter();
            for segment in iter.by_ref() {
                if *segment == "." {
                    f.write_char('.')?;
                } else {
                    f.write_str(segment)?;
                    break;
                }
            }
            for segment in iter {
                f.write_char('.')?;
                f.write_str(segment)?;
            }
        } else {
            let segments = if self.is_builtin() {
                &segments[1..]
            } else {
                segments
            };

            let mut first = true;
            for segment in segments {
                if !first {
                    f.write_char('.')?;
                }

                f.write_str(segment)?;
                first = false;
            }
        }

        Ok(())
    }
}

impl<'a> FromIterator<&'a str> for QualifiedName<'a> {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        Self(SegmentsVec::from_iter(iter))
    }
}

#[derive(Debug, Clone, Default)]
pub struct QualifiedNameBuilder<'a> {
    segments: SegmentsVec<'a>,
}

impl<'a> QualifiedNameBuilder<'a> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: SegmentsVec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    #[inline]
    pub fn push(&mut self, segment: &'a str) {
        self.segments.push(segment);
    }

    #[inline]
    pub fn pop(&mut self) {
        self.segments.pop();
    }

    #[inline]
    pub fn extend(&mut self, segments: impl IntoIterator<Item = &'a str>) {
        self.segments.extend(segments);
    }

    #[inline]
    pub fn extend_from_slice(&mut self, segments: &[&'a str]) {
        self.segments.extend_from_slice(segments);
    }

    pub fn build(self) -> QualifiedName<'a> {
        QualifiedName(self.segments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnqualifiedName<'a>(SegmentsVec<'a>);

impl<'a> UnqualifiedName<'a> {
    /// Convert an `Expr` to its [`UnqualifiedName`] (like `["typing", "List"]`).
    pub fn from_expr(expr: &'a Expr) -> Option<Self> {
        // Unroll the loop up to eight times, to match the maximum number of expected attributes.
        // In practice, unrolling appears to give about a 4x speed-up on this hot path.
        let attr1 = match expr {
            Expr::Attribute(attr1) => attr1,
            // Ex) `foo`
            Expr::Name(ExprName { id, .. }) => return Some(Self::from_slice(&[id.as_str()])),
            _ => return None,
        };

        let attr2 = match attr1.value.as_ref() {
            Expr::Attribute(attr2) => attr2,
            // Ex) `foo.bar`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[id.as_str(), attr1.attr.as_str()]));
            }
            _ => return None,
        };

        let attr3 = match attr2.value.as_ref() {
            Expr::Attribute(attr3) => attr3,
            // Ex) `foo.bar.baz`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[
                    id.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ]));
            }
            _ => return None,
        };

        let attr4 = match attr3.value.as_ref() {
            Expr::Attribute(attr4) => attr4,
            // Ex) `foo.bar.baz.bop`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[
                    id.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ]));
            }
            _ => return None,
        };

        let attr5 = match attr4.value.as_ref() {
            Expr::Attribute(attr5) => attr5,
            // Ex) `foo.bar.baz.bop.bap`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[
                    id.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ]));
            }
            _ => return None,
        };

        let attr6 = match attr5.value.as_ref() {
            Expr::Attribute(attr6) => attr6,
            // Ex) `foo.bar.baz.bop.bap.bab`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[
                    id.as_str(),
                    attr5.attr.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ]));
            }
            _ => return None,
        };

        let attr7 = match attr6.value.as_ref() {
            Expr::Attribute(attr7) => attr7,
            // Ex) `foo.bar.baz.bop.bap.bab.bob`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self::from_slice(&[
                    id.as_str(),
                    attr6.attr.as_str(),
                    attr5.attr.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ]));
            }
            _ => return None,
        };

        let attr8 = match attr7.value.as_ref() {
            Expr::Attribute(attr8) => attr8,
            // Ex) `foo.bar.baz.bop.bap.bab.bob.bib`
            Expr::Name(ExprName { id, .. }) => {
                return Some(Self(SegmentsVec::from([
                    id.as_str(),
                    attr7.attr.as_str(),
                    attr6.attr.as_str(),
                    attr5.attr.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ])));
            }
            _ => return None,
        };

        let mut segments = Vec::with_capacity(SMALL_LEN * 2);

        let mut current = &*attr8.value;

        loop {
            current = match current {
                Expr::Attribute(attr) => {
                    segments.push(attr.attr.as_str());
                    &*attr.value
                }
                Expr::Name(ExprName { id, .. }) => {
                    segments.push(id.as_str());
                    break;
                }
                _ => {
                    return None;
                }
            }
        }

        segments.reverse();

        // Append the attributes we visited before calling into the recursion.
        segments.extend_from_slice(&[
            attr8.attr.as_str(),
            attr7.attr.as_str(),
            attr6.attr.as_str(),
            attr5.attr.as_str(),
            attr4.attr.as_str(),
            attr3.attr.as_str(),
            attr2.attr.as_str(),
            attr1.attr.as_str(),
        ]);

        Some(Self(SegmentsVec::from(segments)))
    }

    #[inline]
    pub fn from_slice(segments: &[&'a str]) -> Self {
        Self(SegmentsVec::from_slice(segments))
    }

    pub fn segments(&self) -> &[&'a str] {
        self.0.as_slice()
    }
}

impl Display for UnqualifiedName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for segment in self.segments() {
            if !first {
                f.write_char('.')?;
            }

            f.write_str(segment)?;
            first = false;
        }

        Ok(())
    }
}

impl<'a> FromIterator<&'a str> for UnqualifiedName<'a> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// A smallvec like storage for qualified and unqualified name segments.
///
/// Stores up to 8 segments inline, and falls back to a heap-allocated vector for names with more segments.
///
/// ## Note
/// The inline variant uses `ArrayVec` rather than `SmallVec` v1 because `SmallVec`'s type
/// definition has a variance problem. The incorrect variance leads lifetime inference in the
/// `SemanticModel` astray, causing all sorts of "strange" lifetime errors.
#[derive(Clone)]
enum SegmentsVec<'a> {
    Stack(SegmentsStack<'a>),
    Heap(Vec<&'a str>),
}

impl<'a> SegmentsVec<'a> {
    /// Creates an empty segment vec.
    fn new() -> Self {
        Self::Stack(SegmentsStack::default())
    }

    /// Creates a segment vec that has reserved storage for up to `capacity` items.
    fn with_capacity(capacity: usize) -> Self {
        if capacity <= SMALL_LEN {
            Self::new()
        } else {
            Self::Heap(Vec::with_capacity(capacity))
        }
    }

    #[cfg(test)]
    const fn is_spilled(&self) -> bool {
        matches!(self, SegmentsVec::Heap(_))
    }

    /// Initializes the segments from a slice.
    #[inline]
    fn from_slice(slice: &[&'a str]) -> Self {
        match SegmentsStack::try_from(slice) {
            Ok(stack) => SegmentsVec::Stack(stack),
            Err(_) => SegmentsVec::Heap(slice.to_vec()),
        }
    }

    /// Returns the segments as a slice.
    #[inline]
    fn as_slice(&self) -> &[&'a str] {
        match self {
            Self::Stack(stack) => stack.as_slice(),
            Self::Heap(heap) => heap.as_slice(),
        }
    }

    /// Pushes `name` to the end of the segments.
    ///
    /// Spills to the heap if the segments are stored on the stack and the 9th segment is pushed.
    #[inline]
    fn push(&mut self, name: &'a str) {
        match self {
            SegmentsVec::Stack(stack) => {
                if let Err(error) = stack.try_push(name) {
                    let mut segments = Vec::with_capacity(stack.len() * 2);
                    segments.extend(stack.iter().copied());
                    segments.push(error.element());
                    *self = SegmentsVec::Heap(segments);
                }
            }
            SegmentsVec::Heap(heap) => {
                heap.push(name);
            }
        }
    }

    /// Pops the last segment from the end and returns it.
    ///
    /// Returns `None` if the vector is empty.
    #[inline]
    fn pop(&mut self) -> Option<&'a str> {
        match self {
            SegmentsVec::Stack(stack) => stack.pop(),
            SegmentsVec::Heap(heap) => heap.pop(),
        }
    }

    #[inline]
    fn extend_from_slice(&mut self, slice: &[&'a str]) {
        match self {
            SegmentsVec::Stack(stack) => {
                if stack.try_extend_from_slice(slice).is_err() {
                    let mut segments = Vec::with_capacity(stack.len() + slice.len());
                    segments.extend(stack.iter().copied());
                    segments.extend_from_slice(slice);
                    *self = SegmentsVec::Heap(segments);
                }
            }
            SegmentsVec::Heap(heap) => heap.extend_from_slice(slice),
        }
    }
}

impl Default for SegmentsVec<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for SegmentsVec<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a> Deref for SegmentsVec<'a> {
    type Target = [&'a str];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'b> PartialEq<SegmentsVec<'b>> for SegmentsVec<'_> {
    fn eq(&self, other: &SegmentsVec<'b>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for SegmentsVec<'_> {}

impl Hash for SegmentsVec<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<'a> FromIterator<&'a str> for SegmentsVec<'a> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut segments = SegmentsVec::default();
        segments.extend(iter);
        segments
    }
}

impl<'a> From<[&'a str; 8]> for SegmentsVec<'a> {
    #[inline]
    fn from(segments: [&'a str; 8]) -> Self {
        SegmentsVec::Stack(SegmentsStack::from(segments))
    }
}

impl<'a> From<Vec<&'a str>> for SegmentsVec<'a> {
    #[inline]
    fn from(segments: Vec<&'a str>) -> Self {
        SegmentsVec::Heap(segments)
    }
}

impl<'a> Extend<&'a str> for SegmentsVec<'a> {
    #[inline]
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, iter: T) {
        match self {
            SegmentsVec::Stack(stack) => {
                let mut iter = iter.into_iter();
                let (lower, _) = iter.size_hint();

                if lower > stack.remaining_capacity() {
                    let mut segments = Vec::with_capacity(stack.len() + lower);
                    segments.extend(stack.iter().copied());
                    segments.extend(iter);
                    *self = SegmentsVec::Heap(segments);
                    return;
                }

                while let Some(name) = iter.next() {
                    if let Err(error) = stack.try_push(name) {
                        let mut segments = Vec::with_capacity(stack.len() * 2);
                        segments.extend(stack.iter().copied());
                        segments.push(error.element());
                        segments.extend(iter);
                        *self = SegmentsVec::Heap(segments);
                        return;
                    }
                }
            }
            SegmentsVec::Heap(heap) => {
                heap.extend(iter);
            }
        }
    }
}

const SMALL_LEN: usize = 8;
type SegmentsStack<'a> = ArrayVec<&'a str, SMALL_LEN>;

#[cfg(test)]
mod tests {
    #[cfg(feature = "salsa")]
    use std::hash::{DefaultHasher, Hash, Hasher};

    #[cfg(feature = "salsa")]
    use crate::name::Name;
    use crate::name::SegmentsVec;

    #[cfg(feature = "salsa")]
    #[test]
    fn salsa_lookup_name_from_str() {
        let name = Name::new("member");
        let lookup = "member";

        let mut name_hasher = DefaultHasher::new();
        salsa::HashEqLike::<&str>::hash(&name, &mut name_hasher);
        let mut lookup_hasher = DefaultHasher::new();
        lookup.hash(&mut lookup_hasher);

        assert_eq!(name_hasher.finish(), lookup_hasher.finish());
        assert!(salsa::HashEqLike::<&str>::eq(&name, &lookup));
        assert_eq!(salsa::Lookup::<Name>::into_owned(lookup), name);
    }

    #[test]
    fn empty_vec() {
        let empty = SegmentsVec::new();
        assert_eq!(empty.as_slice(), &[] as &[&str]);
        assert!(!empty.is_spilled());
    }

    #[test]
    fn from_slice_stack() {
        let stack = SegmentsVec::from_slice(&["a", "b", "c"]);

        assert_eq!(stack.as_slice(), &["a", "b", "c"]);
        assert!(!stack.is_spilled());
    }

    #[test]
    fn from_slice_stack_capacity() {
        let stack = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g", "h"]);

        assert_eq!(stack.as_slice(), &["a", "b", "c", "d", "e", "f", "g", "h"]);
        assert!(!stack.is_spilled());
    }

    #[test]
    fn from_slice_heap() {
        let heap = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]);

        assert_eq!(
            heap.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        );
        assert!(heap.is_spilled());
    }

    #[test]
    fn push_stack() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c"]);
        stack.push("d");
        stack.push("e");

        assert_eq!(stack.as_slice(), &["a", "b", "c", "d", "e"]);
        assert!(!stack.is_spilled());
    }

    #[test]
    fn push_stack_spill() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g"]);
        stack.push("h");

        assert!(!stack.is_spilled());

        stack.push("i");

        assert_eq!(
            stack.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        );
        assert!(stack.is_spilled());
    }

    #[test]
    fn pop_stack() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c", "d", "e"]);
        assert_eq!(stack.pop(), Some("e"));
        assert_eq!(stack.pop(), Some("d"));
        assert_eq!(stack.pop(), Some("c"));
        assert_eq!(stack.pop(), Some("b"));
        assert_eq!(stack.pop(), Some("a"));
        assert_eq!(stack.pop(), None);

        assert!(!stack.is_spilled());
    }

    #[test]
    fn pop_heap() {
        let mut heap = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]);

        assert_eq!(heap.pop(), Some("i"));
        assert_eq!(heap.pop(), Some("h"));
        assert_eq!(heap.pop(), Some("g"));

        assert!(heap.is_spilled());
    }

    #[test]
    fn extend_from_slice_stack() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c"]);
        stack.extend_from_slice(&["d", "e", "f"]);

        assert_eq!(stack.as_slice(), &["a", "b", "c", "d", "e", "f"]);
        assert!(!stack.is_spilled());
    }

    #[test]
    fn extend_from_slice_stack_spill() {
        let mut spilled = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f"]);
        spilled.extend_from_slice(&["g", "h", "i", "j"]);

        assert_eq!(
            spilled.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
        );
        assert!(spilled.is_spilled());
    }

    #[test]
    fn extend_from_slice_heap() {
        let mut heap = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
        assert!(heap.is_spilled());

        heap.extend_from_slice(&["j", "k", "l"]);

        assert_eq!(
            heap.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"]
        );
    }

    #[test]
    fn extend_stack() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c"]);
        stack.extend(["d", "e", "f"]);

        assert_eq!(stack.as_slice(), &["a", "b", "c", "d", "e", "f"]);
        assert!(!stack.is_spilled());
    }

    #[test]
    fn extend_stack_spilled() {
        let mut stack = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f"]);
        stack.extend(["g", "h", "i", "j"]);

        assert_eq!(
            stack.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
        );
        assert!(stack.is_spilled());
    }

    #[test]
    fn extend_heap() {
        let mut heap = SegmentsVec::from_slice(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
        assert!(heap.is_spilled());

        heap.extend(["j", "k", "l"]);

        assert_eq!(
            heap.as_slice(),
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"]
        );
    }
}
