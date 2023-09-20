use itertools::Itertools;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::lexer::{LexResult, Spanned};
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::registry::Rule;
use crate::settings::Settings;

/// Simplified token type.
#[derive(Copy, Clone, PartialEq, Eq)]
enum TokenType {
    Irrelevant,
    NonLogicalNewline,
    Newline,
    Comma,
    OpeningBracket,
    OpeningSquareBracket,
    OpeningCurlyBracket,
    ClosingBracket,
    For,
    Named,
    Def,
    Lambda,
    Colon,
    String,
}

/// Simplified token specialized for the task.
#[derive(Copy, Clone)]
struct Token<'tok> {
    type_: TokenType,
    // Underlying token.
    spanned: Option<&'tok Spanned>,
}

impl<'tok> Token<'tok> {
    const fn irrelevant() -> Token<'static> {
        Token {
            type_: TokenType::Irrelevant,
            spanned: None,
        }
    }

    const fn from_spanned(spanned: &'tok Spanned) -> Token<'tok> {
        let type_ = match &spanned.0 {
            Tok::NonLogicalNewline => TokenType::NonLogicalNewline,
            Tok::Newline => TokenType::Newline,
            Tok::For => TokenType::For,
            Tok::Def => TokenType::Def,
            Tok::Lambda => TokenType::Lambda,
            // Import treated like a function.
            Tok::Import => TokenType::Named,
            Tok::Name { .. } => TokenType::Named,
            Tok::String { .. } => TokenType::String,
            Tok::Comma => TokenType::Comma,
            Tok::Lpar => TokenType::OpeningBracket,
            Tok::Lsqb => TokenType::OpeningSquareBracket,
            Tok::Lbrace => TokenType::OpeningCurlyBracket,
            Tok::Rpar | Tok::Rsqb | Tok::Rbrace => TokenType::ClosingBracket,
            Tok::Colon => TokenType::Colon,
            _ => TokenType::Irrelevant,
        };
        Self {
            spanned: Some(spanned),
            type_,
        }
    }
}

/// Comma context type - types of comma-delimited Python constructs.
#[derive(Copy, Clone, PartialEq, Eq)]
enum ContextType {
    No,
    /// Function definition parameter list, e.g. `def foo(a,b,c)`.
    FunctionParameters,
    /// Call argument-like item list, e.g. `f(1,2,3)`, `foo()(1,2,3)`.
    CallArguments,
    /// Tuple-like item list, e.g. `(1,2,3)`.
    Tuple,
    /// Subscript item list, e.g. `x[1,2,3]`, `foo()[1,2,3]`.
    Subscript,
    /// List-like item list, e.g. `[1,2,3]`.
    List,
    /// Dict-/set-like item list, e.g. `{1,2,3}`.
    Dict,
    /// Lambda parameter list, e.g. `lambda a, b`.
    LambdaParameters,
}

/// Comma context - described a comma-delimited "situation".
#[derive(Copy, Clone)]
struct Context {
    type_: ContextType,
    num_commas: u32,
}

impl Context {
    const fn new(type_: ContextType) -> Self {
        Self {
            type_,
            num_commas: 0,
        }
    }

    fn inc(&mut self) {
        self.num_commas += 1;
    }
}

/// ## What it does
/// Checks for the absence of trailing commas.
///
/// ## Why is this bad?
/// The presence of a trailing comma can reduce diff size when parameters or
/// elements are added or removed from function calls, function definitions,
/// literals, etc.
///
/// ## Example
/// ```python
/// foo = {
///     "bar": 1,
///     "baz": 2
/// }
/// ```
///
/// Use instead:
/// ```python
/// foo = {
///     "bar": 1,
///     "baz": 2,
/// }
/// ```
#[violation]
pub struct MissingTrailingComma;

impl AlwaysAutofixableViolation for MissingTrailingComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing comma missing")
    }

    fn autofix_title(&self) -> String {
        "Add trailing comma".to_string()
    }
}

/// ## What it does
/// Checks for the presence of trailing commas on bare (i.e., unparenthesized)
/// tuples.
///
/// ## Why is this bad?
/// The presence of a misplaced comma will cause Python to interpret the value
/// as a tuple, which can lead to unexpected behaviour.
///
/// ## Example
/// ```python
/// import json
///
///
/// foo = json.dumps({"bar": 1}),
/// ```
///
/// Use instead:
/// ```python
/// import json
///
///
/// foo = json.dumps({"bar": 1})
/// ```
///
/// In the event that a tuple is intended, then use instead:
/// ```python
/// import json
///
///
/// foo = (json.dumps({"bar": 1}),)
/// ```
#[violation]
pub struct TrailingCommaOnBareTuple;

impl Violation for TrailingCommaOnBareTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing comma on bare tuple prohibited")
    }
}

/// ## What it does
/// Checks for the presence of prohibited trailing commas.
///
/// ## Why is this bad?
/// Trailing commas are not essential in some cases and can therefore be viewed
/// as unnecessary.
///
/// ## Example
/// ```python
/// foo = (1, 2, 3,)
/// ```
///
/// Use instead:
/// ```python
/// foo = (1, 2, 3)
/// ```
#[violation]
pub struct ProhibitedTrailingComma;

impl AlwaysAutofixableViolation for ProhibitedTrailingComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing comma prohibited")
    }

    fn autofix_title(&self) -> String {
        "Remove trailing comma".to_string()
    }
}

/// COM812, COM818, COM819
pub(crate) fn trailing_commas(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &[LexResult],
    locator: &Locator,
    settings: &Settings,
) {
    let tokens = tokens
        .iter()
        .flatten()
        // Completely ignore comments -- they just interfere with the logic.
        .filter(|&r| !matches!(r, (Tok::Comment(_), _)))
        .map(Token::from_spanned);
    let tokens = [Token::irrelevant(), Token::irrelevant()]
        .into_iter()
        .chain(tokens);
    // Collapse consecutive newlines to the first one -- trailing commas are
    // added before the first newline.
    let tokens = tokens.coalesce(|previous, current| {
        if previous.type_ == TokenType::NonLogicalNewline
            && current.type_ == TokenType::NonLogicalNewline
        {
            Ok(previous)
        } else {
            Err((previous, current))
        }
    });

    // The current nesting of the comma contexts.
    let mut stack = vec![Context::new(ContextType::No)];

    for (prev_prev, prev, token) in tokens.tuple_windows() {
        // Update the comma context stack.
        match token.type_ {
            TokenType::OpeningBracket => match (prev.type_, prev_prev.type_) {
                (TokenType::Named, TokenType::Def) => {
                    stack.push(Context::new(ContextType::FunctionParameters));
                }
                (TokenType::Named | TokenType::ClosingBracket, _) => {
                    stack.push(Context::new(ContextType::CallArguments));
                }
                _ => {
                    stack.push(Context::new(ContextType::Tuple));
                }
            },
            TokenType::OpeningSquareBracket => match prev.type_ {
                TokenType::ClosingBracket | TokenType::Named | TokenType::String => {
                    stack.push(Context::new(ContextType::Subscript));
                }
                _ => {
                    stack.push(Context::new(ContextType::List));
                }
            },
            TokenType::OpeningCurlyBracket => {
                stack.push(Context::new(ContextType::Dict));
            }
            TokenType::Lambda => {
                stack.push(Context::new(ContextType::LambdaParameters));
            }
            TokenType::For => {
                let len = stack.len();
                stack[len - 1] = Context::new(ContextType::No);
            }
            TokenType::Comma => {
                let len = stack.len();
                stack[len - 1].inc();
            }
            _ => {}
        }
        let context = &stack[stack.len() - 1];

        // Is it allowed to have a trailing comma before this token?
        let comma_allowed = token.type_ == TokenType::ClosingBracket
            && match context.type_ {
                ContextType::No => false,
                ContextType::FunctionParameters => true,
                ContextType::CallArguments => true,
                // `(1)` is not equivalent to `(1,)`.
                ContextType::Tuple => context.num_commas != 0,
                // `x[1]` is not equivalent to `x[1,]`.
                ContextType::Subscript => context.num_commas != 0,
                ContextType::List => true,
                ContextType::Dict => true,
                // Lambdas are required to be a single line, trailing comma never makes sense.
                ContextType::LambdaParameters => false,
            };

        // Is prev a prohibited trailing comma?
        let comma_prohibited = prev.type_ == TokenType::Comma && {
            // Is `(1,)` or `x[1,]`?
            let is_singleton_tuplish =
                matches!(context.type_, ContextType::Subscript | ContextType::Tuple)
                    && context.num_commas <= 1;
            // There was no non-logical newline, so prohibit (except in `(1,)` or `x[1,]`).
            if comma_allowed && !is_singleton_tuplish {
                true
            // Lambdas not handled by comma_allowed so handle it specially.
            } else {
                context.type_ == ContextType::LambdaParameters && token.type_ == TokenType::Colon
            }
        };
        if comma_prohibited {
            let comma = prev.spanned.unwrap();
            let mut diagnostic = Diagnostic::new(ProhibitedTrailingComma, comma.1);
            if settings.rules.should_fix(Rule::ProhibitedTrailingComma) {
                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(diagnostic.range())));
            }
            diagnostics.push(diagnostic);
        }

        // Is prev a prohibited trailing comma on a bare tuple?
        // Approximation: any comma followed by a statement-ending newline.
        let bare_comma_prohibited =
            prev.type_ == TokenType::Comma && token.type_ == TokenType::Newline;
        if bare_comma_prohibited {
            let comma = prev.spanned.unwrap();
            diagnostics.push(Diagnostic::new(TrailingCommaOnBareTuple, comma.1));
        }

        // Comma is required if:
        // - It is allowed,
        // - Followed by a newline,
        // - Not already present,
        // - Not on an empty (), {}, [].
        let comma_required = comma_allowed
            && prev.type_ == TokenType::NonLogicalNewline
            && !matches!(
                prev_prev.type_,
                TokenType::Comma
                    | TokenType::OpeningBracket
                    | TokenType::OpeningSquareBracket
                    | TokenType::OpeningCurlyBracket
            );
        if comma_required {
            let missing_comma = prev_prev.spanned.unwrap();
            let mut diagnostic = Diagnostic::new(
                MissingTrailingComma,
                TextRange::empty(missing_comma.1.end()),
            );
            if settings.rules.should_fix(Rule::MissingTrailingComma) {
                // Create a replacement that includes the final bracket (or other token),
                // rather than just inserting a comma at the end. This prevents the UP034 autofix
                // removing any brackets in the same linter pass - doing both at the same time could
                // lead to a syntax error.
                let contents = locator.slice(missing_comma.1);
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    format!("{contents},"),
                    missing_comma.1,
                )));
            }
            diagnostics.push(diagnostic);
        }

        // Pop the current context if the current token ended it.
        // The top context is never popped (if unbalanced closing brackets).
        let pop_context = match context.type_ {
            // Lambda terminated by `:`.
            ContextType::LambdaParameters => token.type_ == TokenType::Colon,
            // All others terminated by a closing bracket.
            // flake8-commas doesn't verify that it matches the opening...
            _ => token.type_ == TokenType::ClosingBracket,
        };
        if pop_context && stack.len() > 1 {
            stack.pop();
        }
    }
}
