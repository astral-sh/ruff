use smallvec::SmallVec;
use std::fmt::{Display, Formatter, Write};

use crate::{nodes, Expr};

/// A representation of a qualified name, like `typing.List`.
#[derive(Debug, Clone, Eq, Hash)]
pub struct QualifiedName<'a> {
    segments: SmallVec<[&'a str; 8]>,
}

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
            let mut segments = SmallVec::new();
            segments.push(&name[..dot]);
            segments.extend(name[dot + 1..].split('.'));
            Self { segments }
        } else {
            Self::builtin(name)
        }
    }

    /// Creates a name that's guaranteed not be a built in
    #[inline]
    pub fn imported(name: &'a str) -> Self {
        name.split('.').collect()
    }

    /// Creates a qualified name for a built in
    #[inline]
    pub fn builtin(name: &'a str) -> Self {
        debug_assert!(!name.contains('.'));
        Self {
            segments: ["", name].into_iter().collect(),
        }
    }

    #[inline]
    pub fn from_slice(segments: &[&'a str]) -> Self {
        Self {
            segments: segments.into(),
        }
    }

    pub fn starts_with(&self, other: &QualifiedName) -> bool {
        self.segments().starts_with(other.segments())
    }

    #[inline]
    pub fn segments(&self) -> &[&'a str] {
        &self.segments
    }

    #[inline]
    pub fn into_boxed_slice(self) -> Box<[&'a str]> {
        self.segments.into_boxed_slice()
    }
}

impl<'a> FromIterator<&'a str> for QualifiedName<'a> {
    fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> Self {
        Self {
            segments: iter.into_iter().collect(),
        }
    }
}

impl<'a, 'b> PartialEq<QualifiedName<'b>> for QualifiedName<'a> {
    #[inline]
    fn eq(&self, other: &QualifiedName<'b>) -> bool {
        self.segments == other.segments
    }
}

#[derive(Debug, Clone)]
pub struct QualifiedNameBuilder<'a> {
    segments: SmallVec<[&'a str; 8]>,
}

impl<'a> QualifiedNameBuilder<'a> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: SmallVec::with_capacity(capacity),
        }
    }

    pub fn from_qualified_name(qualified_name: QualifiedName<'a>) -> Self {
        Self {
            segments: qualified_name.segments,
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

    pub fn pop(&mut self) {
        self.segments.pop();
    }

    #[inline]
    pub fn extend(&mut self, segments: impl IntoIterator<Item = &'a str>) {
        self.segments.extend(segments);
    }

    pub fn extend_from_slice(&mut self, segments: &[&'a str]) {
        self.segments.extend_from_slice(segments);
    }

    pub fn build(self) -> QualifiedName<'a> {
        QualifiedName {
            segments: self.segments,
        }
    }
}

impl Display for QualifiedName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        format_qualified_name_segments(self.segments(), f)
    }
}

pub fn format_qualified_name_segments(segments: &[&str], w: &mut dyn Write) -> std::fmt::Result {
    if segments.first().is_some_and(|first| first.is_empty()) {
        // If the first segment is empty, the `CallPath` is that of a builtin.
        // Ex) `["", "bool"]` -> `"bool"`
        let mut first = true;

        for segment in segments.iter().skip(1) {
            if !first {
                w.write_char('.')?;
            }

            w.write_str(segment)?;
            first = false;
        }
    } else if segments.first().is_some_and(|first| matches!(*first, ".")) {
        // If the call path is dot-prefixed, it's an unresolved relative import.
        // Ex) `[".foo", "bar"]` -> `".foo.bar"`

        let mut iter = segments.iter();
        for segment in iter.by_ref() {
            if *segment == "." {
                w.write_char('.')?;
            } else {
                w.write_str(segment)?;
                break;
            }
        }
        for segment in iter {
            w.write_char('.')?;
            w.write_str(segment)?;
        }
    } else {
        let mut first = true;
        for segment in segments {
            if !first {
                w.write_char('.')?;
            }

            w.write_str(segment)?;
            first = false;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Eq, Hash)]
pub struct UnqualifiedName<'a> {
    segments: SmallVec<[&'a str; 8]>,
}

impl<'a> UnqualifiedName<'a> {
    pub fn from_expr(expr: &'a Expr) -> Option<Self> {
        let segments = collect_segments(expr)?;
        Some(Self { segments })
    }

    pub fn segments(&self) -> &[&'a str] {
        &self.segments
    }
}

impl<'a, 'b> PartialEq<UnqualifiedName<'b>> for UnqualifiedName<'a> {
    #[inline]
    fn eq(&self, other: &UnqualifiedName<'b>) -> bool {
        self.segments == other.segments
    }
}

impl Display for UnqualifiedName<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for segment in &self.segments {
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
        Self {
            segments: iter.into_iter().collect(),
        }
    }
}

/// Convert an `Expr` to its [`QualifiedName`] segments (like `["typing", "List"]`).
fn collect_segments(expr: &Expr) -> Option<SmallVec<[&str; 8]>> {
    // Unroll the loop up to eight times, to match the maximum number of expected attributes.
    // In practice, unrolling appears to give about a 4x speed-up on this hot path.
    let attr1 = match expr {
        Expr::Attribute(attr1) => attr1,
        // Ex) `foo`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[id.as_str()]))
        }
        _ => return None,
    };

    let attr2 = match attr1.value.as_ref() {
        Expr::Attribute(attr2) => attr2,
        // Ex) `foo.bar`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[id.as_str(), attr1.attr.as_str()]))
        }
        _ => return None,
    };

    let attr3 = match attr2.value.as_ref() {
        Expr::Attribute(attr3) => attr3,
        // Ex) `foo.bar.baz`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[
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
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[
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
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[
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
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[
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
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from_slice(&[
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
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(SmallVec::from([
                id.as_str(),
                attr7.attr.as_str(),
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

    collect_segments(&attr8.value).map(|mut segments| {
        segments.extend([
            attr8.attr.as_str(),
            attr7.attr.as_str(),
            attr6.attr.as_str(),
            attr5.attr.as_str(),
            attr4.attr.as_str(),
            attr3.attr.as_str(),
            attr2.attr.as_str(),
            attr1.attr.as_str(),
        ]);
        segments
    })
}
