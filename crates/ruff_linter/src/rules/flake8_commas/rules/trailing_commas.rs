use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

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

impl AlwaysFixableViolation for MissingTrailingComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing comma missing")
    }

    fn fix_title(&self) -> String {
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

impl AlwaysFixableViolation for ProhibitedTrailingComma {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Trailing comma prohibited")
    }

    fn fix_title(&self) -> String {
        "Remove trailing comma".to_string()
    }
}

/// COM812, COM818, COM819
pub(crate) fn trailing_commas(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &[LexResult],
    locator: &Locator,
    indexer: &Indexer,
) {
    let mut fstrings = 0u32;
    let tokens = tokens.iter().filter_map(|result| {
        let Ok((tok, tok_range)) = result else {
            return None;
        };

        match tok {
            // Completely ignore comments -- they just interfere with the logic.
            Tok::Comment(_) => None,
            // F-strings are handled as `String` token type with the complete range
            // of the outermost f-string. This means that the expression inside the
            // f-string is not checked for trailing commas.
            Tok::FStringStart => {
                fstrings = fstrings.saturating_add(1);
                None
            }
            Tok::FStringEnd => {
                fstrings = fstrings.saturating_sub(1);
                if fstrings == 0 {
                    indexer
                        .fstring_ranges()
                        .outermost(tok_range.start())
                        .map(|range| Token::new(TokenType::String, range))
                } else {
                    None
                }
            }
            _ => {
                if fstrings == 0 {
                    Some(Token::from((tok, *tok_range)))
                } else {
                    None
                }
            }
        }
    });

    let mut prev = Token::irrelevant();
    let mut prev_prev = Token::irrelevant();

    let mut stack = vec![Context::new(ContextType::No)];

    for token in tokens {
        if prev.ty == TokenType::NonLogicalNewline && token.ty == TokenType::NonLogicalNewline {
            // Collapse consecutive newlines to the first one -- trailing commas are
            // added before the first newline.
            continue;
        }

        // Update the comma context stack.
        let context = update_context(token, prev, prev_prev, &mut stack);

        if let Some(diagnostic) = check_token(token, prev, prev_prev, context, locator) {
            diagnostics.push(diagnostic);
        }

        // Pop the current context if the current token ended it.
        // The top context is never popped (if unbalanced closing brackets).
        let pop_context = match context.ty {
            // Lambda terminated by `:`.
            ContextType::LambdaParameters => token.ty == TokenType::Colon,
            // All others terminated by a closing bracket.
            // flake8-commas doesn't verify that it matches the opening...
            _ => token.ty == TokenType::ClosingBracket,
        };
        if pop_context && stack.len() > 1 {
            stack.pop();
        }

        prev_prev = prev;
        prev = token;
    }
}

fn check_token(
    token: Token,
    prev: Token,
    prev_prev: Token,
    context: Context,
    locator: &Locator,
) -> Option<Diagnostic> {
    // Is it allowed to have a trailing comma before this closing bracket (`)`, `]`, or `}).
    let comma_allowed = token.ty == TokenType::ClosingBracket
        && match context.ty {
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
    let comma_prohibited = prev.ty == TokenType::Comma && {
        // Is `(1,)` or `x[1,]`?
        let is_singleton_tuplish =
            matches!(context.ty, ContextType::Subscript | ContextType::Tuple)
                && context.num_commas <= 1;

        // There was no non-logical newline, so prohibit (except in `(1,)` or `x[1,]`).
        let is_flat_and_not_singleton_tuplish = comma_allowed && !is_singleton_tuplish;

        // Lambdas not handled by comma_allowed so handle it specially.
        let is_lambda_parameters =
            context.ty == ContextType::LambdaParameters && token.ty == TokenType::Colon;

        is_flat_and_not_singleton_tuplish || is_lambda_parameters
    };

    if comma_prohibited {
        let mut diagnostic = Diagnostic::new(ProhibitedTrailingComma, prev.range());
        diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(diagnostic.range())));
        return Some(diagnostic);
    }

    // Is prev a prohibited trailing comma on a bare tuple?
    // Approximation: any comma followed by a statement-ending newline.
    let bare_comma_prohibited = prev.ty == TokenType::Comma && token.ty == TokenType::Newline;
    if bare_comma_prohibited {
        return Some(Diagnostic::new(TrailingCommaOnBareTuple, prev.range()));
    }

    if !comma_allowed {
        return None;
    }

    let is_empty = matches!(
        prev_prev.ty,
        TokenType::Comma
            | TokenType::OpeningBracket
            | TokenType::OpeningSquareBracket
            | TokenType::OpeningCurlyBracket
    );

    // Comma is required if:
    // - positioned at a closing parentheses and it is allowed,
    // - Followed by a newline,
    // - Not already present,
    // - Not on an empty (), {}, [].
    let comma_required = prev.ty == TokenType::NonLogicalNewline
        && !is_empty
        && (!context.is_singleline() || context.num_commas == 0);

    if comma_required {
        let range = prev_prev.range();

        let mut diagnostic = Diagnostic::new(MissingTrailingComma, TextRange::empty(range.end()));
        // Create a replacement that includes the final bracket (or other token),
        // rather than just inserting a comma at the end. This prevents the UP034 fix
        // removing any brackets in the same linter pass - doing both at the same time could
        // lead to a syntax error.
        let contents = locator.slice(range);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            format!("{contents},"),
            range,
        )));
        Some(diagnostic)
    } else {
        None
    }
}

fn update_context(
    token: Token,
    prev: Token,
    prev_prev: Token,
    stack: &mut Vec<Context>,
) -> Context {
    let new_context = match token.ty {
        TokenType::OpeningBracket => match (prev.ty, prev_prev.ty) {
            (TokenType::Named, TokenType::Def) => Context::new(ContextType::FunctionParameters),
            (TokenType::Named | TokenType::ClosingBracket, _) => {
                Context::new(ContextType::CallArguments)
            }
            _ => Context::new(ContextType::Tuple),
        },
        TokenType::OpeningSquareBracket => match prev.ty {
            TokenType::ClosingBracket | TokenType::Named | TokenType::String => {
                Context::new(ContextType::Subscript)
            }
            _ => Context::new(ContextType::List),
        },
        TokenType::OpeningCurlyBracket => Context::new(ContextType::Dict),
        TokenType::Lambda => Context::new(ContextType::LambdaParameters),
        TokenType::For => {
            let last = stack.last_mut().expect("Stack to never be empty");
            *last = Context::new(ContextType::No);
            return *last;
        }
        TokenType::Comma => {
            let last = stack.last_mut().expect("Stack to never be empty");
            last.inc();
            return *last;
        }
        TokenType::NonLogicalNewline => {
            let last = stack.last_mut().expect("Stack to never be empty");
            if matches!(
                prev.ty,
                TokenType::OpeningBracket
                    | TokenType::OpeningSquareBracket
                    | TokenType::OpeningCurlyBracket
            ) {
                last.layout = Layout::NewlineAfterOpenBracket;
            } else {
                last.add_newline();
            }

            return *last;
        }
        _ => return stack.last().copied().expect("Stack to never be empty"),
    };

    stack.push(new_context);
    new_context
}

/// Simplified token type.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum TokenType {
    Named,
    String,
    Newline,
    NonLogicalNewline,
    OpeningBracket,
    ClosingBracket,
    OpeningSquareBracket,
    Colon,
    Comma,
    OpeningCurlyBracket,
    Def,
    For,
    Lambda,
    Irrelevant,
}

/// Simplified token specialized for the task.
#[derive(Copy, Clone, Debug)]
struct Token {
    ty: TokenType,
    range: TextRange,
}

impl Ranged for Token {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl Token {
    fn new(ty: TokenType, range: TextRange) -> Self {
        Self { ty, range }
    }

    fn irrelevant() -> Token {
        Token {
            ty: TokenType::Irrelevant,
            range: TextRange::default(),
        }
    }
}

impl From<(&Tok, TextRange)> for Token {
    fn from((tok, range): (&Tok, TextRange)) -> Self {
        let ty = match tok {
            Tok::Name { .. } => TokenType::Named,
            Tok::String { .. } => TokenType::String,
            Tok::Newline => TokenType::Newline,
            Tok::NonLogicalNewline => TokenType::NonLogicalNewline,
            Tok::Lpar => TokenType::OpeningBracket,
            Tok::Rpar => TokenType::ClosingBracket,
            Tok::Lsqb => TokenType::OpeningSquareBracket,
            Tok::Rsqb => TokenType::ClosingBracket,
            Tok::Colon => TokenType::Colon,
            Tok::Comma => TokenType::Comma,
            Tok::Lbrace => TokenType::OpeningCurlyBracket,
            Tok::Rbrace => TokenType::ClosingBracket,
            Tok::Def => TokenType::Def,
            Tok::For => TokenType::For,
            Tok::Lambda => TokenType::Lambda,
            // Import treated like a function.
            Tok::Import => TokenType::Named,
            _ => TokenType::Irrelevant,
        };
        #[allow(clippy::inconsistent_struct_constructor)]
        Self { range, ty }
    }
}

/// Comma context type - types of comma-delimited Python constructs.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
#[derive(Copy, Clone, Debug)]
struct Context {
    ty: ContextType,
    num_commas: u32,
    layout: Layout,
}

impl Context {
    const fn new(ty: ContextType) -> Self {
        Self {
            ty,
            num_commas: 0,
            layout: Layout::Flat,
        }
    }

    fn inc(&mut self) {
        self.num_commas += 1;
    }

    fn add_newline(&mut self) {
        self.layout = match self.layout {
            // Note: Not necessarily correct yet
            Layout::Flat => Layout::Multiline,
            Layout::NewlineAfterOpenBracket => Layout::Singleline,
            Layout::Singleline => Layout::Multiline,
            Layout::Multiline => Layout::Multiline,
        }
    }

    const fn is_singleline(self) -> bool {
        matches!(self.layout, Layout::Singleline)
    }
}

#[derive(Copy, Clone, Debug)]
enum Layout {
    /// The opening bracket, the content between, and the closing bracket are all "flat" on the same line.
    ///
    /// ```python
    /// def test(a, b, c):
    ///     ...
    /// ```
    Flat,

    /// There's a newline after the opening bracket. But there are no other line breaks.
    /// ```python
    /// def test(
    ///     a, b, c):
    ///     ...
    /// ```
    NewlineAfterOpenBracket,

    /// There's a line break after the opening bracket and before the closing bracket but the content is on a single line.
    ///
    /// ```python
    /// def test(
    ///     a, b, c
    /// ):
    ///     ...
    /// ```
    Singleline,

    /// There's at least one line break that splits the parenthesized content over multiple lines.
    ///
    /// ```python
    /// def test(
    ///     a,
    ///     b
    /// ):
    ///     ...
    /// ```
    /// But also
    ///
    /// ```python
    /// def test(a,
    ///         b):
    ///     ...
    /// ```
    Multiline,
}
