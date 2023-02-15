use crate::{AstNode, Language};

/// Matches a `SyntaxNode` against an `ast` type.
///
/// # Example:
///
/// ```
/// use ruff_rowan::{match_ast, AstNode};
/// use ruff_rowan::raw_language::{LiteralExpression, RawLanguageRoot, RawLanguageKind, RawSyntaxTreeBuilder};
///
/// let mut builder = RawSyntaxTreeBuilder::new();
/// builder.start_node(RawLanguageKind::ROOT);
/// builder.start_node(RawLanguageKind::LITERAL_EXPRESSION);
/// builder.token(RawLanguageKind::NUMBER_TOKEN, "5");
/// builder.finish_node();
/// builder.finish_node();
///
/// let root = builder.finish();
///
/// let text = match_ast! {
///     match &root {
///         RawLanguageRoot(root) => { format!("root: {}", root.text()) },
///         LiteralExpression(literal) => { format!("literal: {}", literal.text()) },
///         _ => {
///             root.text().to_string()
///         }
///     }
/// };
///
/// assert_eq!(text, "root: 5");
/// ```
#[macro_export]
macro_rules! match_ast {
    // Necessary because expressions aren't allowed in front of `{`
    (match &$node:ident { $($tt:tt)* }) => { match_ast!(match (&$node) { $($tt)* }) };
    (match $node:ident { $($tt:tt)* }) => { match_ast!(match ($node) { $($tt)* }) };

    (match ($node:expr) {
        $( $( $path:ident )::+ ($it:pat) => $res:expr, )*
        _ => $catch_all:expr $(,)?
    }) => {{
        $( if let Some($it) = $($path::)+cast_ref($node) { $res } else )*
        { $catch_all }
    }};
}

/// Declares a custom union AstNode type with an ungram-like syntax
///
/// # Example
///
/// ```ignore
/// declare_node_union! {
///     /// Matches an if statement or a conditional expression
///     pub(crate) JsAnyConditional = JsIfStatement | JsConditionalExpression
/// }
/// ```
#[macro_export]
macro_rules! declare_node_union {
    (@merge_kind $head:ident ) => {
        $head::KIND_SET
    };
    (@merge_kind $head:ident $( $rest:ident )* ) => {
        $head::KIND_SET.union($crate::declare_node_union!( @merge_kind $( $rest )* ))
    };

    ( $( #[$attr:meta] )* $vis:vis $name:ident = $( $variant:ident )|* ) => {
        $( #[$attr] )*
        #[allow(clippy::enum_variant_names)]
        #[derive(Clone, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $variant($variant), )*
        }

        impl $crate::AstNode for $name {
            type Language = <( $( $variant, )* ) as $crate::macros::UnionLanguage>::Language;

            const KIND_SET: $crate::SyntaxKindSet<Self::Language> = $crate::declare_node_union!( @merge_kind $( $variant )* );

            fn can_cast(kind: <Self::Language as $crate::Language>::Kind) -> bool {
                $( $variant::can_cast(kind) )||*
            }

            fn cast(syntax: $crate::SyntaxNode<Self::Language>) -> Option<Self>
            where
                Self: Sized,
            {
                $( if $variant::can_cast(syntax.kind()) {
                    return Some(Self::$variant($variant::unwrap_cast(syntax)));
                } )*

                None
            }

            fn syntax(&self) -> &$crate::SyntaxNode<Self::Language> {
                match self {
                    $( Self::$variant(node) => node.syntax() ),*
                }
            }

            fn into_syntax(self) -> $crate::SyntaxNode<Self::Language> {
                match self {
                    $( Self::$variant(node) => node.into_syntax() ),*
                }
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $( Self::$variant(it) => std::fmt::Debug::fmt(it, f), )*
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt($crate::AstNode::syntax(self), f)
            }
        }

        $( impl From<$variant> for $name {
            fn from(node: $variant) -> Self {
                Self::$variant(node)
            }
        } )*

        impl From<$name> for $crate::SyntaxNode<<$name as $crate::AstNode>::Language> {
            fn from(n: $name) -> $crate::SyntaxNode<<$name as $crate::AstNode>::Language> {
                match n {
                    $( $name::$variant(it) => it.into(), )*
                }
            }
        }

        impl From<$name> for $crate::SyntaxElement<<$name as $crate::AstNode>::Language> {
            fn from(n: $name) -> $crate::SyntaxElement<<$name as $crate::AstNode>::Language> {
                $crate::SyntaxNode::<<$name as $crate::AstNode>::Language>::from(n).into()
            }
        }
    };
}

/// This trait is implemented for tuples of AstNode types of size 1 to 12 if
/// all node types share the same associated language (which is then aliased as
/// the `Language` associated type on [UnionLanguage] itself)
pub trait UnionLanguage {
    type Language: Language;
}

macro_rules! impl_union_language {
    ( $head:ident $( , $rest:ident )* ) => {
        impl<$head $( , $rest )*> UnionLanguage for ($head, $( $rest ),*)
        where
            $head: AstNode $( , $rest: AstNode<Language = <$head as AstNode>::Language> )*
        {
            type Language = <$head as AstNode>::Language;
        }

        impl_union_language!( $( $rest ),* );
    };

    () => {};
}

impl_union_language!(
    T00, T01, T02, T03, T04, T05, T06, T07, T08, T09, T10, T11, T12, T13, T14, T15, T16, T17, T18,
    T19, T20, T21, T22, T23, T24, T25, T26, T27, T28, T29
);
