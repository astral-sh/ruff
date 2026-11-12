//! Structural equality and hashing for borrowed Python AST nodes.
//!
//! Two [`ComparableExpr`]s are considered equal if the underlying AST nodes have the
//! same shape, ignoring trivia (e.g., parentheses, comments, and whitespace), the
//! location in the source code, and other contextual information (e.g., whether they
//! represent reads or writes, which is typically encoded in the Python AST).
//!
//! For example, in `[(a, b) for a, b in c]`, the `(a, b)` and `a, b` expressions are
//! considered equal, despite the former being parenthesized, and despite the former
//! being a write ([`ast::ExprContext::Store`]) and the latter being a read
//! ([`ast::ExprContext::Load`]).
//!
//! Similarly, `"a" "b"` and `"ab"` would be considered equal, despite the former being
//! an implicit concatenation of string literals, as these expressions are considered to
//! have the same shape in that they evaluate to the same value.

use crate as ast;
use crate::{Expr, Number};
use std::borrow::Cow;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Use `compare_enum!(Wrapper(AstEnum) { ... })` to compare borrowed AST nodes
/// by their structural fields. Listing the fields once keeps the wrapper's
/// `PartialEq`, `Hash`, and `Debug` implementations in sync. Equality checks
/// the variants before reading either node's fields.
///
/// List every variant of `AstEnum`. For example, `ComparableStmt` uses both
/// forms of entry.
///
/// - `Return => stmt_return_fields` delegates to a function taking
///   `&ast::StmtReturn` and returning `Option<ComparableExpr<'_>>`. Only the
///   return value participates in comparison.
/// - `Pass(ast::StmtPass { range: _, node_index: _ })` compares just the
///   variant. Any two `pass` statements are structurally equal.
///
/// Destructure the node exhaustively in the helper or the inline pattern so
/// that adding an AST field forces a decision about whether to compare it.
macro_rules! compare_enum {
    ($key:ident($ast:ident) {
        $($variant:ident $(($ignored:pat))? $(=> $fields:ident)?,)*
    }) => {
        impl PartialEq for $key<'_> {
            fn eq(&self, other: &Self) -> bool {
                match self.0 {
                    $(
                        $(ast::$ast::$variant(left) => match other.0 {
                            ast::$ast::$variant(right) => $fields(left) == $fields(right),
                            _ => false,
                        },)?
                        $(ast::$ast::$variant($ignored) => matches!(other.0, ast::$ast::$variant(_)),)?
                    )*
                }
            }
        }

        impl Eq for $key<'_> {}

        impl Hash for $key<'_> {
            fn hash<H: Hasher>(&self, state: &mut H) {
                std::mem::discriminant::<ast::$ast>(&self.0).hash(state);
                match self.0 {
                    $(
                        $(ast::$ast::$variant(node) => $fields(node).hash(state),)?
                        $(ast::$ast::$variant($ignored) => {},)?
                    )*
                }
            }
        }

        impl fmt::Debug for $key<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.0 {
                    $(
                        $(ast::$ast::$variant(node) => f
                            .debug_tuple(stringify!($variant))
                            .field(&$fields(node))
                            .finish(),)?
                        $(ast::$ast::$variant($ignored) => f.write_str(stringify!($variant)),)?
                    )*
                }
            }
        }
    };
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableAlias<'a> {
    name: &'a str,
    asname: Option<&'a str>,
}

impl<'a> From<&'a ast::Alias> for ComparableAlias<'a> {
    fn from(alias: &'a ast::Alias) -> Self {
        let ast::Alias {
            name,
            asname,
            range: _,
            node_index: _,
        } = alias;
        Self {
            name: name.as_str(),
            asname: asname.as_deref(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableWithItem<'a> {
    context_expr: ComparableExpr<'a>,
    optional_vars: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::WithItem> for ComparableWithItem<'a> {
    fn from(with_item: &'a ast::WithItem) -> Self {
        let ast::WithItem {
            context_expr,
            optional_vars,
            range: _,
            node_index: _,
        } = with_item;
        Self {
            context_expr: context_expr.into(),
            optional_vars: optional_vars.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparablePatternKeyword<'a> {
    attr: &'a str,
    pattern: ComparablePattern<'a>,
}

impl<'a> From<&'a ast::PatternKeyword> for ComparablePatternKeyword<'a> {
    fn from(keyword: &'a ast::PatternKeyword) -> Self {
        let ast::PatternKeyword {
            attr,
            pattern,
            range: _,
            node_index: _,
        } = keyword;
        Self {
            attr: attr.as_str(),
            pattern: pattern.into(),
        }
    }
}

/// A borrowed structural comparison key for a match pattern.
#[derive(Clone, Copy)]
pub struct ComparablePattern<'a>(&'a ast::Pattern);

impl<'a> From<&'a ast::Pattern> for ComparablePattern<'a> {
    fn from(node: &'a ast::Pattern) -> Self {
        Self(node)
    }
}

compare_enum!(ComparablePattern(Pattern) {
    MatchValue => match_value_fields,
    MatchSingleton => match_singleton_fields,
    MatchSequence => match_sequence_fields,
    MatchMapping => match_mapping_fields,
    MatchClass => match_class_fields,
    MatchStar => match_star_fields,
    MatchAs => match_as_fields,
    MatchOr => match_or_fields,
});

fn match_value_fields(node: &ast::PatternMatchValue) -> ComparableExpr<'_> {
    let ast::PatternMatchValue {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn match_singleton_fields(node: &ast::PatternMatchSingleton) -> ast::Singleton {
    let ast::PatternMatchSingleton {
        value,
        range: _,
        node_index: _,
    } = node;
    *value
}

fn match_sequence_fields(node: &ast::PatternMatchSequence) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchSequence {
        patterns,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(patterns)
}

type ComparableMappingPatternItem<'a> =
    ast::MappingPatternItem<ComparableExpr<'a>, ComparablePattern<'a>, &'a str>;

impl<'a> From<&'a ast::MappingPatternItem> for ComparableMappingPatternItem<'a> {
    fn from(item: &'a ast::MappingPatternItem) -> Self {
        match item {
            ast::MappingPatternItem::Entry { key, pattern } => ast::MappingPatternItem::Entry {
                key: key.into(),
                pattern: pattern.into(),
            },
            ast::MappingPatternItem::Rest(identifier) => {
                ast::MappingPatternItem::Rest(identifier.as_str())
            }
        }
    }
}

fn match_mapping_fields(
    node: &ast::PatternMatchMapping,
) -> ComparableSlice<'_, ast::MappingPatternItem, ComparableMappingPatternItem<'_>> {
    let ast::PatternMatchMapping {
        items,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(items)
}

type ComparablePatternOrKeyword<'a> =
    ast::PatternOrKeyword<ComparablePattern<'a>, ComparablePatternKeyword<'a>>;

impl<'a> From<&'a ast::PatternOrKeyword> for ComparablePatternOrKeyword<'a> {
    fn from(item: &'a ast::PatternOrKeyword) -> Self {
        match item {
            ast::PatternOrKeyword::Pattern(pattern) => Self::Pattern(pattern.into()),
            ast::PatternOrKeyword::Keyword(keyword) => Self::Keyword(keyword.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct MatchClassFields<'a> {
    cls: ComparableExpr<'a>,
    items: ComparableSlice<'a, ast::PatternOrKeyword, ComparablePatternOrKeyword<'a>>,
}

fn match_class_fields(node: &ast::PatternMatchClass) -> MatchClassFields<'_> {
    let ast::PatternMatchClass {
        cls,
        arguments:
            ast::PatternArguments {
                items,
                range: _,
                node_index: _,
            },
        range: _,
        node_index: _,
    } = node;
    MatchClassFields {
        cls: cls.into(),
        items: ComparableSlice::new(items),
    }
}

fn match_star_fields(node: &ast::PatternMatchStar) -> Option<&str> {
    let ast::PatternMatchStar {
        name,
        range: _,
        node_index: _,
    } = node;
    name.as_deref()
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct MatchAsFields<'a> {
    pattern: Option<ComparablePattern<'a>>,
    name: Option<&'a str>,
}

fn match_as_fields(node: &ast::PatternMatchAs) -> MatchAsFields<'_> {
    let ast::PatternMatchAs {
        pattern,
        name,
        range: _,
        node_index: _,
    } = node;
    MatchAsFields {
        pattern: pattern.as_deref().map(Into::into),
        name: name.as_deref(),
    }
}

fn match_or_fields(node: &ast::PatternMatchOr) -> ComparablePatternSlice<'_> {
    let ast::PatternMatchOr {
        patterns,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(patterns)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableMatchCase<'a> {
    pattern: ComparablePattern<'a>,
    guard: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::MatchCase> for ComparableMatchCase<'a> {
    fn from(match_case: &'a ast::MatchCase) -> Self {
        let ast::MatchCase {
            pattern,
            guard,
            body,
            range: _,
            node_index: _,
        } = match_case;
        Self {
            pattern: pattern.into(),
            guard: guard.as_ref().map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

impl<'a> From<&'a ast::Decorator> for ComparableExpr<'a> {
    fn from(decorator: &'a ast::Decorator) -> Self {
        let ast::Decorator {
            expression,
            range: _,
            node_index: _,
        } = decorator;
        expression.into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableText<'a>(&'a str);
impl<'a> From<&'a ast::Identifier> for ComparableText<'a> {
    fn from(identifier: &'a ast::Identifier) -> Self {
        let ast::Identifier {
            id,
            range: _,
            node_index: _,
        } = identifier;
        Self(id.as_str())
    }
}

struct ComparableNumber<'a>(&'a ast::Number);

impl fmt::Debug for ComparableNumber<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl PartialEq for ComparableNumber<'_> {
    fn eq(&self, other: &Self) -> bool {
        match self.0 {
            ast::Number::Int(left) => matches!(other.0, ast::Number::Int(right) if left == right),
            ast::Number::Float(left) => {
                matches!(other.0, ast::Number::Float(right) if left.to_bits() == right.to_bits())
            }
            ast::Number::Complex { real, imag } => match other.0 {
                ast::Number::Complex {
                    real: other_real,
                    imag: other_imag,
                } => {
                    real.to_bits() == other_real.to_bits() && imag.to_bits() == other_imag.to_bits()
                }
                _ => false,
            },
        }
    }
}

impl Eq for ComparableNumber<'_> {}

impl Hash for ComparableNumber<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            ast::Number::Int(value) => value.hash(state),
            ast::Number::Float(value) => value.to_bits().hash(state),
            ast::Number::Complex { real, imag } => (real.to_bits(), imag.to_bits()).hash(state),
        }
    }
}

/// Borrows a sequence and converts each element as it is visited.
struct ComparableSlice<'a, T, C> {
    items: &'a [T],
    comparable: PhantomData<fn() -> C>,
}

impl<'a, T, C> ComparableSlice<'a, T, C> {
    fn new(items: &'a [T]) -> Self {
        Self {
            items,
            comparable: PhantomData,
        }
    }

    fn iter(&self) -> impl Iterator<Item = C>
    where
        C: From<&'a T>,
    {
        self.items.iter().map(C::from)
    }
}

impl<'a, T, C: From<&'a T> + fmt::Debug> fmt::Debug for ComparableSlice<'a, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T, C> Default for ComparableSlice<'_, T, C> {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl<'a, T, C: From<&'a T> + PartialEq> PartialEq for ComparableSlice<'a, T, C> {
    fn eq(&self, other: &Self) -> bool {
        self.items.len() == other.items.len() && self.iter().eq(other.iter())
    }
}

impl<'a, T, C: From<&'a T> + Eq> Eq for ComparableSlice<'a, T, C> {}

impl<'a, T, C: From<&'a T> + Hash> Hash for ComparableSlice<'a, T, C> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.items.len().hash(state);
        for item in self.iter() {
            item.hash(state);
        }
    }
}

type ComparableStmtSlice<'a> = ComparableSlice<'a, ast::Stmt, ComparableStmt<'a>>;
type ComparablePatternSlice<'a> = ComparableSlice<'a, ast::Pattern, ComparablePattern<'a>>;
type ComparableComprehensionSlice<'a> =
    ComparableSlice<'a, ast::Comprehension, ComparableComprehension<'a>>;
type ComparableExprSlice<'a> = ComparableSlice<'a, Expr, ComparableExpr<'a>>;

#[derive(Default)]
struct ComparableArguments<'a>(Option<&'a ast::Arguments>);

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableArgument<'a> {
    Positional(ComparableExpr<'a>),
    Keyword(ComparableKeyword<'a>),
}

impl<'a> From<ast::ArgOrKeyword<'a>> for ComparableArgument<'a> {
    fn from(argument: ast::ArgOrKeyword<'a>) -> Self {
        match argument {
            ast::ArgOrKeyword::Arg(expr) => Self::Positional(expr.into()),
            ast::ArgOrKeyword::Keyword(keyword) => Self::Keyword(keyword.into()),
        }
    }
}

fn grouped_arguments<'a>(
    items: impl Iterator<Item = ast::ArgOrKeyword<'a>> + Clone,
) -> impl Iterator<Item = ComparableArgument<'a>> {
    items
        .clone()
        .filter(|item| matches!(item, ast::ArgOrKeyword::Arg(_)))
        .chain(items.filter(|item| matches!(item, ast::ArgOrKeyword::Keyword(_))))
        .map(ComparableArgument::from)
}

impl fmt::Debug for ComparableArguments<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        if let Some(arguments) = self.0 {
            arguments.for_each_in_evaluation_order(|arg| {
                list.entry(&ComparableArgument::from(arg));
            });
        }
        list.finish()
    }
}

impl PartialEq for ComparableArguments<'_> {
    fn eq(&self, other: &Self) -> bool {
        let (left_args, right_args) = match (self.0, other.0) {
            (Some(left), Some(right)) => (left, right),
            (Some(args), None) | (None, Some(args)) => return args.is_empty(),
            (None, None) => return true,
        };
        if left_args.len() != right_args.len() {
            return false;
        }
        for (index, (left, right)) in left_args
            .iter_source_order()
            .zip(right_args.iter_source_order())
            .enumerate()
        {
            let equal = match (left, right) {
                (ast::ArgOrKeyword::Arg(left), ast::ArgOrKeyword::Arg(right)) => {
                    ComparableExpr::from(left) == ComparableExpr::from(right)
                }
                (ast::ArgOrKeyword::Keyword(left), ast::ArgOrKeyword::Keyword(right)) => {
                    ComparableKeyword::from(left) == ComparableKeyword::from(right)
                }
                // f(key=x, *ys) and f(*ys, key=x) evaluate their arguments in the same order.
                _ => {
                    return grouped_arguments(left_args.iter_source_order().skip(index)).eq(
                        grouped_arguments(right_args.iter_source_order().skip(index)),
                    );
                }
            };
            if !equal {
                return false;
            }
        }
        true
    }
}

impl Eq for ComparableArguments<'_> {}

impl Hash for ComparableArguments<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Some(arguments) = self.0 else {
            0usize.hash(state);
            return;
        };
        arguments.len().hash(state);
        arguments.for_each_in_evaluation_order(|arg| ComparableArgument::from(arg).hash(state));
    }
}

impl<'a> From<&'a ast::Arguments> for ComparableArguments<'a> {
    fn from(arguments: &'a ast::Arguments) -> Self {
        Self(Some(arguments))
    }
}

#[derive(Clone, Copy)]
struct ComparableParameters<'a>(&'a ast::Parameters);

impl fmt::Debug for ComparableParameters<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        ComparableParametersFields::from(self.0).fmt(f)
    }
}

impl PartialEq for ComparableParameters<'_> {
    fn eq(&self, other: &Self) -> bool {
        ComparableParametersFields::from(self.0) == ComparableParametersFields::from(other.0)
    }
}

impl Eq for ComparableParameters<'_> {}

impl Hash for ComparableParameters<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ComparableParametersFields::from(self.0).hash(state);
    }
}

type ComparableParametersFields<'a> =
    ComparableSlice<'a, ast::ParameterItem, ComparableParameterItem<'a>>;

impl<'a> From<&'a ast::Parameters> for ComparableParametersFields<'a> {
    fn from(parameters: &'a ast::Parameters) -> Self {
        let ast::Parameters {
            items,
            range: _,
            node_index: _,
        } = parameters;
        ComparableSlice::new(items)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum ComparableParameterItem<'a> {
    PositionalOnly(ComparableParameterWithDefault<'a>),
    PositionalOrKeyword(ComparableParameterWithDefault<'a>),
    Vararg(ComparableParameter<'a>),
    KeywordOnly(ComparableParameterWithDefault<'a>),
    Kwarg(ComparableParameter<'a>),
}

impl<'a> From<&'a ast::ParameterItem> for ComparableParameterItem<'a> {
    fn from(item: &'a ast::ParameterItem) -> Self {
        match item {
            ast::ParameterItem::PositionalOnly(p) => Self::PositionalOnly(p.into()),
            ast::ParameterItem::PositionalOrKeyword(p) => Self::PositionalOrKeyword(p.into()),
            ast::ParameterItem::Vararg(p) => Self::Vararg(p.into()),
            ast::ParameterItem::KeywordOnly(p) => Self::KeywordOnly(p.into()),
            ast::ParameterItem::Kwarg(p) => Self::Kwarg(p.into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameter<'a> {
    arg: &'a str,
    annotation: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::Parameter> for ComparableParameter<'a> {
    fn from(arg: &'a ast::Parameter) -> Self {
        let ast::Parameter {
            name,
            annotation,
            range: _,
            node_index: _,
        } = arg;
        Self {
            arg: name.as_str(),
            annotation: annotation.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableParameterWithDefault<'a> {
    def: ComparableParameter<'a>,
    default: Option<ComparableExpr<'a>>,
}

impl<'a> From<&'a ast::ParameterWithDefault> for ComparableParameterWithDefault<'a> {
    fn from(arg: &'a ast::ParameterWithDefault) -> Self {
        let ast::ParameterWithDefault {
            parameter,
            default,
            range: _,
            node_index: _,
        } = arg;
        Self {
            def: parameter.into(),
            default: default.as_ref().map(Into::into),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ComparableKeyword<'a> {
    arg: Option<&'a str>,
    value: ComparableExpr<'a>,
}

impl<'a> From<&'a ast::Keyword> for ComparableKeyword<'a> {
    fn from(keyword: &'a ast::Keyword) -> Self {
        let ast::Keyword {
            arg,
            value,
            range: _,
            node_index: _,
        } = keyword;
        Self {
            arg: arg.as_ref().map(ast::Identifier::as_str),
            value: value.as_ref().into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableComprehension<'a> {
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    ifs: ComparableExprSlice<'a>,
    is_async: bool,
}

impl<'a> From<&'a ast::Comprehension> for ComparableComprehension<'a> {
    fn from(comprehension: &'a ast::Comprehension) -> Self {
        let ast::Comprehension {
            target,
            iter,
            ifs,
            is_async,
            range: _,
            node_index: _,
        } = comprehension;
        Self {
            target: target.into(),
            iter: iter.into(),
            ifs: ComparableSlice::new(ifs),
            is_async: *is_async,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableExceptHandler<'a> {
    type_: Option<ComparableExpr<'a>>,
    name: Option<&'a str>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::ExceptHandler> for ComparableExceptHandler<'a> {
    fn from(except_handler: &'a ast::ExceptHandler) -> Self {
        let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_,
            name,
            body,
            range: _,
            node_index: _,
        }) = except_handler;
        Self {
            type_: type_.as_ref().map(Into::into),
            name: name.as_deref(),
            body: ComparableSlice::new(body),
        }
    }
}

/// Compares normalized elements. The enclosing expression distinguishes f-strings from t-strings.
#[derive(Clone, Copy)]
enum ComparableInterpolatedString<'a> {
    FString(&'a ast::FStringValue),
    TString(&'a ast::TStringValue),
}

impl<'a> ComparableInterpolatedString<'a> {
    /// Coalesce adjacent literals so that part boundaries do not affect comparison.
    /// For example, `f"{foo!r} one" " and two " f" and three {bar!s}"` decomposes into
    ///
    /// - An f-string part with the interpolation `{foo!r}` and the literal `" one"`.
    /// - A plain string literal part containing `" and two "`.
    /// - An f-string part with the literal `" and three "` and the interpolation `{bar!s}`.
    ///
    /// All three pieces of literal text form one comparable element between the
    /// two interpolations. Equality compares these elements in order, and hashing
    /// visits the same sequence.
    ///
    /// [CPython's `string.templatelib.Template`] has one more string than interpolation.
    /// Interleaving the two sequences, starting with a string, recovers the reading
    /// order. For example, `t"{foo}bar"` has string slots `("", "bar")`, while
    /// `t"bar{foo}"` has `("bar", "")`. Here the interpolations already delimit
    /// the slots, so we can omit empty t-string text. For f-strings, an explicit
    /// empty literal part still participates in comparison, as in `f"{foo}" ""`.
    ///
    /// [CPython's `string.templatelib.Template`]: https://github.com/python/cpython/blob/c91ad5da9d92eac4718e4da8d53689c3cc24535e/Python/codegen.c#L4052-L4103
    fn normalized_elements(self) -> impl Iterator<Item = ComparableStringElement<'a>> {
        let mut raw = self.raw_elements().peekable();
        std::iter::from_fn(move || {
            loop {
                match raw.next()? {
                    element @ ComparableStringElement::Interpolation(_) => {
                        return Some(element);
                    }
                    ComparableStringElement::Literal(mut text) => {
                        // In `f"{x}a" "b"`, after the interpolation, `raw.next()`
                        // returns `Literal(Cow::Borrowed("a"))` by value. The pattern
                        // moves the `Cow` into the mutable local variable `text`.
                        // Its `&str` points into the AST. `text += next` appends "b" by
                        // replacing the local `Cow` with an owned `String` containing "ab".
                        // In `f"{x}ab"`, "ab" stays borrowed and compares equal by content.
                        while let Some(ComparableStringElement::Literal(next)) =
                            raw.next_if(|next| matches!(next, ComparableStringElement::Literal(_)))
                        {
                            text += next;
                        }
                        if !text.is_empty() || matches!(self, Self::FString(_)) {
                            return Some(ComparableStringElement::Literal(text));
                        }
                    }
                }
            }
        })
    }

    /// Flatten the parts in source order. Each element borrows from the AST,
    /// and adjacent literal pieces are still separate at this point.
    fn raw_elements(self) -> impl Iterator<Item = ComparableStringElement<'a>> {
        enum Parts<'a> {
            FString(ast::FStringParts<'a>),
            TString(std::slice::Iter<'a, ast::TString>),
        }

        let mut parts = match self {
            Self::FString(value) => Parts::FString(value.iter()),
            Self::TString(value) => Parts::TString(value.as_slice().iter()),
        };
        let mut elements = [].iter();
        std::iter::from_fn(move || {
            loop {
                if let Some(element) = elements.next() {
                    return Some(ComparableStringElement::from(element));
                }
                elements = match &mut parts {
                    Parts::FString(parts) => match parts.next()? {
                        ast::FStringPartRef::FString(ast::FString {
                            elements,
                            range: _,
                            node_index: _,
                            flags: _,
                        }) => elements.iter(),
                        ast::FStringPartRef::Literal(part) => {
                            return Some(ComparableStringElement::Literal(Cow::Borrowed(
                                ComparableText::from(part).0,
                            )));
                        }
                    },
                    Parts::TString(parts) => {
                        let ast::TString {
                            elements,
                            range: _,
                            node_index: _,
                            flags: _,
                        } = parts.next()?;
                        elements.iter()
                    }
                };
            }
        })
    }
}

impl PartialEq for ComparableInterpolatedString<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && self.normalized_elements().eq(other.normalized_elements())
    }
}

impl fmt::Debug for ComparableInterpolatedString<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.normalized_elements()).finish()
    }
}

impl Eq for ComparableInterpolatedString<'_> {}

impl Hash for ComparableInterpolatedString<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for element in self.normalized_elements() {
            Some(element).hash(state);
        }
        Option::<ComparableStringElement>::None.hash(state);
    }
}

enum ComparableStringElement<'a> {
    Literal(Cow<'a, str>),
    Interpolation(&'a ast::InterpolatedElement),
}

impl<'a> From<&'a ast::InterpolatedStringElement> for ComparableStringElement<'a> {
    fn from(element: &'a ast::InterpolatedStringElement) -> Self {
        match element {
            ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
                value,
                range: _,
                node_index: _,
            }) => Self::Literal(Cow::Borrowed(value)),
            ast::InterpolatedStringElement::Interpolation(interpolation) => {
                Self::Interpolation(interpolation)
            }
        }
    }
}

impl PartialEq for ComparableStringElement<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(left), Self::Literal(right)) => left == right,
            (Self::Interpolation(left), Self::Interpolation(right)) => {
                interpolation_fields(left) == interpolation_fields(right)
            }
            (Self::Literal(_), Self::Interpolation(_))
            | (Self::Interpolation(_), Self::Literal(_)) => false,
        }
    }
}

impl fmt::Debug for ComparableStringElement<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(text) => text.fmt(f),
            Self::Interpolation(element) => interpolation_fields(element).fmt(f),
        }
    }
}

impl Eq for ComparableStringElement<'_> {}

impl Hash for ComparableStringElement<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Literal(text) => text.hash(state),
            Self::Interpolation(interpolation) => interpolation_fields(interpolation).hash(state),
        }
    }
}

/// The full debug text, including leading and trailing whitespace, appears in
/// the runtime value. If `x` is 1, `f"{x =}"` produces `"x =1"` and `f"{x=}"`
/// produces `"x=1"`. Python normalizes CR and CRLF to LF at compile time, so
/// these newline spellings produce identical text.
fn comparable_debug_text(text: &ast::DebugText) -> Cow<'_, str> {
    normalize_newlines(text.as_str())
}

fn normalize_newlines(contents: &str) -> Cow<'_, str> {
    if contents.contains('\r') {
        Cow::Owned(contents.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(contents)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct InterpolationFields<'a> {
    expression: ComparableExpr<'a>,
    debug_text: Option<Cow<'a, str>>,
    conversion: ast::ConversionFlag,
    format_spec:
        Option<ComparableSlice<'a, ast::InterpolatedStringElement, ComparableStringElement<'a>>>,
}

fn interpolation_fields(node: &ast::InterpolatedElement) -> InterpolationFields<'_> {
    let ast::InterpolatedElement {
        expression,
        debug_text,
        conversion,
        format_spec,
        range: _,
        node_index: _,
    } = node;

    InterpolationFields {
        expression: expression.into(),
        debug_text: debug_text.as_ref().map(comparable_debug_text),
        conversion: *conversion,
        format_spec: format_spec.as_deref().map(
            |ast::InterpolatedStringFormatSpec {
                 elements,
                 range: _,
                 node_index: _,
             }| ComparableSlice::new(elements),
        ),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableElifElseClause<'a> {
    test: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

impl<'a> From<&'a ast::ElifElseClause> for ComparableElifElseClause<'a> {
    fn from(elif_else_clause: &'a ast::ElifElseClause) -> Self {
        let ast::ElifElseClause {
            range: _,
            node_index: _,
            test,
            body,
        } = elif_else_clause;
        Self {
            test: test.as_ref().map(Into::into),
            body: ComparableSlice::new(body),
        }
    }
}

/// A structural key for literal expressions that compares each string or bytes part separately.
#[derive(Clone, Copy)]
pub struct ComparableLiteral<'a>(ast::LiteralExpressionRef<'a>);

impl<'a> From<ast::LiteralExpressionRef<'a>> for ComparableLiteral<'a> {
    fn from(literal: ast::LiteralExpressionRef<'a>) -> Self {
        Self(literal)
    }
}

compare_enum!(ComparableLiteral(LiteralExpressionRef) {
    StringLiteral => string_literal_parts,
    BytesLiteral => bytes_literal_parts,
    NumberLiteral => number_literal_fields,
    BooleanLiteral => boolean_literal_fields,
    NoneLiteral(ast::ExprNoneLiteral { range: _, node_index: _ }),
    EllipsisLiteral(ast::ExprEllipsisLiteral { range: _, node_index: _ }),
});

fn string_literal_parts(
    node: &ast::ExprStringLiteral,
) -> ComparableSlice<'_, ast::StringLiteral, ComparableText<'_>> {
    let ast::ExprStringLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(value.as_slice())
}

fn bytes_literal_parts(
    node: &ast::ExprBytesLiteral,
) -> ComparableSlice<'_, ast::BytesLiteral, ComparableBytesLiteral<'_>> {
    let ast::ExprBytesLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(value.as_slice())
}

impl<'a> From<&'a ast::StringLiteral> for ComparableText<'a> {
    fn from(literal: &'a ast::StringLiteral) -> Self {
        let ast::StringLiteral {
            value,
            range: _,
            node_index: _,
            flags: _,
        } = literal;
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableBytesLiteral<'a>(&'a [u8]);
impl<'a> From<&'a ast::BytesLiteral> for ComparableBytesLiteral<'a> {
    fn from(literal: &'a ast::BytesLiteral) -> Self {
        let ast::BytesLiteral {
            value,
            range: _,
            node_index: _,
            flags: _,
        } = literal;
        Self(value)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComparableDictItem<'a> {
    key: Option<ComparableExpr<'a>>,
    value: ComparableExpr<'a>,
}

impl<'a> From<&'a ast::DictItem> for ComparableDictItem<'a> {
    fn from(
        ast::DictItem {
            key,
            value,
            range: _,
            node_index: _,
        }: &'a ast::DictItem,
    ) -> Self {
        Self {
            key: key.as_ref().map(ComparableExpr::from),
            value: value.into(),
        }
    }
}

/// A borrowed expression key that ignores source locations and expression context.
#[derive(Clone, Copy)]
pub struct ComparableExpr<'a>(ast::ExprRef<'a>);

compare_enum!(ComparableExpr(ExprRef) {
    BoolOp => bool_op_fields,
    Named => named_fields,
    BinOp => bin_op_fields,
    UnaryOp => unary_op_fields,
    Lambda => lambda_fields,
    If => if_fields,
    Dict => dict_fields,
    Set => set_fields,
    ListComp => list_comp_fields,
    SetComp => set_comp_fields,
    DictComp => dict_comp_fields,
    Generator => generator_fields,
    Await => await_fields,
    Yield => yield_fields,
    YieldFrom => yield_from_fields,
    Compare => compare_fields,
    Call => call_fields,
    FString => f_string_fields,
    TString => t_string_fields,
    StringLiteral => string_literal_fields,
    BytesLiteral => bytes_literal_fields,
    NumberLiteral => number_literal_fields,
    BooleanLiteral => boolean_literal_fields,
    NoneLiteral(ast::ExprNoneLiteral { range: _, node_index: _ }),
    EllipsisLiteral(ast::ExprEllipsisLiteral { range: _, node_index: _ }),
    Attribute => attribute_fields,
    Subscript => subscript_fields,
    Starred => starred_fields,
    Name => name_fields,
    List => list_fields,
    Tuple => tuple_fields,
    Slice => slice_fields,
    IpyEscapeCommand => ipy_escape_command_fields,
});

impl<'a> From<&'a ast::Expr> for ComparableExpr<'a> {
    fn from(expr: &'a ast::Expr) -> Self {
        Self(expr.into())
    }
}

impl<'a> From<&'a ast::ExprName> for ComparableExpr<'a> {
    fn from(name: &'a ast::ExprName) -> Self {
        Self(name.into())
    }
}

impl<'a> From<&'a Box<ast::Expr>> for ComparableExpr<'a> {
    fn from(expr: &'a Box<ast::Expr>) -> Self {
        (expr.as_ref()).into()
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct BoolOpFields<'a> {
    op: ast::BoolOp,
    values: ComparableExprSlice<'a>,
}

fn bool_op_fields(node: &ast::ExprBoolOp) -> BoolOpFields<'_> {
    let ast::ExprBoolOp {
        op,
        values,
        range: _,
        node_index: _,
    } = node;
    BoolOpFields {
        op: *op,
        values: ComparableSlice::new(values),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NamedFields<'a> {
    target: ComparableExpr<'a>,
    value: ComparableExpr<'a>,
}

fn named_fields(node: &ast::ExprNamed) -> NamedFields<'_> {
    let ast::ExprNamed {
        target,
        value,
        range: _,
        node_index: _,
    } = node;
    NamedFields {
        target: target.into(),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct BinOpFields<'a> {
    left: ComparableExpr<'a>,
    op: ast::Operator,
    right: ComparableExpr<'a>,
}

fn bin_op_fields(node: &ast::ExprBinOp) -> BinOpFields<'_> {
    let ast::ExprBinOp {
        left,
        op,
        right,
        range: _,
        node_index: _,
    } = node;
    BinOpFields {
        left: left.into(),
        op: *op,
        right: right.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct UnaryOpFields<'a> {
    op: ast::UnaryOp,
    operand: ComparableExpr<'a>,
}

fn unary_op_fields(node: &ast::ExprUnaryOp) -> UnaryOpFields<'_> {
    let ast::ExprUnaryOp {
        op,
        operand,
        range: _,
        node_index: _,
    } = node;
    UnaryOpFields {
        op: *op,
        operand: operand.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct LambdaFields<'a> {
    parameters: Option<ComparableParameters<'a>>,
    body: ComparableExpr<'a>,
}

fn lambda_fields(node: &ast::ExprLambda) -> LambdaFields<'_> {
    let ast::ExprLambda {
        parameters,
        body,
        range: _,
        node_index: _,
    } = node;
    LambdaFields {
        parameters: parameters.as_deref().map(ComparableParameters),
        body: body.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct IfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableExpr<'a>,
    orelse: ComparableExpr<'a>,
}

fn if_fields(node: &ast::ExprIf) -> IfFields<'_> {
    let ast::ExprIf {
        test,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    IfFields {
        test: test.into(),
        body: body.into(),
        orelse: orelse.into(),
    }
}

fn dict_fields(node: &ast::ExprDict) -> ComparableSlice<'_, ast::DictItem, ComparableDictItem<'_>> {
    let ast::ExprDict {
        items,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(items)
}

fn set_fields(node: &ast::ExprSet) -> ComparableExprSlice<'_> {
    let ast::ExprSet {
        elts,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(elts)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct ComprehensionExprFields<'a> {
    elt: ComparableExpr<'a>,
    generators: ComparableComprehensionSlice<'a>,
}

impl<'a> ComprehensionExprFields<'a> {
    fn new(elt: &'a Expr, generators: &'a [ast::Comprehension]) -> Self {
        Self {
            elt: elt.into(),
            generators: ComparableSlice::new(generators),
        }
    }
}

fn list_comp_fields(node: &ast::ExprListComp) -> ComprehensionExprFields<'_> {
    let ast::ExprListComp {
        elt,
        generators,
        range: _,
        node_index: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn set_comp_fields(node: &ast::ExprSetComp) -> ComprehensionExprFields<'_> {
    let ast::ExprSetComp {
        elt,
        generators,
        range: _,
        node_index: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn dict_comp_fields(
    node: &ast::ExprDictComp,
) -> (
    Option<ComparableExpr<'_>>,
    ComparableExpr<'_>,
    ComparableComprehensionSlice<'_>,
) {
    let ast::ExprDictComp {
        key,
        value,
        generators,
        range: _,
        node_index: _,
    } = node;
    (
        key.as_ref().map(Into::into),
        value.into(),
        ComparableSlice::new(generators),
    )
}

fn generator_fields(node: &ast::ExprGenerator) -> ComprehensionExprFields<'_> {
    let ast::ExprGenerator {
        elt,
        generators,
        range: _,
        node_index: _,
        parenthesized: _,
    } = node;
    ComprehensionExprFields::new(elt, generators)
}

fn await_fields(node: &ast::ExprAwait) -> ComparableExpr<'_> {
    let ast::ExprAwait {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn yield_fields(node: &ast::ExprYield) -> Option<ComparableExpr<'_>> {
    let ast::ExprYield {
        value,
        range: _,
        node_index: _,
    } = node;
    value.as_ref().map(Into::into)
}

fn yield_from_fields(node: &ast::ExprYieldFrom) -> ComparableExpr<'_> {
    let ast::ExprYieldFrom {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CompareFields<'a> {
    ops: &'a [ast::CmpOp],
    operands: ComparableExprSlice<'a>,
}

fn compare_fields(node: &ast::ExprCompare) -> CompareFields<'_> {
    let ast::ExprCompare {
        ops,
        operands,
        range: _,
        node_index: _,
    } = node;
    CompareFields {
        ops,
        operands: ComparableSlice::new(operands),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct CallFields<'a> {
    func: ComparableExpr<'a>,
    arguments: ComparableArguments<'a>,
}

fn call_fields(node: &ast::ExprCall) -> CallFields<'_> {
    let ast::ExprCall {
        func,
        arguments,
        range_start: _,
        node_index: _,
    } = node;
    CallFields {
        func: func.into(),
        arguments: arguments.into(),
    }
}

fn f_string_fields(node: &ast::ExprFString) -> ComparableInterpolatedString<'_> {
    let ast::ExprFString {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableInterpolatedString::FString(value)
}

fn t_string_fields(node: &ast::ExprTString) -> ComparableInterpolatedString<'_> {
    let ast::ExprTString {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableInterpolatedString::TString(value)
}

fn string_literal_fields(node: &ast::ExprStringLiteral) -> &str {
    let ast::ExprStringLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    // `"a" "b"` and `"ab"` both yield "ab". The AST caches the joined text
    // for the first spelling and lends the single literal's text for the second.
    value.to_str()
}

fn bytes_literal_fields(node: &ast::ExprBytesLiteral) -> ComparableBytes<'_> {
    let ast::ExprBytesLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableBytes(value)
}

fn number_literal_fields(node: &ast::ExprNumberLiteral) -> ComparableNumber<'_> {
    let ast::ExprNumberLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    ComparableNumber(value)
}

fn boolean_literal_fields(node: &ast::ExprBooleanLiteral) -> bool {
    let ast::ExprBooleanLiteral {
        value,
        range: _,
        node_index: _,
    } = node;
    *value
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct AttributeFields<'a> {
    value: ComparableExpr<'a>,
    attr: &'a str,
}

fn attribute_fields(node: &ast::ExprAttribute) -> AttributeFields<'_> {
    let ast::ExprAttribute {
        value,
        attr,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    AttributeFields {
        value: value.into(),
        attr: attr.as_str(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SubscriptFields<'a> {
    value: ComparableExpr<'a>,
    slice: ComparableExpr<'a>,
}

fn subscript_fields(node: &ast::ExprSubscript) -> SubscriptFields<'_> {
    let ast::ExprSubscript {
        value,
        slice,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    SubscriptFields {
        value: value.into(),
        slice: slice.into(),
    }
}

fn starred_fields(node: &ast::ExprStarred) -> ComparableExpr<'_> {
    let ast::ExprStarred {
        value,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

fn name_fields(node: &ast::ExprName) -> &str {
    let ast::ExprName {
        id,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    id.as_str()
}

fn list_fields(node: &ast::ExprList) -> ComparableExprSlice<'_> {
    let ast::ExprList {
        elts,
        ctx: _,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(elts)
}

fn tuple_fields(node: &ast::ExprTuple) -> ComparableExprSlice<'_> {
    let ast::ExprTuple {
        elts,
        ctx: _,
        range: _,
        node_index: _,
        parenthesized: _,
    } = node;
    ComparableSlice::new(elts)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SliceFields<'a> {
    lower: Option<ComparableExpr<'a>>,
    upper: Option<ComparableExpr<'a>>,
    step: Option<ComparableExpr<'a>>,
}

fn slice_fields(node: &ast::ExprSlice) -> SliceFields<'_> {
    let ast::ExprSlice {
        lower,
        upper,
        step,
        range: _,
        node_index: _,
    } = node;
    SliceFields {
        lower: lower.as_ref().map(Into::into),
        upper: upper.as_ref().map(Into::into),
        step: step.as_ref().map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct IpyEscapeCommandFields<'a> {
    kind: ast::IpyEscapeKind,
    value: &'a str,
}

fn ipy_escape_command_fields(node: &ast::ExprIpyEscapeCommand) -> IpyEscapeCommandFields<'_> {
    let ast::ExprIpyEscapeCommand {
        kind,
        value,
        range: _,
        node_index: _,
    } = node;
    IpyEscapeCommandFields { kind: *kind, value }
}

struct ComparableBytes<'a>(&'a ast::BytesLiteralValue);

impl PartialEq for ComparableBytes<'_> {
    fn eq(&self, other: &Self) -> bool {
        if let ([left], [right]) = (self.0.as_slice(), other.0.as_slice()) {
            return left.as_slice() == right.as_slice();
        }

        // Implicit concatenation makes `b'ab' b'c'` equal to `b'a' b'bc'`.
        // Compare slices up to the next part boundary on either side so we can
        // compare the full byte sequences without allocating a joined buffer.
        // In this example, matching `a` exhausts the first right part. Matching
        // `b` then exhausts the first left part, leaving `c` in `right` for
        // the next left part.
        let mut right_parts = other
            .0
            .iter()
            .map(ast::BytesLiteral::as_slice)
            .filter(|part| !part.is_empty());
        let mut right: &[u8] = &[];
        for part in self.0 {
            let mut left = part.as_slice();
            while !left.is_empty() {
                if right.is_empty() {
                    let Some(next) = right_parts.next() else {
                        return false;
                    };
                    right = next;
                }
                let len = left.len().min(right.len());
                if left[..len] != right[..len] {
                    return false;
                }
                left = &left[len..];
                right = &right[len..];
            }
        }
        // All left bytes matched. Extra right bytes can be in the current
        // slice or in a later part. The iterator skips empty parts.
        right.is_empty() && right_parts.next().is_none()
    }
}

impl fmt::Debug for ComparableBytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for part in self.0 {
            list.entries(part.as_slice());
        }
        list.finish()
    }
}

impl Eq for ComparableBytes<'_> {}

impl Hash for ComparableBytes<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // `b'a' b'b'` and `b'ab'` compare equal, so hash both as the slice `b"ab"`.
        // A hasher can give two one-byte writes a different result from one two-byte write.
        Cow::from(self.0).hash(state);
    }
}

type ComparableTypeParams<'a> = ComparableSlice<'a, ast::TypeParam, ComparableTypeParam<'a>>;
impl<'a> From<&'a ast::TypeParams> for ComparableTypeParams<'a> {
    fn from(params: &'a ast::TypeParams) -> Self {
        let ast::TypeParams {
            type_params,
            range: _,
            node_index: _,
        } = params;
        Self::new(type_params)
    }
}

struct ComparableTypeParam<'a>(&'a ast::TypeParam);

impl<'a> From<&'a ast::TypeParam> for ComparableTypeParam<'a> {
    fn from(node: &'a ast::TypeParam) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableTypeParam(TypeParam) {
    TypeVar => type_var_fields,
    TypeVarTuple => type_var_tuple_fields,
    ParamSpec => param_spec_fields,
});

#[derive(Debug, PartialEq, Eq, Hash)]
struct TypeVarFields<'a> {
    name: &'a str,
    bound: Option<ComparableExpr<'a>>,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_fields(node: &ast::TypeParamTypeVar) -> TypeVarFields<'_> {
    let ast::TypeParamTypeVar {
        name,
        bound,
        default,
        range: _,
        node_index: _,
    } = node;
    TypeVarFields {
        name: name.as_str(),
        bound: bound.as_ref().map(Into::into),
        default: default.as_ref().map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct VariadicTypeParamFields<'a> {
    name: &'a str,
    default: Option<ComparableExpr<'a>>,
}

fn type_var_tuple_fields(node: &ast::TypeParamTypeVarTuple) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamTypeVarTuple {
        name,
        default,
        range: _,
        node_index: _,
    } = node;
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.as_ref().map(Into::into),
    }
}

fn param_spec_fields(node: &ast::TypeParamParamSpec) -> VariadicTypeParamFields<'_> {
    let ast::TypeParamParamSpec {
        name,
        default,
        range: _,
        node_index: _,
    } = node;
    VariadicTypeParamFields {
        name: name.as_str(),
        default: default.as_ref().map(Into::into),
    }
}

/// A borrowed structural comparison key for a statement.
#[derive(Clone, Copy)]
pub struct ComparableStmt<'a>(&'a ast::Stmt);

impl<'a> From<&'a ast::Stmt> for ComparableStmt<'a> {
    fn from(node: &'a ast::Stmt) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableStmt(Stmt) {
    FunctionDef => stmt_function_def_fields,
    ClassDef => stmt_class_def_fields,
    Return => stmt_return_fields,
    Delete => stmt_delete_fields,
    TypeAlias => stmt_type_alias_fields,
    Assign => stmt_assign_fields,
    AugAssign => stmt_aug_assign_fields,
    AnnAssign => stmt_ann_assign_fields,
    For => stmt_for_fields,
    While => stmt_while_fields,
    If => stmt_if_fields,
    With => stmt_with_fields,
    Match => stmt_match_fields,
    Raise => stmt_raise_fields,
    Try => stmt_try_fields,
    Assert => stmt_assert_fields,
    Import => stmt_import_fields,
    ImportFrom => stmt_import_from_fields,
    Global => stmt_global_fields,
    Nonlocal => stmt_nonlocal_fields,
    IpyEscapeCommand => stmt_ipy_escape_command_fields,
    Expr => stmt_expr_fields,
    Pass(ast::StmtPass { range: _, node_index: _ }),
    Break(ast::StmtBreak { range: _, node_index: _ }),
    Continue(ast::StmtContinue { range: _, node_index: _ }),
});

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtFunctionDefFields<'a> {
    is_async: bool,
    decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    parameters: ComparableParameters<'a>,
    returns: Option<ComparableExpr<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_function_def_fields(node: &ast::StmtFunctionDef) -> StmtFunctionDefFields<'_> {
    let ast::StmtFunctionDef {
        is_async,
        name,
        parameters,
        body,
        decorator_list,
        returns,
        type_params,
        range: _,
        node_index: _,
    } = node;
    StmtFunctionDefFields {
        is_async: *is_async,
        name: name.as_str(),
        parameters: ComparableParameters(parameters),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        returns: returns.as_ref().map(Into::into),
        type_params: type_params.as_deref().map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtClassDefFields<'a> {
    decorator_list: ComparableSlice<'a, ast::Decorator, ComparableExpr<'a>>,
    name: &'a str,
    type_params: Option<ComparableTypeParams<'a>>,
    arguments: ComparableArguments<'a>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_class_def_fields(node: &ast::StmtClassDef) -> StmtClassDefFields<'_> {
    let ast::StmtClassDef {
        name,
        arguments,
        body,
        decorator_list,
        type_params,
        range: _,
        node_index: _,
    } = node;
    StmtClassDefFields {
        name: name.as_str(),
        arguments: arguments.as_deref().map(Into::into).unwrap_or_default(),
        body: ComparableSlice::new(body),
        decorator_list: ComparableSlice::new(decorator_list),
        type_params: type_params.as_deref().map(Into::into),
    }
}

fn stmt_return_fields(node: &ast::StmtReturn) -> Option<ComparableExpr<'_>> {
    let ast::StmtReturn {
        value,
        range: _,
        node_index: _,
    } = node;
    value.as_ref().map(Into::into)
}

fn stmt_delete_fields(node: &ast::StmtDelete) -> ComparableExprSlice<'_> {
    let ast::StmtDelete {
        targets,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(targets)
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTypeAliasFields<'a> {
    name: ComparableExpr<'a>,
    type_params: Option<ComparableTypeParams<'a>>,
    value: ComparableExpr<'a>,
}

fn stmt_type_alias_fields(node: &ast::StmtTypeAlias) -> StmtTypeAliasFields<'_> {
    let ast::StmtTypeAlias {
        range: _,
        node_index: _,
        name,
        type_params,
        value,
    } = node;
    StmtTypeAliasFields {
        name: name.into(),
        type_params: type_params.as_deref().map(Into::into),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssignFields<'a> {
    targets: ComparableExprSlice<'a>,
    value: ComparableExpr<'a>,
}

fn stmt_assign_fields(node: &ast::StmtAssign) -> StmtAssignFields<'_> {
    let ast::StmtAssign {
        targets,
        value,
        range: _,
        node_index: _,
    } = node;
    StmtAssignFields {
        targets: ComparableSlice::new(targets),
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAugAssignFields<'a> {
    target: ComparableExpr<'a>,
    op: ast::Operator,
    value: ComparableExpr<'a>,
}

fn stmt_aug_assign_fields(node: &ast::StmtAugAssign) -> StmtAugAssignFields<'_> {
    let ast::StmtAugAssign {
        target,
        op,
        value,
        range: _,
        node_index: _,
    } = node;
    StmtAugAssignFields {
        target: target.into(),
        op: *op,
        value: value.into(),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAnnAssignFields<'a> {
    target: ComparableExpr<'a>,
    annotation: ComparableExpr<'a>,
    value: Option<ComparableExpr<'a>>,
    simple: bool,
}

fn stmt_ann_assign_fields(node: &ast::StmtAnnAssign) -> StmtAnnAssignFields<'_> {
    let ast::StmtAnnAssign {
        target,
        annotation,
        value,
        simple,
        range: _,
        node_index: _,
    } = node;
    StmtAnnAssignFields {
        target: target.into(),
        annotation: annotation.into(),
        value: value.as_ref().map(Into::into),
        simple: *simple,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtForFields<'a> {
    is_async: bool,
    target: ComparableExpr<'a>,
    iter: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_for_fields(node: &ast::StmtFor) -> StmtForFields<'_> {
    let ast::StmtFor {
        is_async,
        target,
        iter,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    StmtForFields {
        is_async: *is_async,
        target: target.into(),
        iter: iter.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWhileFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    orelse: ComparableStmtSlice<'a>,
}

fn stmt_while_fields(node: &ast::StmtWhile) -> StmtWhileFields<'_> {
    let ast::StmtWhile {
        test,
        body,
        orelse,
        range: _,
        node_index: _,
    } = node;
    StmtWhileFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        orelse: ComparableSlice::new(orelse),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtIfFields<'a> {
    test: ComparableExpr<'a>,
    body: ComparableStmtSlice<'a>,
    elif_else_clauses: ComparableSlice<'a, ast::ElifElseClause, ComparableElifElseClause<'a>>,
}

fn stmt_if_fields(node: &ast::StmtIf) -> StmtIfFields<'_> {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
        node_index: _,
    } = node;
    StmtIfFields {
        test: test.into(),
        body: ComparableSlice::new(body),
        elif_else_clauses: ComparableSlice::new(elif_else_clauses),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtWithFields<'a> {
    is_async: bool,
    items: ComparableSlice<'a, ast::WithItem, ComparableWithItem<'a>>,
    body: ComparableStmtSlice<'a>,
}

fn stmt_with_fields(node: &ast::StmtWith) -> StmtWithFields<'_> {
    let ast::StmtWith {
        is_async,
        items,
        body,
        range: _,
        node_index: _,
    } = node;
    StmtWithFields {
        is_async: *is_async,
        items: ComparableSlice::new(items),
        body: ComparableSlice::new(body),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtMatchFields<'a> {
    subject: ComparableExpr<'a>,
    cases: ComparableSlice<'a, ast::MatchCase, ComparableMatchCase<'a>>,
}

fn stmt_match_fields(node: &ast::StmtMatch) -> StmtMatchFields<'_> {
    let ast::StmtMatch {
        subject,
        cases,
        range: _,
        node_index: _,
    } = node;
    StmtMatchFields {
        subject: subject.into(),
        cases: ComparableSlice::new(cases),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtRaiseFields<'a> {
    exc: Option<ComparableExpr<'a>>,
    cause: Option<ComparableExpr<'a>>,
}

fn stmt_raise_fields(node: &ast::StmtRaise) -> StmtRaiseFields<'_> {
    let ast::StmtRaise {
        exc,
        cause,
        range: _,
        node_index: _,
    } = node;
    StmtRaiseFields {
        exc: exc.as_ref().map(Into::into),
        cause: cause.as_ref().map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtTryFields<'a> {
    body: ComparableStmtSlice<'a>,
    handlers: ComparableSlice<'a, ast::ExceptHandler, ComparableExceptHandler<'a>>,
    orelse: ComparableStmtSlice<'a>,
    finalbody: ComparableStmtSlice<'a>,
    is_star: bool,
}

fn stmt_try_fields(node: &ast::StmtTry) -> StmtTryFields<'_> {
    let ast::StmtTry {
        body,
        handlers,
        orelse,
        finalbody,
        is_star,
        range: _,
        node_index: _,
    } = node;
    StmtTryFields {
        body: ComparableSlice::new(body),
        handlers: ComparableSlice::new(handlers),
        orelse: ComparableSlice::new(orelse),
        finalbody: ComparableSlice::new(finalbody),
        is_star: *is_star,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtAssertFields<'a> {
    test: ComparableExpr<'a>,
    msg: Option<ComparableExpr<'a>>,
}

fn stmt_assert_fields(node: &ast::StmtAssert) -> StmtAssertFields<'_> {
    let ast::StmtAssert {
        test,
        msg,
        range: _,
        node_index: _,
    } = node;
    StmtAssertFields {
        test: test.into(),
        msg: msg.as_ref().map(Into::into),
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImportFields<'a> {
    names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
    is_lazy: bool,
}

fn stmt_import_fields(node: &ast::StmtImport) -> StmtImportFields<'_> {
    let ast::StmtImport {
        names,
        is_lazy,
        range: _,
        node_index: _,
    } = node;
    StmtImportFields {
        names: ComparableSlice::new(names),
        is_lazy: *is_lazy,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct StmtImportFromFields<'a> {
    module: Option<&'a str>,
    names: ComparableSlice<'a, ast::Alias, ComparableAlias<'a>>,
    level: u32,
    is_lazy: bool,
}

fn stmt_import_from_fields(node: &ast::StmtImportFrom) -> StmtImportFromFields<'_> {
    let ast::StmtImportFrom {
        module,
        names,
        level,
        is_lazy,
        range: _,
        node_index: _,
    } = node;
    StmtImportFromFields {
        module: module.as_deref(),
        names: ComparableSlice::new(names),
        level: *level,
        is_lazy: *is_lazy,
    }
}

fn stmt_global_fields(
    node: &ast::StmtGlobal,
) -> ComparableSlice<'_, ast::Identifier, ComparableText<'_>> {
    let ast::StmtGlobal {
        names,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(names)
}

fn stmt_nonlocal_fields(
    node: &ast::StmtNonlocal,
) -> ComparableSlice<'_, ast::Identifier, ComparableText<'_>> {
    let ast::StmtNonlocal {
        names,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(names)
}

fn stmt_ipy_escape_command_fields(node: &ast::StmtIpyEscapeCommand) -> IpyEscapeCommandFields<'_> {
    let ast::StmtIpyEscapeCommand {
        kind,
        value,
        range: _,
        node_index: _,
    } = node;
    IpyEscapeCommandFields { kind: *kind, value }
}

fn stmt_expr_fields(node: &ast::StmtExpr) -> ComparableExpr<'_> {
    let ast::StmtExpr {
        value,
        range: _,
        node_index: _,
    } = node;
    value.into()
}

#[derive(Clone, Copy)]
pub struct ComparableMod<'a>(&'a ast::Mod);

impl<'a> From<&'a ast::Mod> for ComparableMod<'a> {
    fn from(node: &'a ast::Mod) -> Self {
        Self(node)
    }
}

compare_enum!(ComparableMod(Mod) {
    Module => mod_module_fields,
    Expression => mod_expression_fields,
});

fn mod_module_fields(node: &ast::ModModule) -> ComparableStmtSlice<'_> {
    let ast::ModModule {
        body,
        range: _,
        node_index: _,
    } = node;
    ComparableSlice::new(body)
}

fn mod_expression_fields(node: &ast::ModExpression) -> ComparableExpr<'_> {
    let ast::ModExpression {
        body,
        range: _,
        node_index: _,
    } = node;
    body.into()
}

/// Wrapper around [`Expr`] that implements [`Hash`] and [`PartialEq`] according to Python
/// semantics:
///
/// > Values that compare equal (such as 1, 1.0, and True) can be used interchangeably to index the
/// > same dictionary entry.
///
/// For example, considers `True`, `1`, and `1.0` to be equal, as they hash to the same value
/// in Python, along with `False`, `0`, and `0.0`.
///
/// See: <https://docs.python.org/3/library/stdtypes.html#mapping-types-dict>
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct HashableExpr<'a>(HashableExprKind<'a>);

#[derive(Debug, PartialEq, Eq, Hash)]
enum HashableExprKind<'a> {
    Comparable(ComparableExpr<'a>),
    Number(HashableNumber),
    NamedExpr {
        target: ComparableExpr<'a>,
        value: Box<HashableExprKind<'a>>,
    },
    Tuple(Vec<HashableExprKind<'a>>),
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct HashableNumber {
    real: HashableReal,
    imag: HashableReal,
}

impl HashableNumber {
    fn real(real: HashableReal) -> Self {
        Self {
            real,
            imag: HashableReal::Integer(0),
        }
    }

    fn complex(real: HashableReal, imag: HashableReal) -> Self {
        Self { real, imag }
    }

    fn negate(mut self) -> Self {
        self.real.negate();
        self.imag.negate();
        self
    }

    fn into_real(self) -> Option<HashableReal> {
        self.imag.is_zero().then_some(self.real)
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum HashableReal {
    Integer(i128),
    Float(u64),
}

impl HashableReal {
    fn from_int(value: &ast::Int) -> Option<Self> {
        value.as_u64().map(i128::from).map(Self::Integer)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::float_cmp,
        reason = "the round-trip check guarantees that the float is exactly representable as an integer"
    )]
    fn from_float(value: f64) -> Self {
        if value.is_finite() && value.abs() < U64_EXCLUSIVE_UPPER_BOUND {
            let integer = value as i128;
            if integer as f64 == value {
                return Self::Integer(integer);
            }
        }
        Self::Float(value.to_bits())
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "Python converts real components to floats before complex arithmetic"
    )]
    fn into_float(self) -> Self {
        match self {
            Self::Integer(integer) => Self::from_float(integer as f64),
            Self::Float(_) => self,
        }
    }

    fn is_zero(&self) -> bool {
        matches!(self, Self::Integer(0))
    }

    fn negate(&mut self) {
        match self {
            Self::Integer(integer) => *integer = -*integer,
            Self::Float(bits) => *bits ^= 1 << 63,
        }
    }
}

// `2^64`, the exclusive upper bound for values representable as a `u64`.
const U64_EXCLUSIVE_UPPER_BOUND: f64 = 18_446_744_073_709_551_616.0;

impl<'a> From<&'a Expr> for HashableExpr<'a> {
    fn from(expr: &'a Expr) -> Self {
        /// Returns a version of the given expression that can be hashed and compared according to
        /// Python  semantics.
        fn as_hashable(expr: &Expr) -> HashableExprKind<'_> {
            if let Some(constant) = as_hashable_constant(expr) {
                return constant;
            }

            match expr {
                Expr::Named(named) => HashableExprKind::NamedExpr {
                    target: ComparableExpr::from(&named.target),
                    value: Box::new(as_hashable(&named.value)),
                },
                _ => HashableExprKind::Comparable(ComparableExpr::from(expr)),
            }
        }

        /// Returns a hashable representation if the expression's value is statically known.
        fn as_hashable_constant(expr: &Expr) -> Option<HashableExprKind<'_>> {
            if let Some(number) = as_number(expr) {
                return Some(HashableExprKind::Number(number));
            }

            let kind = match expr {
                Expr::Tuple(tuple) => HashableExprKind::Tuple(
                    tuple
                        .iter()
                        .map(as_hashable_constant)
                        .collect::<Option<_>>()?,
                ),
                _ if expr.is_literal_expr() => {
                    HashableExprKind::Comparable(ComparableExpr::from(expr))
                }
                _ => return None,
            };

            Some(kind)
        }

        fn as_number(expr: &Expr) -> Option<HashableNumber> {
            match expr {
                Expr::BooleanLiteral(boolean) => Some(HashableNumber::real(HashableReal::Integer(
                    i128::from(u8::from(boolean.value)),
                ))),
                Expr::NumberLiteral(number) => match &number.value {
                    Number::Int(int) => HashableReal::from_int(int).map(HashableNumber::real),
                    Number::Float(float) => {
                        Some(HashableNumber::real(HashableReal::from_float(*float)))
                    }
                    Number::Complex { real, imag } => Some(HashableNumber::complex(
                        HashableReal::from_float(*real),
                        HashableReal::from_float(*imag),
                    )),
                },
                Expr::UnaryOp(ast::ExprUnaryOp { op, operand, .. }) => match op {
                    ast::UnaryOp::UAdd => as_number(operand),
                    ast::UnaryOp::USub => as_number(operand).map(HashableNumber::negate),
                    ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
                },
                Expr::BinOp(ast::ExprBinOp {
                    left,
                    op: op @ (ast::Operator::Add | ast::Operator::Sub),
                    right,
                    ..
                }) => {
                    let real = as_number(left)?.into_real()?.into_float();
                    let Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value:
                            Number::Complex {
                                real: complex_real,
                                imag,
                            },
                        ..
                    }) = right.as_ref()
                    else {
                        return None;
                    };
                    let complex_real = HashableReal::from_float(*complex_real);
                    if !complex_real.is_zero() {
                        return None;
                    }
                    let mut imag = HashableReal::from_float(*imag);
                    if op.is_sub() {
                        imag.negate();
                    }
                    Some(HashableNumber::complex(real, imag))
                }
                _ => None,
            }
        }

        Self(as_hashable(expr))
    }
}
