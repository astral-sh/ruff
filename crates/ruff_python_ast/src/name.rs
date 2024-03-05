use std::fmt::{Debug, Display, Formatter, Write};
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use crate::{nodes, Expr};

/// A representation of a qualified name, like `typing.List`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedName<'a>(SegmentsInner<'a>);

impl<'a> QualifiedName<'a> {
    /// Create a [`QualifiedName`] from a dotted name.
    ///
    /// ```rust
    /// # use smallvec::smallvec;
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
        Self(SegmentsInner::Small(SegmentsSmall::from_slice(&["", name])))
    }

    #[inline]
    pub fn segments(&self) -> &[&'a str] {
        self.0.as_slice()
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self.segments(), ["", ..])
    }

    pub fn is_user_defined(&self) -> bool {
        !self.is_builtin()
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
}

impl Display for QualifiedName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let segments = self.segments();
        if segments.first().is_some_and(|first| first.is_empty()) {
            // If the first segment is empty, the `CallPath` is that of a builtin.
            // Ex) `["", "bool"]` -> `"bool"`
            let mut first = true;

            for segment in segments.iter().skip(1) {
                if !first {
                    f.write_char('.')?;
                }

                f.write_str(segment)?;
                first = false;
            }
        } else if segments.first().is_some_and(|first| matches!(*first, ".")) {
            // If the call path is dot-prefixed, it's an unresolved relative import.
            // Ex) `[".foo", "bar"]` -> `".foo.bar"`

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
        Self(SegmentsInner::from_iter(iter))
    }
}

#[derive(Debug, Clone, Default)]
pub struct QualifiedNameBuilder<'a> {
    segments: SegmentsInner<'a>,
}

impl<'a> QualifiedNameBuilder<'a> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: SegmentsInner::with_capacity(capacity),
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
pub struct UnqualifiedName<'a>(SegmentsInner<'a>);

impl<'a> UnqualifiedName<'a> {
    /// Convert an `Expr` to its [`UnqualifiedName`] (like `["typing", "List"]`).
    pub fn from_expr(expr: &'a Expr) -> Option<Self> {
        // Unroll the loop up to eight times, to match the maximum number of expected attributes.
        // In practice, unrolling appears to give about a 4x speed-up on this hot path.
        let attr1 = match expr {
            Expr::Attribute(attr1) => attr1,
            // Ex) `foo`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[id.as_str()])))
            }
            _ => return None,
        };

        let attr2 = match attr1.value.as_ref() {
            Expr::Attribute(attr2) => attr2,
            // Ex) `foo.bar`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
                    attr1.attr.as_str(),
                ])))
            }
            _ => return None,
        };

        let attr3 = match attr2.value.as_ref() {
            Expr::Attribute(attr3) => attr3,
            // Ex) `foo.bar.baz`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ])));
            }
            _ => return None,
        };

        let attr4 = match attr3.value.as_ref() {
            Expr::Attribute(attr4) => attr4,
            // Ex) `foo.bar.baz.bop`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ])));
            }
            _ => return None,
        };

        let attr5 = match attr4.value.as_ref() {
            Expr::Attribute(attr5) => attr5,
            // Ex) `foo.bar.baz.bop.bap`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ])));
            }
            _ => return None,
        };

        let attr6 = match attr5.value.as_ref() {
            Expr::Attribute(attr6) => attr6,
            // Ex) `foo.bar.baz.bop.bap.bab`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
                    attr5.attr.as_str(),
                    attr4.attr.as_str(),
                    attr3.attr.as_str(),
                    attr2.attr.as_str(),
                    attr1.attr.as_str(),
                ])));
            }
            _ => return None,
        };

        let attr7 = match attr6.value.as_ref() {
            Expr::Attribute(attr7) => attr7,
            // Ex) `foo.bar.baz.bop.bap.bab.bob`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from_slice(&[
                    id.as_str(),
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

        let attr8 = match attr7.value.as_ref() {
            Expr::Attribute(attr8) => attr8,
            // Ex) `foo.bar.baz.bop.bap.bab.bob.bib`
            Expr::Name(nodes::ExprName { id, .. }) => {
                return Some(Self(SegmentsInner::from([
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
                Expr::Name(nodes::ExprName { id, .. }) => {
                    segments.push(id.as_str());
                    break;
                }
                _ => break,
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

        Some(Self(SegmentsInner::from(segments)))
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

#[derive(Clone)]
enum SegmentsInner<'a> {
    Small(SegmentsSmall<'a>),
    Vec(SegmentsVec<'a>),
}

impl<'a> SegmentsInner<'a> {
    fn new() -> Self {
        Self::Small(SegmentsSmall::default())
    }

    fn with_capacity(capacity: usize) -> Self {
        if capacity <= SMALL_LEN {
            Self::new()
        } else {
            Self::Vec(SegmentsVec {
                segments: Vec::with_capacity(capacity),
            })
        }
    }

    fn as_slice(&self) -> &[&'a str] {
        match self {
            Self::Small(name) => name.as_slice(),
            Self::Vec(name) => name.as_slice(),
        }
    }

    fn push(&mut self, name: &'a str) {
        match self {
            SegmentsInner::Small(small) => {
                if small.len < small.segments.len() {
                    small.segments[small.len] = name;
                    small.len += 1;
                } else {
                    *self = SegmentsInner::Vec(SegmentsVec {
                        segments: Vec::from(small.segments),
                    });
                }
            }
            SegmentsInner::Vec(dynamic) => {
                dynamic.segments.push(name);
            }
        }
    }

    fn pop(&mut self) -> Option<&'a str> {
        match self {
            SegmentsInner::Small(small) => {
                if small.len == 0 {
                    None
                } else {
                    small.len -= 1;
                    Some(small.segments[small.len])
                }
            }
            SegmentsInner::Vec(heap) => heap.segments.pop(),
        }
    }

    #[inline]
    fn from_slice(slice: &[&'a str]) -> Self {
        if slice.len() <= SMALL_LEN {
            SegmentsInner::Small(SegmentsSmall::from_slice(slice))
        } else {
            SegmentsInner::Vec(SegmentsVec {
                segments: slice.to_vec(),
            })
        }
    }

    #[inline]
    fn extend_from_slice(&mut self, slice: &[&'a str]) {
        match self {
            SegmentsInner::Small(small) => {
                let capacity = small.segments.len() - small.len;

                if slice.len() <= capacity {
                    let new_len = small.len + slice.len();
                    small.segments[small.len..new_len].copy_from_slice(slice);
                    small.len = new_len;
                } else {
                    let mut segments = small.as_slice().to_vec();
                    segments.extend_from_slice(slice);
                    *self = SegmentsInner::Vec(SegmentsVec { segments });
                }
            }
            SegmentsInner::Vec(heap) => heap.segments.extend_from_slice(slice),
        }
    }
}

impl Default for SegmentsInner<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for SegmentsInner<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a> Deref for SegmentsInner<'a> {
    type Target = [&'a str];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a, 'b> PartialEq<SegmentsInner<'b>> for SegmentsInner<'a> {
    fn eq(&self, other: &SegmentsInner<'b>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for SegmentsInner<'_> {}

impl Hash for SegmentsInner<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<'a> FromIterator<&'a str> for SegmentsInner<'a> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut segments = SegmentsInner::default();
        segments.extend(iter);
        segments
    }
}

impl<'a> From<[&'a str; 8]> for SegmentsInner<'a> {
    #[inline]
    fn from(segments: [&'a str; 8]) -> Self {
        SegmentsInner::Small(SegmentsSmall {
            segments,
            len: segments.len(),
        })
    }
}

impl<'a> From<Vec<&'a str>> for SegmentsInner<'a> {
    #[inline]
    fn from(segments: Vec<&'a str>) -> Self {
        SegmentsInner::Vec(SegmentsVec { segments })
    }
}

impl<'a> Extend<&'a str> for SegmentsInner<'a> {
    #[inline]
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, iter: T) {
        match self {
            SegmentsInner::Small(small) => {
                let iter = iter.into_iter();
                let (lower, upper) = iter.size_hint();

                let capacity = SMALL_LEN - small.len;

                if upper.unwrap_or(lower) <= capacity {
                    for name in iter {
                        self.push(name);
                    }
                } else {
                    let mut segments = small.as_slice().to_vec();
                    segments.extend(iter);
                    *self = SegmentsInner::Vec(SegmentsVec { segments });
                }
            }
            SegmentsInner::Vec(heap) => {
                heap.segments.extend(iter);
            }
        }
    }
}

const SMALL_LEN: usize = 8;

#[derive(Debug, Clone, Default)]
struct SegmentsSmall<'a> {
    segments: [&'a str; SMALL_LEN],
    len: usize,
}

impl<'a> SegmentsSmall<'a> {
    fn from_slice(slice: &[&'a str]) -> Self {
        assert!(slice.len() <= SMALL_LEN);

        let mut segments: [&'a str; SMALL_LEN] = Default::default();
        segments[..slice.len()].copy_from_slice(slice);
        SegmentsSmall {
            segments,
            len: slice.len(),
        }
    }

    fn as_slice(&self) -> &[&'a str] {
        &self.segments[..self.len]
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct SegmentsVec<'a> {
    segments: Vec<&'a str>,
}

impl<'a> SegmentsVec<'a> {
    fn as_slice(&self) -> &[&'a str] {
        &self.segments
    }
}
