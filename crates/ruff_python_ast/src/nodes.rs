#![allow(clippy::derive_partial_eq_without_eq)]

use crate::generated::{
    ExprBytesLiteral, ExprDict, ExprFString, ExprList, ExprName, ExprSet, ExprStringLiteral,
    ExprTuple, StmtClassDef,
};
use std::borrow::Cow;
use std::fmt;
use std::fmt::Debug;
use std::iter::FusedIterator;
use std::ops::{Deref, DerefMut};
use std::slice::{Iter, IterMut};
use std::sync::OnceLock;

use bitflags::bitflags;
use itertools::Itertools;

use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::str_prefix::{AnyStringPrefix, ByteStringPrefix, FStringPrefix, StringLiteralPrefix};
use crate::{
    int,
    name::Name,
    str::{Quote, TripleQuotes},
    Expr, ExprRef, FStringElement, LiteralExpressionRef, OperatorPrecedence, Pattern, Stmt,
    TypeParam,
};

/// See also [Module](https://docs.python.org/3/library/ast.html#ast.Module)
#[derive(Clone, Debug, PartialEq)]
pub struct ModModule {
    pub range: TextRange,
    pub body: Vec<Stmt>,
}

/// See also [Expression](https://docs.python.org/3/library/ast.html#ast.Expression)
#[derive(Clone, Debug, PartialEq)]
pub struct ModExpression {
    pub range: TextRange,
    pub body: Box<Expr>,
}

impl StmtClassDef {
    /// Return an iterator over the bases of the class.
    pub fn bases(&self) -> &[Expr] {
        match &self.arguments {
            Some(arguments) => &arguments.args,
            None => &[],
        }
    }

    /// Return an iterator over the metaclass keywords of the class.
    pub fn keywords(&self) -> &[Keyword] {
        match &self.arguments {
            Some(arguments) => &arguments.keywords,
            None => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ElifElseClause {
    pub range: TextRange,
    pub test: Option<Expr>,
    pub body: Vec<Stmt>,
}

impl Expr {
    /// Returns `true` if the expression is a literal expression.
    ///
    /// A literal expression is either a string literal, bytes literal,
    /// integer, float, complex number, boolean, `None`, or ellipsis (`...`).
    pub fn is_literal_expr(&self) -> bool {
        matches!(
            self,
            Expr::StringLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
        )
    }

    /// Returns [`LiteralExpressionRef`] if the expression is a literal expression.
    pub fn as_literal_expr(&self) -> Option<LiteralExpressionRef<'_>> {
        match self {
            Expr::StringLiteral(expr) => Some(LiteralExpressionRef::StringLiteral(expr)),
            Expr::BytesLiteral(expr) => Some(LiteralExpressionRef::BytesLiteral(expr)),
            Expr::NumberLiteral(expr) => Some(LiteralExpressionRef::NumberLiteral(expr)),
            Expr::BooleanLiteral(expr) => Some(LiteralExpressionRef::BooleanLiteral(expr)),
            Expr::NoneLiteral(expr) => Some(LiteralExpressionRef::NoneLiteral(expr)),
            Expr::EllipsisLiteral(expr) => Some(LiteralExpressionRef::EllipsisLiteral(expr)),
            _ => None,
        }
    }

    /// Return the [`OperatorPrecedence`] of this expression
    pub fn precedence(&self) -> OperatorPrecedence {
        OperatorPrecedence::from(self)
    }
}

impl ExprRef<'_> {
    pub fn precedence(&self) -> OperatorPrecedence {
        OperatorPrecedence::from(self)
    }
}

/// Represents an item in a [dictionary literal display][1].
///
/// Consider the following Python dictionary literal:
/// ```python
/// {key1: value1, **other_dictionary}
/// ```
///
/// In our AST, this would be represented using an `ExprDict` node containing
/// two `DictItem` nodes inside it:
/// ```ignore
/// [
///     DictItem {
///         key: Some(Expr::Name(ExprName { id: "key1" })),
///         value: Expr::Name(ExprName { id: "value1" }),
///     },
///     DictItem {
///         key: None,
///         value: Expr::Name(ExprName { id: "other_dictionary" }),
///     }
/// ]
/// ```
///
/// [1]: https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries
#[derive(Debug, Clone, PartialEq)]
pub struct DictItem {
    pub key: Option<Expr>,
    pub value: Expr,
}

impl DictItem {
    fn key(&self) -> Option<&Expr> {
        self.key.as_ref()
    }

    fn value(&self) -> &Expr {
        &self.value
    }
}

impl Ranged for DictItem {
    fn range(&self) -> TextRange {
        TextRange::new(
            self.key.as_ref().map_or(self.value.start(), Ranged::start),
            self.value.end(),
        )
    }
}

impl ExprDict {
    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's keys.
    pub fn iter_keys(&self) -> DictKeyIterator {
        DictKeyIterator::new(&self.items)
    }

    /// Returns an `Iterator` over the AST nodes representing the
    /// dictionary's values.
    pub fn iter_values(&self) -> DictValueIterator {
        DictValueIterator::new(&self.items)
    }

    /// Returns the AST node representing the *n*th key of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn key(&self, n: usize) -> Option<&Expr> {
        self.items[n].key()
    }

    /// Returns the AST node representing the *n*th value of this
    /// dictionary.
    ///
    /// Panics: If the index `n` is out of bounds.
    pub fn value(&self, n: usize) -> &Expr {
        self.items[n].value()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DictItem> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprDict {
    type IntoIter = std::slice::Iter<'a, DictItem>;
    type Item = &'a DictItem;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct DictKeyIterator<'a> {
    items: Iter<'a, DictItem>,
}

impl<'a> DictKeyIterator<'a> {
    fn new(items: &'a [DictItem]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> Iterator for DictKeyIterator<'a> {
    type Item = Option<&'a Expr>;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next().map(DictItem::key)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl DoubleEndedIterator for DictKeyIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::key)
    }
}

impl FusedIterator for DictKeyIterator<'_> {}
impl ExactSizeIterator for DictKeyIterator<'_> {}

#[derive(Debug, Clone)]
pub struct DictValueIterator<'a> {
    items: Iter<'a, DictItem>,
}

impl<'a> DictValueIterator<'a> {
    fn new(items: &'a [DictItem]) -> Self {
        Self {
            items: items.iter(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'a> Iterator for DictValueIterator<'a> {
    type Item = &'a Expr;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next().map(DictItem::value)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}

impl DoubleEndedIterator for DictValueIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.items.next_back().map(DictItem::value)
    }
}

impl FusedIterator for DictValueIterator<'_> {}
impl ExactSizeIterator for DictValueIterator<'_> {}

impl ExprSet {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprSet {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FStringFormatSpec {
    pub range: TextRange,
    pub elements: FStringElements,
}

/// See also [FormattedValue](https://docs.python.org/3/library/ast.html#ast.FormattedValue)
#[derive(Clone, Debug, PartialEq)]
pub struct FStringExpressionElement {
    pub range: TextRange,
    pub expression: Box<Expr>,
    pub debug_text: Option<DebugText>,
    pub conversion: ConversionFlag,
    pub format_spec: Option<Box<FStringFormatSpec>>,
}

/// An `FStringLiteralElement` with an empty `value` is an invalid f-string element.
#[derive(Clone, Debug, PartialEq)]
pub struct FStringLiteralElement {
    pub range: TextRange,
    pub value: Box<str>,
}

impl FStringLiteralElement {
    pub fn is_valid(&self) -> bool {
        !self.value.is_empty()
    }
}

impl Deref for FStringLiteralElement {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Transforms a value prior to formatting it.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, is_macro::Is)]
#[repr(i8)]
#[allow(clippy::cast_possible_wrap)]
pub enum ConversionFlag {
    /// No conversion
    None = -1, // CPython uses -1
    /// Converts by calling `str(<value>)`.
    Str = b's' as i8,
    /// Converts by calling `ascii(<value>)`.
    Ascii = b'a' as i8,
    /// Converts by calling `repr(<value>)`.
    Repr = b'r' as i8,
}

impl ConversionFlag {
    pub fn to_byte(&self) -> Option<u8> {
        match self {
            Self::None => None,
            flag => Some(*flag as u8),
        }
    }
    pub fn to_char(&self) -> Option<char> {
        Some(self.to_byte()? as char)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugText {
    /// The text between the `{` and the expression node.
    pub leading: String,
    /// The text between the expression and the conversion, the `format_spec`, or the `}`, depending on what's present in the source
    pub trailing: String,
}

impl ExprFString {
    /// Returns the single [`FString`] if the f-string isn't implicitly concatenated, [`None`]
    /// otherwise.
    pub const fn as_single_part_fstring(&self) -> Option<&FString> {
        match &self.value.inner {
            FStringValueInner::Single(FStringPart::FString(fstring)) => Some(fstring),
            _ => None,
        }
    }
}

/// The value representing an [`ExprFString`].
#[derive(Clone, Debug, PartialEq)]
pub struct FStringValue {
    inner: FStringValueInner,
}

impl FStringValue {
    /// Creates a new f-string literal with a single [`FString`] part.
    pub fn single(value: FString) -> Self {
        Self {
            inner: FStringValueInner::Single(FStringPart::FString(value)),
        }
    }

    /// Creates a new f-string with the given values that represents an implicitly
    /// concatenated f-string.
    ///
    /// # Panics
    ///
    /// Panics if `values` has less than 2 elements.
    /// Use [`FStringValue::single`] instead.
    pub fn concatenated(values: Vec<FStringPart>) -> Self {
        assert!(
            values.len() > 1,
            "Use `FStringValue::single` to create single-part f-strings"
        );
        Self {
            inner: FStringValueInner::Concatenated(values),
        }
    }

    /// Returns `true` if the f-string is implicitly concatenated, `false` otherwise.
    pub fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, FStringValueInner::Concatenated(_))
    }

    /// Returns a slice of all the [`FStringPart`]s contained in this value.
    pub fn as_slice(&self) -> &[FStringPart] {
        match &self.inner {
            FStringValueInner::Single(part) => std::slice::from_ref(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns a mutable slice of all the [`FStringPart`]s contained in this value.
    fn as_mut_slice(&mut self) -> &mut [FStringPart] {
        match &mut self.inner {
            FStringValueInner::Single(part) => std::slice::from_mut(part),
            FStringValueInner::Concatenated(parts) => parts,
        }
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value.
    pub fn iter(&self) -> Iter<FStringPart> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`FStringPart`]s contained in this value
    /// that allows modification.
    pub fn iter_mut(&mut self) -> IterMut<FStringPart> {
        self.as_mut_slice().iter_mut()
    }

    /// Returns an iterator over the [`StringLiteral`] parts contained in this value.
    ///
    /// Note that this doesn't recurse into the f-string parts. For example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// Here, the string literal parts returned would be `"foo"` and `"baz"`.
    pub fn literals(&self) -> impl Iterator<Item = &StringLiteral> {
        self.iter().filter_map(|part| part.as_literal())
    }

    /// Returns an iterator over the [`FString`] parts contained in this value.
    ///
    /// Note that this doesn't recurse into the f-string parts. For example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// Here, the f-string parts returned would be `f"bar {x}"` and `f"qux"`.
    pub fn f_strings(&self) -> impl Iterator<Item = &FString> {
        self.iter().filter_map(|part| part.as_f_string())
    }

    /// Returns an iterator over all the [`FStringElement`] contained in this value.
    ///
    /// An f-string element is what makes up an [`FString`] i.e., it is either a
    /// string literal or an expression. In the following example,
    ///
    /// ```python
    /// "foo" f"bar {x}" "baz" f"qux"
    /// ```
    ///
    /// The f-string elements returned would be string literal (`"bar "`),
    /// expression (`x`) and string literal (`"qux"`).
    pub fn elements(&self) -> impl Iterator<Item = &FStringElement> {
        self.f_strings().flat_map(|fstring| fstring.elements.iter())
    }
}

impl<'a> IntoIterator for &'a FStringValue {
    type Item = &'a FStringPart;
    type IntoIter = Iter<'a, FStringPart>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut FStringValue {
    type Item = &'a mut FStringPart;
    type IntoIter = IterMut<'a, FStringPart>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// An internal representation of [`FStringValue`].
#[derive(Clone, Debug, PartialEq)]
enum FStringValueInner {
    /// A single f-string i.e., `f"foo"`.
    ///
    /// This is always going to be `FStringPart::FString` variant which is
    /// maintained by the `FStringValue::single` constructor.
    Single(FStringPart),

    /// An implicitly concatenated f-string i.e., `"foo" f"bar {x}"`.
    Concatenated(Vec<FStringPart>),
}

/// An f-string part which is either a string literal or an f-string.
#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum FStringPart {
    Literal(StringLiteral),
    FString(FString),
}

impl FStringPart {
    pub fn quote_style(&self) -> Quote {
        match self {
            Self::Literal(string_literal) => string_literal.flags.quote_style(),
            Self::FString(f_string) => f_string.flags.quote_style(),
        }
    }
}

impl Ranged for FStringPart {
    fn range(&self) -> TextRange {
        match self {
            FStringPart::Literal(string_literal) => string_literal.range(),
            FStringPart::FString(f_string) => f_string.range(),
        }
    }
}

pub trait StringFlags: Copy {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote;

    fn triple_quotes(self) -> TripleQuotes;

    fn prefix(self) -> AnyStringPrefix;

    /// Is the string triple-quoted, i.e.,
    /// does it begin and end with three consecutive quote characters?
    fn is_triple_quoted(self) -> bool {
        self.triple_quotes().is_yes()
    }

    /// A `str` representation of the quotes used to start and close.
    /// This does not include any prefixes the string has in its opener.
    fn quote_str(self) -> &'static str {
        match (self.triple_quotes(), self.quote_style()) {
            (TripleQuotes::Yes, Quote::Single) => "'''",
            (TripleQuotes::Yes, Quote::Double) => r#"""""#,
            (TripleQuotes::No, Quote::Single) => "'",
            (TripleQuotes::No, Quote::Double) => "\"",
        }
    }

    /// The length of the quotes used to start and close the string.
    /// This does not include the length of any prefixes the string has
    /// in its opener.
    fn quote_len(self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::new(3)
        } else {
            TextSize::new(1)
        }
    }

    /// The total length of the string's opener,
    /// i.e., the length of the prefixes plus the length
    /// of the quotes used to open the string.
    fn opener_len(self) -> TextSize {
        self.prefix().text_len() + self.quote_len()
    }

    /// The total length of the string's closer.
    /// This is always equal to `self.quote_len()`,
    /// but is provided here for symmetry with the `opener_len()` method.
    fn closer_len(self) -> TextSize {
        self.quote_len()
    }

    fn as_any_string_flags(self) -> AnyStringFlags {
        AnyStringFlags::new(self.prefix(), self.quote_style(), self.triple_quotes())
    }

    fn display_contents(self, contents: &str) -> DisplayFlags {
        DisplayFlags {
            flags: self.as_any_string_flags(),
            contents,
        }
    }
}

pub struct DisplayFlags<'a> {
    flags: AnyStringFlags,
    contents: &'a str,
}

impl std::fmt::Display for DisplayFlags<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{prefix}{quote}{contents}{quote}",
            prefix = self.flags.prefix(),
            quote = self.flags.quote_str(),
            contents = self.contents
        )
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct FStringFlagsInner: u8 {
        /// The f-string uses double quotes (`"`) for its opener and closer.
        /// If this flag is not set, the f-string uses single quotes (`'`)
        /// for its opener and closer.
        const DOUBLE = 1 << 0;

        /// The f-string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        /// For example: `f"""{bar}"""`.
        const TRIPLE_QUOTED = 1 << 1;

        /// The f-string has an `r` prefix, meaning it is a raw f-string
        /// with a lowercase 'r'. For example: `rf"{bar}"`
        const R_PREFIX_LOWER = 1 << 2;

        /// The f-string has an `R` prefix, meaning it is a raw f-string
        /// with an uppercase 'r'. For example: `Rf"{bar}"`.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for an f-string.
///
/// ## Notes on usage
///
/// If you're using a `Generator` from the `ruff_python_codegen` crate to generate a lint-rule fix
/// from an existing f-string literal, consider passing along the [`FString::flags`] field. If you
/// don't have an existing literal but have a `Checker` from the `ruff_linter` crate available,
/// consider using `Checker::default_fstring_flags` to create instances of this struct; this method
/// will properly handle nested f-strings. For usage that doesn't fit into one of these categories,
/// the public constructor [`FStringFlags::empty`] can be used.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct FStringFlags(FStringFlagsInner);

impl FStringFlags {
    /// Construct a new [`FStringFlags`] with **no flags set**.
    ///
    /// See [`FStringFlags::with_quote_style`], [`FStringFlags::with_triple_quotes`], and
    /// [`FStringFlags::with_prefix`] for ways of setting the quote style (single or double),
    /// enabling triple quotes, and adding prefixes (such as `r`), respectively.
    ///
    /// See the documentation for [`FStringFlags`] for additional caveats on this constructor, and
    /// situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(FStringFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(FStringFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0
            .set(FStringFlagsInner::TRIPLE_QUOTED, triple_quotes.is_yes());
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: FStringPrefix) -> Self {
        match prefix {
            FStringPrefix::Regular => {
                Self(self.0 - FStringFlagsInner::R_PREFIX_LOWER - FStringFlagsInner::R_PREFIX_UPPER)
            }
            FStringPrefix::Raw { uppercase_r } => {
                self.0.set(FStringFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0.set(FStringFlagsInner::R_PREFIX_LOWER, !uppercase_r);
                self
            }
        }
    }

    pub const fn prefix(self) -> FStringPrefix {
        if self.0.contains(FStringFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(FStringFlagsInner::R_PREFIX_UPPER));
            FStringPrefix::Raw { uppercase_r: false }
        } else if self.0.contains(FStringFlagsInner::R_PREFIX_UPPER) {
            FStringPrefix::Raw { uppercase_r: true }
        } else {
            FStringPrefix::Regular
        }
    }
}

impl StringFlags for FStringFlags {
    /// Return `true` if the f-string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `f"""{bar}"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(FStringFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    /// Return the quoting style (single or double quotes)
    /// used by the f-string's opener and closer:
    /// - `f"{"a"}"` -> `QuoteStyle::Double`
    /// - `f'{"a"}'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(FStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Format(self.prefix())
    }
}

impl fmt::Debug for FStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FStringFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single f-string which is part of an [`ExprFString`].
#[derive(Clone, Debug, PartialEq)]
pub struct FString {
    pub range: TextRange,
    pub elements: FStringElements,
    pub flags: FStringFlags,
}

impl From<FString> for Expr {
    fn from(payload: FString) -> Self {
        ExprFString {
            range: payload.range,
            value: FStringValue::single(payload),
        }
        .into()
    }
}

/// A newtype wrapper around a list of [`FStringElement`].
#[derive(Clone, Default, PartialEq)]
pub struct FStringElements(Vec<FStringElement>);

impl FStringElements {
    /// Returns an iterator over all the [`FStringLiteralElement`] nodes contained in this f-string.
    pub fn literals(&self) -> impl Iterator<Item = &FStringLiteralElement> {
        self.iter().filter_map(|element| element.as_literal())
    }

    /// Returns an iterator over all the [`FStringExpressionElement`] nodes contained in this f-string.
    pub fn expressions(&self) -> impl Iterator<Item = &FStringExpressionElement> {
        self.iter().filter_map(|element| element.as_expression())
    }
}

impl From<Vec<FStringElement>> for FStringElements {
    fn from(elements: Vec<FStringElement>) -> Self {
        FStringElements(elements)
    }
}

impl<'a> IntoIterator for &'a FStringElements {
    type IntoIter = Iter<'a, FStringElement>;
    type Item = &'a FStringElement;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut FStringElements {
    type IntoIter = IterMut<'a, FStringElement>;
    type Item = &'a mut FStringElement;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl Deref for FStringElements {
    type Target = [FStringElement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FStringElements {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for FStringElements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl ExprStringLiteral {
    /// Return `Some(literal)` if the string only consists of a single `StringLiteral` part
    /// (indicating that it is not implicitly concatenated). Otherwise, return `None`.
    pub fn as_single_part_string(&self) -> Option<&StringLiteral> {
        match &self.value.inner {
            StringLiteralValueInner::Single(value) => Some(value),
            StringLiteralValueInner::Concatenated(_) => None,
        }
    }
}

/// The value representing a [`ExprStringLiteral`].
#[derive(Clone, Debug, PartialEq)]
pub struct StringLiteralValue {
    inner: StringLiteralValueInner,
}

impl StringLiteralValue {
    /// Creates a new string literal with a single [`StringLiteral`] part.
    pub fn single(string: StringLiteral) -> Self {
        Self {
            inner: StringLiteralValueInner::Single(string),
        }
    }

    /// Returns the [`StringLiteralFlags`] associated with this string literal.
    ///
    /// For an implicitly concatenated string, it returns the flags for the first literal.
    pub fn first_literal_flags(&self) -> StringLiteralFlags {
        self.iter()
            .next()
            .expect(
                "There should always be at least one string literal in an `ExprStringLiteral` node",
            )
            .flags
    }

    /// Creates a new string literal with the given values that represents an
    /// implicitly concatenated strings.
    ///
    /// # Panics
    ///
    /// Panics if `strings` has less than 2 elements.
    /// Use [`StringLiteralValue::single`] instead.
    pub fn concatenated(strings: Vec<StringLiteral>) -> Self {
        assert!(
            strings.len() > 1,
            "Use `StringLiteralValue::single` to create single-part strings"
        );
        Self {
            inner: StringLiteralValueInner::Concatenated(ConcatenatedStringLiteral {
                strings,
                value: OnceLock::new(),
            }),
        }
    }

    /// Returns `true` if the string literal is implicitly concatenated.
    pub const fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, StringLiteralValueInner::Concatenated(_))
    }

    /// Returns `true` if the string literal has a `u` prefix, e.g. `u"foo"`.
    ///
    /// Although all strings in Python 3 are valid unicode (and the `u` prefix
    /// is only retained for backwards compatibility), these strings are known as
    /// "unicode strings".
    ///
    /// For an implicitly concatenated string, it returns `true` only if the first
    /// [`StringLiteral`] has the `u` prefix.
    pub fn is_unicode(&self) -> bool {
        self.iter()
            .next()
            .is_some_and(|part| part.flags.prefix().is_unicode())
    }

    /// Returns a slice of all the [`StringLiteral`] parts contained in this value.
    pub fn as_slice(&self) -> &[StringLiteral] {
        match &self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_ref(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`StringLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [StringLiteral] {
        match &mut self.inner {
            StringLiteralValueInner::Single(value) => std::slice::from_mut(value),
            StringLiteralValueInner::Concatenated(value) => value.strings.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<StringLiteral> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`StringLiteral`] parts contained in this value
    /// that allows modification.
    pub fn iter_mut(&mut self) -> IterMut<StringLiteral> {
        self.as_mut_slice().iter_mut()
    }

    /// Returns `true` if the node represents an empty string.
    ///
    /// Note that a [`StringLiteralValue`] node will always have >=1 [`StringLiteral`] parts
    /// inside it. This method checks whether the value of the concatenated parts is equal
    /// to the empty string, not whether the string has 0 parts inside it.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the total length of the string literal value, in bytes, not
    /// [`char`]s or graphemes.
    pub fn len(&self) -> usize {
        self.iter().fold(0, |acc, part| acc + part.value.len())
    }

    /// Returns an iterator over the [`char`]s of each string literal part.
    pub fn chars(&self) -> impl Iterator<Item = char> + Clone + '_ {
        self.iter().flat_map(|part| part.value.chars())
    }

    /// Returns the concatenated string value as a [`str`].
    ///
    /// Note that this will perform an allocation on the first invocation if the
    /// string value is implicitly concatenated.
    pub fn to_str(&self) -> &str {
        match &self.inner {
            StringLiteralValueInner::Single(value) => value.as_str(),
            StringLiteralValueInner::Concatenated(value) => value.to_str(),
        }
    }
}

impl<'a> IntoIterator for &'a StringLiteralValue {
    type Item = &'a StringLiteral;
    type IntoIter = Iter<'a, StringLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut StringLiteralValue {
    type Item = &'a mut StringLiteral;
    type IntoIter = IterMut<'a, StringLiteral>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<str> for StringLiteralValue {
    fn eq(&self, other: &str) -> bool {
        if self.len() != other.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.chars().zip(other.chars()).all(|(c1, c2)| c1 == c2)
    }
}

impl fmt::Display for StringLiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

/// An internal representation of [`StringLiteralValue`].
#[derive(Clone, Debug, PartialEq)]
enum StringLiteralValueInner {
    /// A single string literal i.e., `"foo"`.
    Single(StringLiteral),

    /// An implicitly concatenated string literals i.e., `"foo" "bar"`.
    Concatenated(ConcatenatedStringLiteral),
}

bitflags! {
    #[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct StringLiteralFlagsInner: u8 {
        /// The string uses double quotes (e.g. `"foo"`).
        /// If this flag is not set, the string uses single quotes (`'foo'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted (`"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix, e.g. `u"foo"`.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set;
        /// it is therefore invalid for this flag to be set
        /// if `R_PREFIX` is also set.
        const U_PREFIX = 1 << 2;

        /// The string has an `r` prefix, meaning it is a raw string
        /// with a lowercase 'r' (e.g. `r"foo\."`).
        /// It is invalid to set this flag if `U_PREFIX` is also set.
        const R_PREFIX_LOWER = 1 << 3;

        /// The string has an `R` prefix, meaning it is a raw string
        /// with an uppercase 'R' (e.g. `R'foo\d'`).
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 4;

        /// The string was deemed invalid by the parser.
        const INVALID = 1 << 5;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a string literal.
///
/// ## Notes on usage
///
/// If you're using a `Generator` from the `ruff_python_codegen` crate to generate a lint-rule fix
/// from an existing string literal, consider passing along the [`StringLiteral::flags`] field or
/// the result of the [`StringLiteralValue::first_literal_flags`] method. If you don't have an
/// existing string but have a `Checker` from the `ruff_linter` crate available, consider using
/// `Checker::default_string_flags` to create instances of this struct; this method will properly
/// handle surrounding f-strings. For usage that doesn't fit into one of these categories, the
/// public constructor [`StringLiteralFlags::empty`] can be used.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct StringLiteralFlags(StringLiteralFlagsInner);

impl StringLiteralFlags {
    /// Construct a new [`StringLiteralFlags`] with **no flags set**.
    ///
    /// See [`StringLiteralFlags::with_quote_style`], [`StringLiteralFlags::with_triple_quotes`],
    /// and [`StringLiteralFlags::with_prefix`] for ways of setting the quote style (single or
    /// double), enabling triple quotes, and adding prefixes (such as `r` or `u`), respectively.
    ///
    /// See the documentation for [`StringLiteralFlags`] for additional caveats on this constructor,
    /// and situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(StringLiteralFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(StringLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            StringLiteralFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_prefix(self, prefix: StringLiteralPrefix) -> Self {
        let StringLiteralFlags(flags) = self;
        match prefix {
            StringLiteralPrefix::Empty => Self(
                flags
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: false } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_LOWER)
                    - StringLiteralFlagsInner::R_PREFIX_UPPER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Raw { uppercase: true } => Self(
                (flags | StringLiteralFlagsInner::R_PREFIX_UPPER)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::U_PREFIX,
            ),
            StringLiteralPrefix::Unicode => Self(
                (flags | StringLiteralFlagsInner::U_PREFIX)
                    - StringLiteralFlagsInner::R_PREFIX_LOWER
                    - StringLiteralFlagsInner::R_PREFIX_UPPER,
            ),
        }
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= StringLiteralFlagsInner::INVALID;
        self
    }

    pub const fn prefix(self) -> StringLiteralPrefix {
        if self.0.contains(StringLiteralFlagsInner::U_PREFIX) {
            debug_assert!(!self.0.intersects(
                StringLiteralFlagsInner::R_PREFIX_LOWER
                    .union(StringLiteralFlagsInner::R_PREFIX_UPPER)
            ));
            StringLiteralPrefix::Unicode
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER));
            StringLiteralPrefix::Raw { uppercase: false }
        } else if self.0.contains(StringLiteralFlagsInner::R_PREFIX_UPPER) {
            StringLiteralPrefix::Raw { uppercase: true }
        } else {
            StringLiteralPrefix::Empty
        }
    }
}

impl StringFlags for StringLiteralFlags {
    /// Return the quoting style (single or double quotes)
    /// used by the string's opener and closer:
    /// - `"a"` -> `QuoteStyle::Double`
    /// - `'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(StringLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    /// Return `true` if the string is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `"""bar"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(StringLiteralFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Regular(self.prefix())
    }
}

impl fmt::Debug for StringLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single string literal which is part of an
/// [`ExprStringLiteral`].
#[derive(Clone, Debug, PartialEq)]
pub struct StringLiteral {
    pub range: TextRange,
    pub value: Box<str>,
    pub flags: StringLiteralFlags,
}

impl Deref for StringLiteral {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl StringLiteral {
    /// Extracts a string slice containing the entire `String`.
    pub fn as_str(&self) -> &str {
        self
    }

    /// Creates an invalid string literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: "".into(),
            flags: StringLiteralFlags::empty().with_invalid(),
        }
    }

    /// The range of the string literal's contents.
    ///
    /// This excludes any prefixes, opening quotes or closing quotes.
    pub fn content_range(&self) -> TextRange {
        TextRange::new(
            self.start() + self.flags.opener_len(),
            self.end() - self.flags.closer_len(),
        )
    }
}

impl From<StringLiteral> for Expr {
    fn from(payload: StringLiteral) -> Self {
        ExprStringLiteral {
            range: payload.range,
            value: StringLiteralValue::single(payload),
        }
        .into()
    }
}

/// An internal representation of [`StringLiteral`] that represents an
/// implicitly concatenated string.
#[derive(Clone)]
struct ConcatenatedStringLiteral {
    /// The individual [`StringLiteral`] parts that make up the concatenated string.
    strings: Vec<StringLiteral>,
    /// The concatenated string value.
    value: OnceLock<Box<str>>,
}

impl ConcatenatedStringLiteral {
    /// Extracts a string slice containing the entire concatenated string.
    fn to_str(&self) -> &str {
        self.value.get_or_init(|| {
            let concatenated: String = self.strings.iter().map(StringLiteral::as_str).collect();
            concatenated.into_boxed_str()
        })
    }
}

impl PartialEq for ConcatenatedStringLiteral {
    fn eq(&self, other: &Self) -> bool {
        if self.strings.len() != other.strings.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.strings
            .iter()
            .zip(&other.strings)
            .all(|(s1, s2)| s1 == s2)
    }
}

impl Debug for ConcatenatedStringLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcatenatedStringLiteral")
            .field("strings", &self.strings)
            .field("value", &self.to_str())
            .finish()
    }
}

impl ExprBytesLiteral {
    /// Return `Some(literal)` if the bytestring only consists of a single `BytesLiteral` part
    /// (indicating that it is not implicitly concatenated). Otherwise, return `None`.
    pub const fn as_single_part_bytestring(&self) -> Option<&BytesLiteral> {
        match &self.value.inner {
            BytesLiteralValueInner::Single(value) => Some(value),
            BytesLiteralValueInner::Concatenated(_) => None,
        }
    }
}

/// The value representing a [`ExprBytesLiteral`].
#[derive(Clone, Debug, PartialEq)]
pub struct BytesLiteralValue {
    inner: BytesLiteralValueInner,
}

impl BytesLiteralValue {
    /// Create a new bytestring literal with a single [`BytesLiteral`] part.
    pub fn single(value: BytesLiteral) -> Self {
        Self {
            inner: BytesLiteralValueInner::Single(value),
        }
    }

    /// Creates a new bytestring literal with the given values that represents an
    /// implicitly concatenated bytestring.
    ///
    /// # Panics
    ///
    /// Panics if `values` has less than 2 elements.
    /// Use [`BytesLiteralValue::single`] instead.
    pub fn concatenated(values: Vec<BytesLiteral>) -> Self {
        assert!(
            values.len() > 1,
            "Use `BytesLiteralValue::single` to create single-part bytestrings"
        );
        Self {
            inner: BytesLiteralValueInner::Concatenated(values),
        }
    }

    /// Returns `true` if the bytestring is implicitly concatenated.
    pub const fn is_implicit_concatenated(&self) -> bool {
        matches!(self.inner, BytesLiteralValueInner::Concatenated(_))
    }

    /// Returns a slice of all the [`BytesLiteral`] parts contained in this value.
    pub fn as_slice(&self) -> &[BytesLiteral] {
        match &self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_ref(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_slice(),
        }
    }

    /// Returns a mutable slice of all the [`BytesLiteral`] parts contained in this value.
    fn as_mut_slice(&mut self) -> &mut [BytesLiteral] {
        match &mut self.inner {
            BytesLiteralValueInner::Single(value) => std::slice::from_mut(value),
            BytesLiteralValueInner::Concatenated(value) => value.as_mut_slice(),
        }
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value.
    pub fn iter(&self) -> Iter<BytesLiteral> {
        self.as_slice().iter()
    }

    /// Returns an iterator over all the [`BytesLiteral`] parts contained in this value
    /// that allows modification.
    pub fn iter_mut(&mut self) -> IterMut<BytesLiteral> {
        self.as_mut_slice().iter_mut()
    }

    /// Return `true` if the node represents an empty bytestring.
    ///
    /// Note that a [`BytesLiteralValue`] node will always have >=1 [`BytesLiteral`] parts
    /// inside it. This method checks whether the value of the concatenated parts is equal
    /// to the empty bytestring, not whether the bytestring has 0 parts inside it.
    pub fn is_empty(&self) -> bool {
        self.iter().all(|part| part.is_empty())
    }

    /// Returns the length of the concatenated bytestring.
    pub fn len(&self) -> usize {
        self.iter().map(|part| part.len()).sum()
    }

    /// Returns an iterator over the bytes of the concatenated bytestring.
    pub fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.iter().flat_map(|part| part.as_slice().iter().copied())
    }
}

impl<'a> IntoIterator for &'a BytesLiteralValue {
    type Item = &'a BytesLiteral;
    type IntoIter = Iter<'a, BytesLiteral>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut BytesLiteralValue {
    type Item = &'a mut BytesLiteral;
    type IntoIter = IterMut<'a, BytesLiteral>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<[u8]> for BytesLiteralValue {
    fn eq(&self, other: &[u8]) -> bool {
        if self.len() != other.len() {
            return false;
        }
        // The `zip` here is safe because we have checked the length of both parts.
        self.bytes()
            .zip(other.iter().copied())
            .all(|(b1, b2)| b1 == b2)
    }
}

impl<'a> From<&'a BytesLiteralValue> for Cow<'a, [u8]> {
    fn from(value: &'a BytesLiteralValue) -> Self {
        match &value.inner {
            BytesLiteralValueInner::Single(BytesLiteral {
                value: bytes_value, ..
            }) => Cow::from(bytes_value.as_ref()),
            BytesLiteralValueInner::Concatenated(bytes_literal_vec) => Cow::Owned(
                bytes_literal_vec
                    .iter()
                    .flat_map(|bytes_literal| bytes_literal.value.to_vec())
                    .collect::<Vec<u8>>(),
            ),
        }
    }
}

/// An internal representation of [`BytesLiteralValue`].
#[derive(Clone, Debug, PartialEq)]
enum BytesLiteralValueInner {
    /// A single-part bytestring literal i.e., `b"foo"`.
    Single(BytesLiteral),

    /// An implicitly concatenated bytestring literal i.e., `b"foo" b"bar"`.
    Concatenated(Vec<BytesLiteral>),
}

bitflags! {
    #[derive(Default, Copy, Clone, PartialEq, Eq, Hash)]
    struct BytesLiteralFlagsInner: u8 {
        /// The bytestring uses double quotes (e.g. `b"foo"`).
        /// If this flag is not set, the bytestring uses single quotes (e.g. `b'foo'`).
        const DOUBLE = 1 << 0;

        /// The bytestring is triple-quoted (e.g. `b"""foo"""`):
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The bytestring has an `r` prefix (e.g. `rb"foo"`),
        /// meaning it is a raw bytestring with a lowercase 'r'.
        const R_PREFIX_LOWER = 1 << 2;

        /// The bytestring has an `R` prefix (e.g. `Rb"foo"`),
        /// meaning it is a raw bytestring with an uppercase 'R'.
        /// See https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix, but not for any other prefix
        const R_PREFIX_UPPER = 1 << 3;

        /// The bytestring was deemed invalid by the parser.
        const INVALID = 1 << 4;
    }
}

/// Flags that can be queried to obtain information
/// regarding the prefixes and quotes used for a bytes literal.
///
/// ## Notes on usage
///
/// If you're using a `Generator` from the `ruff_python_codegen` crate to generate a lint-rule fix
/// from an existing bytes literal, consider passing along the [`BytesLiteral::flags`] field. If
/// you don't have an existing literal but have a `Checker` from the `ruff_linter` crate available,
/// consider using `Checker::default_bytes_flags` to create instances of this struct; this method
/// will properly handle surrounding f-strings. For usage that doesn't fit into one of these
/// categories, the public constructor [`BytesLiteralFlags::empty`] can be used.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct BytesLiteralFlags(BytesLiteralFlagsInner);

impl BytesLiteralFlags {
    /// Construct a new [`BytesLiteralFlags`] with **no flags set**.
    ///
    /// See [`BytesLiteralFlags::with_quote_style`], [`BytesLiteralFlags::with_triple_quotes`], and
    /// [`BytesLiteralFlags::with_prefix`] for ways of setting the quote style (single or double),
    /// enabling triple quotes, and adding prefixes (such as `r`), respectively.
    ///
    /// See the documentation for [`BytesLiteralFlags`] for additional caveats on this constructor,
    /// and situations in which alternative ways to construct this struct should be used, especially
    /// when writing lint rules.
    pub fn empty() -> Self {
        Self(BytesLiteralFlagsInner::empty())
    }

    #[must_use]
    pub fn with_quote_style(mut self, quote_style: Quote) -> Self {
        self.0
            .set(BytesLiteralFlagsInner::DOUBLE, quote_style.is_double());
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0.set(
            BytesLiteralFlagsInner::TRIPLE_QUOTED,
            triple_quotes.is_yes(),
        );
        self
    }

    #[must_use]
    pub fn with_prefix(mut self, prefix: ByteStringPrefix) -> Self {
        match prefix {
            ByteStringPrefix::Regular => {
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_LOWER;
                self.0 -= BytesLiteralFlagsInner::R_PREFIX_UPPER;
            }
            ByteStringPrefix::Raw { uppercase_r } => {
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_UPPER, uppercase_r);
                self.0
                    .set(BytesLiteralFlagsInner::R_PREFIX_LOWER, !uppercase_r);
            }
        }
        self
    }

    #[must_use]
    pub fn with_invalid(mut self) -> Self {
        self.0 |= BytesLiteralFlagsInner::INVALID;
        self
    }

    pub const fn prefix(self) -> ByteStringPrefix {
        if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_LOWER) {
            debug_assert!(!self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER));
            ByteStringPrefix::Raw { uppercase_r: false }
        } else if self.0.contains(BytesLiteralFlagsInner::R_PREFIX_UPPER) {
            ByteStringPrefix::Raw { uppercase_r: true }
        } else {
            ByteStringPrefix::Regular
        }
    }
}

impl StringFlags for BytesLiteralFlags {
    /// Return `true` if the bytestring is triple-quoted, i.e.,
    /// it begins and ends with three consecutive quote characters.
    /// For example: `b"""{bar}"""`
    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(BytesLiteralFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    /// Return the quoting style (single or double quotes)
    /// used by the bytestring's opener and closer:
    /// - `b"a"` -> `QuoteStyle::Double`
    /// - `b'a'` -> `QuoteStyle::Single`
    fn quote_style(self) -> Quote {
        if self.0.contains(BytesLiteralFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        AnyStringPrefix::Bytes(self.prefix())
    }
}

impl fmt::Debug for BytesLiteralFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BytesLiteralFlags")
            .field("quote_style", &self.quote_style())
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .finish()
    }
}

/// An AST node that represents a single bytes literal which is part of an
/// [`ExprBytesLiteral`].
#[derive(Clone, Debug, PartialEq)]
pub struct BytesLiteral {
    pub range: TextRange,
    pub value: Box<[u8]>,
    pub flags: BytesLiteralFlags,
}

impl Deref for BytesLiteral {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl BytesLiteral {
    /// Extracts a byte slice containing the entire [`BytesLiteral`].
    pub fn as_slice(&self) -> &[u8] {
        self
    }

    /// Creates a new invalid bytes literal with the given range.
    pub fn invalid(range: TextRange) -> Self {
        Self {
            range,
            value: Box::new([]),
            flags: BytesLiteralFlags::empty().with_invalid(),
        }
    }
}

impl From<BytesLiteral> for Expr {
    fn from(payload: BytesLiteral) -> Self {
        ExprBytesLiteral {
            range: payload.range,
            value: BytesLiteralValue::single(payload),
        }
        .into()
    }
}

bitflags! {
    /// Flags that can be queried to obtain information
    /// regarding the prefixes and quotes used for a string literal.
    ///
    /// Note that not all of these flags can be validly combined -- e.g.,
    /// it is invalid to combine the `U_PREFIX` flag with any other
    /// of the `*_PREFIX` flags. As such, the recommended way to set the
    /// prefix flags is by calling the `as_flags()` method on the
    /// `StringPrefix` enum.
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
    struct AnyStringFlagsInner: u8 {
        /// The string uses double quotes (`"`).
        /// If this flag is not set, the string uses single quotes (`'`).
        const DOUBLE = 1 << 0;

        /// The string is triple-quoted:
        /// it begins and ends with three consecutive quote characters.
        const TRIPLE_QUOTED = 1 << 1;

        /// The string has a `u` or `U` prefix.
        /// While this prefix is a no-op at runtime,
        /// strings with this prefix can have no other prefixes set.
        const U_PREFIX = 1 << 2;

        /// The string has a `b` or `B` prefix.
        /// This means that the string is a sequence of `int`s at runtime,
        /// rather than a sequence of `str`s.
        /// Strings with this flag can also be raw strings,
        /// but can have no other prefixes.
        const B_PREFIX = 1 << 3;

        /// The string has a `f` or `F` prefix, meaning it is an f-string.
        /// F-strings can also be raw strings,
        /// but can have no other prefixes.
        const F_PREFIX = 1 << 4;

        /// The string has an `r` prefix, meaning it is a raw string.
        /// F-strings and byte-strings can be raw,
        /// as can strings with no other prefixes.
        /// U-strings cannot be raw.
        const R_PREFIX_LOWER = 1 << 5;

        /// The string has an `R` prefix, meaning it is a raw string.
        /// The casing of the `r`/`R` has no semantic significance at runtime;
        /// see https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#r-strings-and-r-strings
        /// for why we track the casing of the `r` prefix,
        /// but not for any other prefix
        const R_PREFIX_UPPER = 1 << 6;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyStringFlags(AnyStringFlagsInner);

impl AnyStringFlags {
    #[must_use]
    pub fn with_prefix(mut self, prefix: AnyStringPrefix) -> Self {
        self.0 |= match prefix {
            // regular strings
            AnyStringPrefix::Regular(StringLiteralPrefix::Empty) => AnyStringFlagsInner::empty(),
            AnyStringPrefix::Regular(StringLiteralPrefix::Unicode) => AnyStringFlagsInner::U_PREFIX,
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false }) => {
                AnyStringFlagsInner::R_PREFIX_LOWER
            }
            AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true }) => {
                AnyStringFlagsInner::R_PREFIX_UPPER
            }

            // bytestrings
            AnyStringPrefix::Bytes(ByteStringPrefix::Regular) => AnyStringFlagsInner::B_PREFIX,
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::B_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }

            // f-strings
            AnyStringPrefix::Format(FStringPrefix::Regular) => AnyStringFlagsInner::F_PREFIX,
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_LOWER)
            }
            AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true }) => {
                AnyStringFlagsInner::F_PREFIX.union(AnyStringFlagsInner::R_PREFIX_UPPER)
            }
        };
        self
    }

    pub fn new(prefix: AnyStringPrefix, quotes: Quote, triple_quotes: TripleQuotes) -> Self {
        Self(AnyStringFlagsInner::empty())
            .with_prefix(prefix)
            .with_quote_style(quotes)
            .with_triple_quotes(triple_quotes)
    }

    /// Does the string have a `u` or `U` prefix?
    pub const fn is_u_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::U_PREFIX)
    }

    /// Does the string have an `r` or `R` prefix?
    pub const fn is_raw_string(self) -> bool {
        self.0.intersects(
            AnyStringFlagsInner::R_PREFIX_LOWER.union(AnyStringFlagsInner::R_PREFIX_UPPER),
        )
    }

    /// Does the string have an `f` or `F` prefix?
    pub const fn is_f_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::F_PREFIX)
    }

    /// Does the string have a `b` or `B` prefix?
    pub const fn is_byte_string(self) -> bool {
        self.0.contains(AnyStringFlagsInner::B_PREFIX)
    }

    #[must_use]
    pub fn with_quote_style(mut self, quotes: Quote) -> Self {
        match quotes {
            Quote::Double => self.0 |= AnyStringFlagsInner::DOUBLE,
            Quote::Single => self.0 -= AnyStringFlagsInner::DOUBLE,
        }
        self
    }

    #[must_use]
    pub fn with_triple_quotes(mut self, triple_quotes: TripleQuotes) -> Self {
        self.0
            .set(AnyStringFlagsInner::TRIPLE_QUOTED, triple_quotes.is_yes());
        self
    }
}

impl StringFlags for AnyStringFlags {
    /// Does the string use single or double quotes in its opener and closer?
    fn quote_style(self) -> Quote {
        if self.0.contains(AnyStringFlagsInner::DOUBLE) {
            Quote::Double
        } else {
            Quote::Single
        }
    }

    fn triple_quotes(self) -> TripleQuotes {
        if self.0.contains(AnyStringFlagsInner::TRIPLE_QUOTED) {
            TripleQuotes::Yes
        } else {
            TripleQuotes::No
        }
    }

    fn prefix(self) -> AnyStringPrefix {
        let AnyStringFlags(flags) = self;

        // f-strings
        if flags.contains(AnyStringFlagsInner::F_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Format(FStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Format(FStringPrefix::Regular);
        }

        // bytestrings
        if flags.contains(AnyStringFlagsInner::B_PREFIX) {
            if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: false });
            }
            if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
                return AnyStringPrefix::Bytes(ByteStringPrefix::Raw { uppercase_r: true });
            }
            return AnyStringPrefix::Bytes(ByteStringPrefix::Regular);
        }

        // all other strings
        if flags.contains(AnyStringFlagsInner::R_PREFIX_LOWER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: false });
        }
        if flags.contains(AnyStringFlagsInner::R_PREFIX_UPPER) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Raw { uppercase: true });
        }
        if flags.contains(AnyStringFlagsInner::U_PREFIX) {
            return AnyStringPrefix::Regular(StringLiteralPrefix::Unicode);
        }
        AnyStringPrefix::Regular(StringLiteralPrefix::Empty)
    }
}

impl fmt::Debug for AnyStringFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyStringFlags")
            .field("prefix", &self.prefix())
            .field("triple_quoted", &self.is_triple_quoted())
            .field("quote_style", &self.quote_style())
            .finish()
    }
}

impl From<AnyStringFlags> for StringLiteralFlags {
    fn from(value: AnyStringFlags) -> StringLiteralFlags {
        let AnyStringPrefix::Regular(prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a regular string",
                value.prefix()
            )
        };
        StringLiteralFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(prefix)
            .with_triple_quotes(value.triple_quotes())
    }
}

impl From<StringLiteralFlags> for AnyStringFlags {
    fn from(value: StringLiteralFlags) -> Self {
        value.as_any_string_flags()
    }
}

impl From<AnyStringFlags> for BytesLiteralFlags {
    fn from(value: AnyStringFlags) -> BytesLiteralFlags {
        let AnyStringPrefix::Bytes(bytestring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into a bytestring",
                value.prefix()
            )
        };
        BytesLiteralFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(bytestring_prefix)
            .with_triple_quotes(value.triple_quotes())
    }
}

impl From<BytesLiteralFlags> for AnyStringFlags {
    fn from(value: BytesLiteralFlags) -> Self {
        value.as_any_string_flags()
    }
}

impl From<AnyStringFlags> for FStringFlags {
    fn from(value: AnyStringFlags) -> FStringFlags {
        let AnyStringPrefix::Format(fstring_prefix) = value.prefix() else {
            unreachable!(
                "Should never attempt to convert {} into an f-string",
                value.prefix()
            )
        };
        FStringFlags::empty()
            .with_quote_style(value.quote_style())
            .with_prefix(fstring_prefix)
            .with_triple_quotes(value.triple_quotes())
    }
}

impl From<FStringFlags> for AnyStringFlags {
    fn from(value: FStringFlags) -> Self {
        value.as_any_string_flags()
    }
}

#[derive(Clone, Debug, PartialEq, is_macro::Is)]
pub enum Number {
    Int(int::Int),
    Float(f64),
    Complex { real: f64, imag: f64 },
}

impl ExprName {
    pub fn id(&self) -> &Name {
        &self.id
    }

    /// Returns `true` if this node represents an invalid name i.e., the `ctx` is [`Invalid`].
    ///
    /// [`Invalid`]: ExprContext::Invalid
    pub const fn is_invalid(&self) -> bool {
        matches!(self.ctx, ExprContext::Invalid)
    }
}

impl ExprList {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprList {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl ExprTuple {
    pub fn iter(&self) -> std::slice::Iter<'_, Expr> {
        self.elts.iter()
    }

    pub fn len(&self) -> usize {
        self.elts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elts.is_empty()
    }
}

impl<'a> IntoIterator for &'a ExprTuple {
    type IntoIter = std::slice::Iter<'a, Expr>;
    type Item = &'a Expr;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// See also [expr_context](https://docs.python.org/3/library/ast.html#ast.expr_context)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum ExprContext {
    Load,
    Store,
    Del,
    Invalid,
}

/// See also [boolop](https://docs.python.org/3/library/ast.html#ast.BoolOp)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum BoolOp {
    And,
    Or,
}

impl BoolOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BoolOp::And => "and",
            BoolOp::Or => "or",
        }
    }
}

impl fmt::Display for BoolOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [operator](https://docs.python.org/3/library/ast.html#ast.operator)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum Operator {
    Add,
    Sub,
    Mult,
    MatMult,
    Div,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
    FloorDiv,
}

impl Operator {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mult => "*",
            Operator::MatMult => "@",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::Pow => "**",
            Operator::LShift => "<<",
            Operator::RShift => ">>",
            Operator::BitOr => "|",
            Operator::BitXor => "^",
            Operator::BitAnd => "&",
            Operator::FloorDiv => "//",
        }
    }

    /// Returns the dunder method name for the operator.
    pub const fn dunder(self) -> &'static str {
        match self {
            Operator::Add => "__add__",
            Operator::Sub => "__sub__",
            Operator::Mult => "__mul__",
            Operator::MatMult => "__matmul__",
            Operator::Div => "__truediv__",
            Operator::Mod => "__mod__",
            Operator::Pow => "__pow__",
            Operator::LShift => "__lshift__",
            Operator::RShift => "__rshift__",
            Operator::BitOr => "__or__",
            Operator::BitXor => "__xor__",
            Operator::BitAnd => "__and__",
            Operator::FloorDiv => "__floordiv__",
        }
    }

    /// Returns the in-place dunder method name for the operator.
    pub const fn in_place_dunder(self) -> &'static str {
        match self {
            Operator::Add => "__iadd__",
            Operator::Sub => "__isub__",
            Operator::Mult => "__imul__",
            Operator::MatMult => "__imatmul__",
            Operator::Div => "__itruediv__",
            Operator::Mod => "__imod__",
            Operator::Pow => "__ipow__",
            Operator::LShift => "__ilshift__",
            Operator::RShift => "__irshift__",
            Operator::BitOr => "__ior__",
            Operator::BitXor => "__ixor__",
            Operator::BitAnd => "__iand__",
            Operator::FloorDiv => "__ifloordiv__",
        }
    }

    /// Returns the reflected dunder method name for the operator.
    pub const fn reflected_dunder(self) -> &'static str {
        match self {
            Operator::Add => "__radd__",
            Operator::Sub => "__rsub__",
            Operator::Mult => "__rmul__",
            Operator::MatMult => "__rmatmul__",
            Operator::Div => "__rtruediv__",
            Operator::Mod => "__rmod__",
            Operator::Pow => "__rpow__",
            Operator::LShift => "__rlshift__",
            Operator::RShift => "__rrshift__",
            Operator::BitOr => "__ror__",
            Operator::BitXor => "__rxor__",
            Operator::BitAnd => "__rand__",
            Operator::FloorDiv => "__rfloordiv__",
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [unaryop](https://docs.python.org/3/library/ast.html#ast.unaryop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum UnaryOp {
    Invert,
    Not,
    UAdd,
    USub,
}

impl UnaryOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Invert => "~",
            UnaryOp::Not => "not",
            UnaryOp::UAdd => "+",
            UnaryOp::USub => "-",
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [cmpop](https://docs.python.org/3/library/ast.html#ast.cmpop)
#[derive(Clone, Debug, PartialEq, is_macro::Is, Copy, Hash, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtE,
    Gt,
    GtE,
    Is,
    IsNot,
    In,
    NotIn,
}

impl CmpOp {
    pub const fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }

    #[must_use]
    pub const fn negate(&self) -> Self {
        match self {
            CmpOp::Eq => CmpOp::NotEq,
            CmpOp::NotEq => CmpOp::Eq,
            CmpOp::Lt => CmpOp::GtE,
            CmpOp::LtE => CmpOp::Gt,
            CmpOp::Gt => CmpOp::LtE,
            CmpOp::GtE => CmpOp::Lt,
            CmpOp::Is => CmpOp::IsNot,
            CmpOp::IsNot => CmpOp::Is,
            CmpOp::In => CmpOp::NotIn,
            CmpOp::NotIn => CmpOp::In,
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// See also [comprehension](https://docs.python.org/3/library/ast.html#ast.comprehension)
#[derive(Clone, Debug, PartialEq)]
pub struct Comprehension {
    pub range: TextRange,
    pub target: Expr,
    pub iter: Expr,
    pub ifs: Vec<Expr>,
    pub is_async: bool,
}

/// See also [ExceptHandler](https://docs.python.org/3/library/ast.html#ast.ExceptHandler)
#[derive(Clone, Debug, PartialEq)]
pub struct ExceptHandlerExceptHandler {
    pub range: TextRange,
    pub type_: Option<Box<Expr>>,
    pub name: Option<Identifier>,
    pub body: Vec<Stmt>,
}

/// See also [arg](https://docs.python.org/3/library/ast.html#ast.arg)
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub range: TextRange,
    pub name: Identifier,
    pub annotation: Option<Box<Expr>>,
}

impl Parameter {
    pub const fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn annotation(&self) -> Option<&Expr> {
        self.annotation.as_deref()
    }
}

/// See also [keyword](https://docs.python.org/3/library/ast.html#ast.keyword)
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    pub range: TextRange,
    pub arg: Option<Identifier>,
    pub value: Expr,
}

/// See also [alias](https://docs.python.org/3/library/ast.html#ast.alias)
#[derive(Clone, Debug, PartialEq)]
pub struct Alias {
    pub range: TextRange,
    pub name: Identifier,
    pub asname: Option<Identifier>,
}

/// See also [withitem](https://docs.python.org/3/library/ast.html#ast.withitem)
#[derive(Clone, Debug, PartialEq)]
pub struct WithItem {
    pub range: TextRange,
    pub context_expr: Expr,
    pub optional_vars: Option<Box<Expr>>,
}

/// See also [match_case](https://docs.python.org/3/library/ast.html#ast.match_case)
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    pub range: TextRange,
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Vec<Stmt>,
}

impl Pattern {
    /// Checks if the [`Pattern`] is an [irrefutable pattern].
    ///
    /// [irrefutable pattern]: https://peps.python.org/pep-0634/#irrefutable-case-blocks
    pub fn is_irrefutable(&self) -> bool {
        self.irrefutable_pattern().is_some()
    }

    /// Return `Some(IrrefutablePattern)` if `self` is irrefutable or `None` otherwise.
    pub fn irrefutable_pattern(&self) -> Option<IrrefutablePattern> {
        match self {
            Pattern::MatchAs(PatternMatchAs {
                pattern,
                name,
                range,
            }) => match pattern {
                Some(pattern) => pattern.irrefutable_pattern(),
                None => match name {
                    Some(name) => Some(IrrefutablePattern {
                        kind: IrrefutablePatternKind::Name(name.id.clone()),
                        range: *range,
                    }),
                    None => Some(IrrefutablePattern {
                        kind: IrrefutablePatternKind::Wildcard,
                        range: *range,
                    }),
                },
            },
            Pattern::MatchOr(PatternMatchOr { patterns, .. }) => {
                patterns.iter().find_map(Pattern::irrefutable_pattern)
            }
            _ => None,
        }
    }

    /// Checks if the [`Pattern`] is a [wildcard pattern].
    ///
    /// The following are wildcard patterns:
    /// ```python
    /// match subject:
    ///     case _ as x: ...
    ///     case _ | _: ...
    ///     case _: ...
    /// ```
    ///
    /// [wildcard pattern]: https://docs.python.org/3/reference/compound_stmts.html#wildcard-patterns
    pub fn is_wildcard(&self) -> bool {
        match self {
            Pattern::MatchAs(PatternMatchAs { pattern, .. }) => {
                pattern.as_deref().is_none_or(Pattern::is_wildcard)
            }
            Pattern::MatchOr(PatternMatchOr { patterns, .. }) => {
                patterns.iter().all(Pattern::is_wildcard)
            }
            _ => false,
        }
    }
}

pub struct IrrefutablePattern {
    pub kind: IrrefutablePatternKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrrefutablePatternKind {
    Name(Name),
    Wildcard,
}

/// See also [MatchValue](https://docs.python.org/3/library/ast.html#ast.MatchValue)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchValue {
    pub range: TextRange,
    pub value: Box<Expr>,
}

/// See also [MatchSingleton](https://docs.python.org/3/library/ast.html#ast.MatchSingleton)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchSingleton {
    pub range: TextRange,
    pub value: Singleton,
}

/// See also [MatchSequence](https://docs.python.org/3/library/ast.html#ast.MatchSequence)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchSequence {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}

/// See also [MatchMapping](https://docs.python.org/3/library/ast.html#ast.MatchMapping)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchMapping {
    pub range: TextRange,
    pub keys: Vec<Expr>,
    pub patterns: Vec<Pattern>,
    pub rest: Option<Identifier>,
}

/// See also [MatchClass](https://docs.python.org/3/library/ast.html#ast.MatchClass)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchClass {
    pub range: TextRange,
    pub cls: Box<Expr>,
    pub arguments: PatternArguments,
}

/// An AST node to represent the arguments to a [`PatternMatchClass`], i.e., the
/// parenthesized contents in `case Point(1, x=0, y=0)`.
///
/// Like [`Arguments`], but for [`PatternMatchClass`].
#[derive(Clone, Debug, PartialEq)]
pub struct PatternArguments {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
    pub keywords: Vec<PatternKeyword>,
}

/// An AST node to represent the keyword arguments to a [`PatternMatchClass`], i.e., the
/// `x=0` and `y=0` in `case Point(x=0, y=0)`.
///
/// Like [`Keyword`], but for [`PatternMatchClass`].
#[derive(Clone, Debug, PartialEq)]
pub struct PatternKeyword {
    pub range: TextRange,
    pub attr: Identifier,
    pub pattern: Pattern,
}

/// See also [MatchStar](https://docs.python.org/3/library/ast.html#ast.MatchStar)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchStar {
    pub range: TextRange,
    pub name: Option<Identifier>,
}

/// See also [MatchAs](https://docs.python.org/3/library/ast.html#ast.MatchAs)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchAs {
    pub range: TextRange,
    pub pattern: Option<Box<Pattern>>,
    pub name: Option<Identifier>,
}

/// See also [MatchOr](https://docs.python.org/3/library/ast.html#ast.MatchOr)
#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatchOr {
    pub range: TextRange,
    pub patterns: Vec<Pattern>,
}

impl TypeParam {
    pub const fn name(&self) -> &Identifier {
        match self {
            Self::TypeVar(x) => &x.name,
            Self::ParamSpec(x) => &x.name,
            Self::TypeVarTuple(x) => &x.name,
        }
    }

    pub fn default(&self) -> Option<&Expr> {
        match self {
            Self::TypeVar(x) => x.default.as_deref(),
            Self::ParamSpec(x) => x.default.as_deref(),
            Self::TypeVarTuple(x) => x.default.as_deref(),
        }
    }
}

/// See also [TypeVar](https://docs.python.org/3/library/ast.html#ast.TypeVar)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamTypeVar {
    pub range: TextRange,
    pub name: Identifier,
    pub bound: Option<Box<Expr>>,
    pub default: Option<Box<Expr>>,
}

/// See also [ParamSpec](https://docs.python.org/3/library/ast.html#ast.ParamSpec)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamParamSpec {
    pub range: TextRange,
    pub name: Identifier,
    pub default: Option<Box<Expr>>,
}

/// See also [TypeVarTuple](https://docs.python.org/3/library/ast.html#ast.TypeVarTuple)
#[derive(Clone, Debug, PartialEq)]
pub struct TypeParamTypeVarTuple {
    pub range: TextRange,
    pub name: Identifier,
    pub default: Option<Box<Expr>>,
}

/// See also [decorator](https://docs.python.org/3/library/ast.html#ast.decorator)
#[derive(Clone, Debug, PartialEq)]
pub struct Decorator {
    pub range: TextRange,
    pub expression: Expr,
}

/// Enumeration of the two kinds of parameter
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AnyParameterRef<'a> {
    /// Variadic parameters cannot have default values,
    /// e.g. both `*args` and `**kwargs` in the following function:
    ///
    /// ```python
    /// def foo(*args, **kwargs): pass
    /// ```
    Variadic(&'a Parameter),

    /// Non-variadic parameters can have default values,
    /// though they won't necessarily always have them:
    ///
    /// ```python
    /// def bar(a=1, /, b=2, *, c=3): pass
    /// ```
    NonVariadic(&'a ParameterWithDefault),
}

impl<'a> AnyParameterRef<'a> {
    pub const fn as_parameter(self) -> &'a Parameter {
        match self {
            Self::NonVariadic(param) => &param.parameter,
            Self::Variadic(param) => param,
        }
    }

    pub const fn name(self) -> &'a Identifier {
        &self.as_parameter().name
    }

    pub const fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic(_))
    }

    pub fn annotation(self) -> Option<&'a Expr> {
        self.as_parameter().annotation.as_deref()
    }

    pub fn default(self) -> Option<&'a Expr> {
        match self {
            Self::NonVariadic(param) => param.default.as_deref(),
            Self::Variadic(_) => None,
        }
    }
}

impl Ranged for AnyParameterRef<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::NonVariadic(param) => param.range,
            Self::Variadic(param) => param.range,
        }
    }
}

/// An alternative type of AST `arguments`. This is ruff_python_parser-friendly and human-friendly definition of function arguments.
/// This form also has advantage to implement pre-order traverse.
///
/// `defaults` and `kw_defaults` fields are removed and the default values are placed under each [`ParameterWithDefault`] typed argument.
/// `vararg` and `kwarg` are still typed as `arg` because they never can have a default value.
///
/// The original Python-style AST type orders `kwonlyargs` fields by default existence; [Parameters] has location-ordered `kwonlyargs` fields.
///
/// NOTE: This type differs from the original Python AST. See: [arguments](https://docs.python.org/3/library/ast.html#ast.arguments).

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Parameters {
    pub range: TextRange,
    pub posonlyargs: Vec<ParameterWithDefault>,
    pub args: Vec<ParameterWithDefault>,
    pub vararg: Option<Box<Parameter>>,
    pub kwonlyargs: Vec<ParameterWithDefault>,
    pub kwarg: Option<Box<Parameter>>,
}

impl Parameters {
    /// Returns an iterator over all non-variadic parameters included in this [`Parameters`] node.
    ///
    /// The variadic parameters (`.vararg` and `.kwarg`) can never have default values;
    /// non-variadic parameters sometimes will.
    pub fn iter_non_variadic_params(&self) -> impl Iterator<Item = &ParameterWithDefault> {
        self.posonlyargs
            .iter()
            .chain(&self.args)
            .chain(&self.kwonlyargs)
    }

    /// Returns the [`ParameterWithDefault`] with the given name, or `None` if no such [`ParameterWithDefault`] exists.
    pub fn find(&self, name: &str) -> Option<&ParameterWithDefault> {
        self.iter_non_variadic_params()
            .find(|arg| arg.parameter.name.as_str() == name)
    }

    /// Returns an iterator over all parameters included in this [`Parameters`] node.
    pub fn iter(&self) -> ParametersIterator {
        ParametersIterator::new(self)
    }

    /// Returns the total number of parameters included in this [`Parameters`] node.
    pub fn len(&self) -> usize {
        let Parameters {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;
        // Safety: a Python function can have an arbitrary number of parameters,
        // so theoretically this could be a number that wouldn't fit into a usize,
        // which would lead to a panic. A Python function with that many parameters
        // is extremely unlikely outside of generated code, however, and it's even
        // more unlikely that we'd find a function with that many parameters in a
        // source-code file <=4GB large (Ruff's maximum).
        posonlyargs
            .len()
            .checked_add(args.len())
            .and_then(|length| length.checked_add(usize::from(vararg.is_some())))
            .and_then(|length| length.checked_add(kwonlyargs.len()))
            .and_then(|length| length.checked_add(usize::from(kwarg.is_some())))
            .expect("Failed to fit the number of parameters into a usize")
    }

    /// Returns `true` if a parameter with the given name is included in this [`Parameters`].
    pub fn includes(&self, name: &str) -> bool {
        self.iter().any(|param| param.name() == name)
    }

    /// Returns `true` if the [`Parameters`] is empty.
    pub fn is_empty(&self) -> bool {
        self.posonlyargs.is_empty()
            && self.args.is_empty()
            && self.kwonlyargs.is_empty()
            && self.vararg.is_none()
            && self.kwarg.is_none()
    }
}

pub struct ParametersIterator<'a> {
    posonlyargs: Iter<'a, ParameterWithDefault>,
    args: Iter<'a, ParameterWithDefault>,
    vararg: Option<&'a Parameter>,
    kwonlyargs: Iter<'a, ParameterWithDefault>,
    kwarg: Option<&'a Parameter>,
}

impl<'a> ParametersIterator<'a> {
    fn new(parameters: &'a Parameters) -> Self {
        let Parameters {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = parameters;
        Self {
            posonlyargs: posonlyargs.iter(),
            args: args.iter(),
            vararg: vararg.as_deref(),
            kwonlyargs: kwonlyargs.iter(),
            kwarg: kwarg.as_deref(),
        }
    }
}

impl<'a> Iterator for ParametersIterator<'a> {
    type Item = AnyParameterRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        if let Some(param) = posonlyargs.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = args.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = vararg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = kwonlyargs.next() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        kwarg.take().map(AnyParameterRef::Variadic)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        let posonlyargs_len = posonlyargs.len();
        let args_len = args.len();
        let vararg_len = usize::from(vararg.is_some());
        let kwonlyargs_len = kwonlyargs.len();
        let kwarg_len = usize::from(kwarg.is_some());

        let lower = posonlyargs_len
            .saturating_add(args_len)
            .saturating_add(vararg_len)
            .saturating_add(kwonlyargs_len)
            .saturating_add(kwarg_len);

        let upper = posonlyargs_len
            .checked_add(args_len)
            .and_then(|length| length.checked_add(vararg_len))
            .and_then(|length| length.checked_add(kwonlyargs_len))
            .and_then(|length| length.checked_add(kwarg_len));

        (lower, upper)
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for ParametersIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let ParametersIterator {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        if let Some(param) = kwarg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = kwonlyargs.next_back() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        if let Some(param) = vararg.take() {
            return Some(AnyParameterRef::Variadic(param));
        }
        if let Some(param) = args.next_back() {
            return Some(AnyParameterRef::NonVariadic(param));
        }
        posonlyargs.next_back().map(AnyParameterRef::NonVariadic)
    }
}

impl FusedIterator for ParametersIterator<'_> {}

/// We rely on the same invariants outlined in the comment above `Parameters::len()`
/// in order to implement `ExactSizeIterator` here
impl ExactSizeIterator for ParametersIterator<'_> {}

impl<'a> IntoIterator for &'a Parameters {
    type IntoIter = ParametersIterator<'a>;
    type Item = AnyParameterRef<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a Box<Parameters> {
    type IntoIter = ParametersIterator<'a>;
    type Item = AnyParameterRef<'a>;
    fn into_iter(self) -> Self::IntoIter {
        (&**self).into_iter()
    }
}

/// An alternative type of AST `arg`. This is used for each function argument that might have a default value.
/// Used by `Arguments` original type.
///
/// NOTE: This type is different from original Python AST.

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterWithDefault {
    pub range: TextRange,
    pub parameter: Parameter,
    pub default: Option<Box<Expr>>,
}

impl ParameterWithDefault {
    pub fn default(&self) -> Option<&Expr> {
        self.default.as_deref()
    }

    pub const fn name(&self) -> &Identifier {
        self.parameter.name()
    }

    pub fn annotation(&self) -> Option<&Expr> {
        self.parameter.annotation()
    }
}

/// An AST node used to represent the arguments passed to a function call or class definition.
///
/// For example, given:
/// ```python
/// foo(1, 2, 3, bar=4, baz=5)
/// ```
/// The `Arguments` node would span from the left to right parentheses (inclusive), and contain
/// the arguments and keyword arguments in the order they appear in the source code.
///
/// Similarly, given:
/// ```python
/// class Foo(Bar, baz=1, qux=2):
///     pass
/// ```
/// The `Arguments` node would again span from the left to right parentheses (inclusive), and
/// contain the `Bar` argument and the `baz` and `qux` keyword arguments in the order they
/// appear in the source code.
///
/// In the context of a class definition, the Python-style AST refers to the arguments as `bases`,
/// as they represent the "explicitly specified base classes", while the keyword arguments are
/// typically used for `metaclass`, with any additional arguments being passed to the `metaclass`.

#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    pub range: TextRange,
    pub args: Box<[Expr]>,
    pub keywords: Box<[Keyword]>,
}

/// An entry in the argument list of a function call.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ArgOrKeyword<'a> {
    Arg(&'a Expr),
    Keyword(&'a Keyword),
}

impl<'a> ArgOrKeyword<'a> {
    pub const fn value(self) -> &'a Expr {
        match self {
            ArgOrKeyword::Arg(argument) => argument,
            ArgOrKeyword::Keyword(keyword) => &keyword.value,
        }
    }
}

impl<'a> From<&'a Expr> for ArgOrKeyword<'a> {
    fn from(arg: &'a Expr) -> Self {
        Self::Arg(arg)
    }
}

impl<'a> From<&'a Keyword> for ArgOrKeyword<'a> {
    fn from(keyword: &'a Keyword) -> Self {
        Self::Keyword(keyword)
    }
}

impl Ranged for ArgOrKeyword<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Arg(arg) => arg.range(),
            Self::Keyword(keyword) => keyword.range(),
        }
    }
}

impl Arguments {
    /// Return the number of positional and keyword arguments.
    pub fn len(&self) -> usize {
        self.args.len() + self.keywords.len()
    }

    /// Return `true` if there are no positional or keyword arguments.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the [`Keyword`] with the given name, or `None` if no such [`Keyword`] exists.
    pub fn find_keyword(&self, keyword_name: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|keyword| {
            let Keyword { arg, .. } = keyword;
            arg.as_ref().is_some_and(|arg| arg == keyword_name)
        })
    }

    /// Return the positional argument at the given index, or `None` if no such argument exists.
    pub fn find_positional(&self, position: usize) -> Option<&Expr> {
        self.args
            .iter()
            .take_while(|expr| !expr.is_starred_expr())
            .nth(position)
    }

    /// Return the value for the argument with the given name or at the given position, or `None` if no such
    /// argument exists. Used to retrieve argument values that can be provided _either_ as keyword or
    /// positional arguments.
    pub fn find_argument_value(&self, name: &str, position: usize) -> Option<&Expr> {
        self.find_argument(name, position).map(ArgOrKeyword::value)
    }

    /// Return the the argument with the given name or at the given position, or `None` if no such
    /// argument exists. Used to retrieve arguments that can be provided _either_ as keyword or
    /// positional arguments.
    pub fn find_argument(&self, name: &str, position: usize) -> Option<ArgOrKeyword> {
        self.find_keyword(name)
            .map(ArgOrKeyword::from)
            .or_else(|| self.find_positional(position).map(ArgOrKeyword::from))
    }

    /// Return the positional and keyword arguments in the order of declaration.
    ///
    /// Positional arguments are generally before keyword arguments, but star arguments are an
    /// exception:
    /// ```python
    /// class A(*args, a=2, *args2, **kwargs):
    ///     pass
    ///
    /// f(*args, a=2, *args2, **kwargs)
    /// ```
    /// where `*args` and `args2` are `args` while `a=1` and `kwargs` are `keywords`.
    ///
    /// If you would just chain `args` and `keywords` the call would get reordered which we don't
    /// want. This function instead "merge sorts" them into the correct order.
    ///
    /// Note that the order of evaluation is always first `args`, then `keywords`:
    /// ```python
    /// def f(*args, **kwargs):
    ///     pass
    ///
    /// def g(x):
    ///     print(x)
    ///     return x
    ///
    ///
    /// f(*g([1]), a=g(2), *g([3]), **g({"4": 5}))
    /// ```
    /// Output:
    /// ```text
    /// [1]
    /// [3]
    /// 2
    /// {'4': 5}
    /// ```
    pub fn arguments_source_order(&self) -> impl Iterator<Item = ArgOrKeyword<'_>> {
        let args = self.args.iter().map(ArgOrKeyword::Arg);
        let keywords = self.keywords.iter().map(ArgOrKeyword::Keyword);
        args.merge_by(keywords, |left, right| left.start() < right.start())
    }

    pub fn inner_range(&self) -> TextRange {
        TextRange::new(self.l_paren_range().end(), self.r_paren_range().start())
    }

    pub fn l_paren_range(&self) -> TextRange {
        TextRange::at(self.start(), '('.text_len())
    }

    pub fn r_paren_range(&self) -> TextRange {
        TextRange::new(self.end() - ')'.text_len(), self.end())
    }
}

/// An AST node used to represent a sequence of type parameters.
///
/// For example, given:
/// ```python
/// class C[T, U, V]: ...
/// ```
/// The `TypeParams` node would span from the left to right brackets (inclusive), and contain
/// the `T`, `U`, and `V` type parameters in the order they appear in the source code.

#[derive(Clone, Debug, PartialEq)]
pub struct TypeParams {
    pub range: TextRange,
    pub type_params: Vec<TypeParam>,
}

impl Deref for TypeParams {
    type Target = [TypeParam];

    fn deref(&self) -> &Self::Target {
        &self.type_params
    }
}

/// A suite represents a [Vec] of [Stmt].
///
/// See: <https://docs.python.org/3/reference/compound_stmts.html#grammar-token-python-grammar-suite>
pub type Suite = Vec<Stmt>;

/// The kind of escape command as defined in [IPython Syntax] in the IPython codebase.
///
/// [IPython Syntax]: https://github.com/ipython/ipython/blob/635815e8f1ded5b764d66cacc80bbe25e9e2587f/IPython/core/inputtransformer2.py#L335-L343
#[derive(PartialEq, Eq, Debug, Clone, Hash, Copy)]
pub enum IpyEscapeKind {
    /// Send line to underlying system shell (`!`).
    Shell,
    /// Send line to system shell and capture output (`!!`).
    ShCap,
    /// Show help on object (`?`).
    Help,
    /// Show help on object, with extra verbosity (`??`).
    Help2,
    /// Call magic function (`%`).
    Magic,
    /// Call cell magic function (`%%`).
    Magic2,
    /// Call first argument with rest of line as arguments after splitting on whitespace
    /// and quote each as string (`,`).
    Quote,
    /// Call first argument with rest of line as an argument quoted as a single string (`;`).
    Quote2,
    /// Call first argument with rest of line as arguments (`/`).
    Paren,
}

impl TryFrom<char> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: char) -> Result<Self, Self::Error> {
        match ch {
            '!' => Ok(IpyEscapeKind::Shell),
            '?' => Ok(IpyEscapeKind::Help),
            '%' => Ok(IpyEscapeKind::Magic),
            ',' => Ok(IpyEscapeKind::Quote),
            ';' => Ok(IpyEscapeKind::Quote2),
            '/' => Ok(IpyEscapeKind::Paren),
            _ => Err(format!("Unexpected magic escape: {ch}")),
        }
    }
}

impl TryFrom<[char; 2]> for IpyEscapeKind {
    type Error = String;

    fn try_from(ch: [char; 2]) -> Result<Self, Self::Error> {
        match ch {
            ['!', '!'] => Ok(IpyEscapeKind::ShCap),
            ['?', '?'] => Ok(IpyEscapeKind::Help2),
            ['%', '%'] => Ok(IpyEscapeKind::Magic2),
            [c1, c2] => Err(format!("Unexpected magic escape: {c1}{c2}")),
        }
    }
}

impl fmt::Display for IpyEscapeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl IpyEscapeKind {
    /// Returns `true` if the escape kind is help i.e., `?` or `??`.
    pub const fn is_help(self) -> bool {
        matches!(self, IpyEscapeKind::Help | IpyEscapeKind::Help2)
    }

    /// Returns `true` if the escape kind is magic i.e., `%` or `%%`.
    pub const fn is_magic(self) -> bool {
        matches!(self, IpyEscapeKind::Magic | IpyEscapeKind::Magic2)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IpyEscapeKind::Shell => "!",
            IpyEscapeKind::ShCap => "!!",
            IpyEscapeKind::Help => "?",
            IpyEscapeKind::Help2 => "??",
            IpyEscapeKind::Magic => "%",
            IpyEscapeKind::Magic2 => "%%",
            IpyEscapeKind::Quote => ",",
            IpyEscapeKind::Quote2 => ";",
            IpyEscapeKind::Paren => "/",
        }
    }
}

/// An `Identifier` with an empty `id` is invalid.
///
/// For example, in the following code `id` will be empty.
/// ```python
/// def 1():
///     ...
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub id: Name,
    pub range: TextRange,
}

impl Identifier {
    #[inline]
    pub fn new(id: impl Into<Name>, range: TextRange) -> Self {
        Self {
            id: id.into(),
            range,
        }
    }

    pub fn id(&self) -> &Name {
        &self.id
    }

    pub fn is_valid(&self) -> bool {
        !self.id.is_empty()
    }
}

impl Identifier {
    #[inline]
    pub fn as_str(&self) -> &str {
        self.id.as_str()
    }
}

impl PartialEq<str> for Identifier {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.id == other
    }
}

impl PartialEq<String> for Identifier {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.id == other
    }
}

impl std::ops::Deref for Identifier {
    type Target = str;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.id.as_str()
    }
}

impl AsRef<str> for Identifier {
    #[inline]
    fn as_ref(&self) -> &str {
        self.id.as_str()
    }
}

impl std::fmt::Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.id, f)
    }
}

impl From<Identifier> for Name {
    #[inline]
    fn from(identifier: Identifier) -> Name {
        identifier.id
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq)]
pub enum Singleton {
    None,
    True,
    False,
}

impl From<bool> for Singleton {
    fn from(value: bool) -> Self {
        if value {
            Singleton::True
        } else {
            Singleton::False
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generated::*;
    use crate::Mod;

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn size() {
        assert!(std::mem::size_of::<Stmt>() <= 120);
        assert!(std::mem::size_of::<StmtFunctionDef>() <= 120);
        assert!(std::mem::size_of::<StmtClassDef>() <= 104);
        assert!(std::mem::size_of::<StmtTry>() <= 112);
        assert!(std::mem::size_of::<Mod>() <= 32);
        assert!(matches!(std::mem::size_of::<Pattern>(), 88));

        assert_eq!(std::mem::size_of::<Expr>(), 64);
        assert_eq!(std::mem::size_of::<ExprAttribute>(), 56);
        assert_eq!(std::mem::size_of::<ExprAwait>(), 16);
        assert_eq!(std::mem::size_of::<ExprBinOp>(), 32);
        assert_eq!(std::mem::size_of::<ExprBoolOp>(), 40);
        assert_eq!(std::mem::size_of::<ExprBooleanLiteral>(), 12);
        assert_eq!(std::mem::size_of::<ExprBytesLiteral>(), 40);
        assert_eq!(std::mem::size_of::<ExprCall>(), 56);
        assert_eq!(std::mem::size_of::<ExprCompare>(), 48);
        assert_eq!(std::mem::size_of::<ExprDict>(), 32);
        assert_eq!(std::mem::size_of::<ExprDictComp>(), 48);
        assert_eq!(std::mem::size_of::<ExprEllipsisLiteral>(), 8);
        assert!(matches!(std::mem::size_of::<ExprFString>(), 48));
        assert_eq!(std::mem::size_of::<ExprGenerator>(), 48);
        assert_eq!(std::mem::size_of::<ExprIf>(), 32);
        assert_eq!(std::mem::size_of::<ExprIpyEscapeCommand>(), 32);
        assert_eq!(std::mem::size_of::<ExprLambda>(), 24);
        assert_eq!(std::mem::size_of::<ExprList>(), 40);
        assert_eq!(std::mem::size_of::<ExprListComp>(), 40);
        assert_eq!(std::mem::size_of::<ExprName>(), 40);
        assert_eq!(std::mem::size_of::<ExprNamed>(), 24);
        assert_eq!(std::mem::size_of::<ExprNoneLiteral>(), 8);
        assert_eq!(std::mem::size_of::<ExprNumberLiteral>(), 32);
        assert_eq!(std::mem::size_of::<ExprSet>(), 32);
        assert_eq!(std::mem::size_of::<ExprSetComp>(), 40);
        assert_eq!(std::mem::size_of::<ExprSlice>(), 32);
        assert_eq!(std::mem::size_of::<ExprStarred>(), 24);
        assert_eq!(std::mem::size_of::<ExprStringLiteral>(), 56);
        assert_eq!(std::mem::size_of::<ExprSubscript>(), 32);
        assert_eq!(std::mem::size_of::<ExprTuple>(), 40);
        assert_eq!(std::mem::size_of::<ExprUnaryOp>(), 24);
        assert_eq!(std::mem::size_of::<ExprYield>(), 16);
        assert_eq!(std::mem::size_of::<ExprYieldFrom>(), 16);
    }
}
