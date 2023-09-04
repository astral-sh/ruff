use bitflags::bitflags;

use ruff_text_size::TextSize;

bitflags! {
    #[derive(Debug)]
    pub(crate) struct FStringContextFlags: u32 {
        /// The current f-string is a triple-quoted f-string i.e., the number of
        /// opening quotes is 3. If this flag is not set, the number of opening
        /// quotes is 1.
        const TRIPLE = 1 << 0;

        /// The current f-string is a double-quoted f-string. If this flag is not
        /// set, the current f-string is a single-quoted f-string.
        const DOUBLE = 1 << 1;

        /// The current f-string is a raw f-string i.e., prefixed with `r`/`R`.
        /// If this flag is not set, the current f-string is a normal f-string.
        const RAW = 1 << 2;
    }
}

/// The context representing the current f-string that the lexer is in.
#[derive(Debug)]
pub(crate) struct FStringContext {
    flags: FStringContextFlags,

    /// The number of open parentheses for the current f-string. This includes all
    /// three types of parentheses: round (`(`), square (`[`), and curly (`{`).
    open_parentheses_count: u32,

    /// The current depth of format spec for the current f-string. This is because
    /// there can be multiple format specs nested for the same f-string.
    /// For example, `{a:{b:{c}}}` has 3 format specs.
    format_spec_depth: u32,
}

impl FStringContext {
    pub(crate) fn new(flags: FStringContextFlags) -> Self {
        Self {
            flags,
            open_parentheses_count: 0,
            format_spec_depth: 0,
        }
    }

    /// Returns the quote character for the current f-string.
    pub(crate) fn quote_char(&self) -> char {
        if self.flags.contains(FStringContextFlags::DOUBLE) {
            '"'
        } else {
            '\''
        }
    }

    /// Returns the number of quotes for the current f-string.
    pub(crate) fn quote_size(&self) -> TextSize {
        if self.is_triple_quoted() {
            TextSize::from(3)
        } else {
            TextSize::from(1)
        }
    }

    /// Returns the triple quotes for the current f-string if it is a triple-quoted
    /// f-string, `None` otherwise.
    pub(crate) fn triple_quotes(&self) -> Option<&'static str> {
        if self.is_triple_quoted() {
            if self.flags.contains(FStringContextFlags::DOUBLE) {
                Some(r#"""""#)
            } else {
                Some("'''")
            }
        } else {
            None
        }
    }

    /// Returns `true` if the current f-string is a raw f-string.
    pub(crate) fn is_raw_string(&self) -> bool {
        self.flags.contains(FStringContextFlags::RAW)
    }

    /// Returns `true` if the current f-string is a triple-quoted f-string.
    pub(crate) fn is_triple_quoted(&self) -> bool {
        self.flags.contains(FStringContextFlags::TRIPLE)
    }

    /// Returns `true` if the current f-string has open parentheses.
    pub(crate) fn has_open_parentheses(&mut self) -> bool {
        self.open_parentheses_count > 0
    }

    /// Increments the number of parentheses for the current f-string.
    pub(crate) fn increment_opening_parentheses(&mut self) {
        self.open_parentheses_count += 1;
    }

    /// Decrements the number of parentheses for the current f-string. If the
    /// lexer is in a format spec, also decrements the format spec depth.
    pub(crate) fn decrement_closing_parentheses(&mut self) {
        self.try_end_format_spec();
        self.open_parentheses_count = self.open_parentheses_count.saturating_sub(1);
    }

    /// Returns `true` if the lexer is in a f-string expression i.e., between
    /// two curly braces.
    pub(crate) fn is_in_expression(&self) -> bool {
        self.open_parentheses_count > self.format_spec_depth
    }

    /// Returns `true` if the lexer is in a f-string format spec i.e., after a colon.
    pub(crate) fn is_in_format_spec(&self) -> bool {
        self.format_spec_depth > 0 && !self.is_in_expression()
    }

    /// Returns `true` if the context is in a valid position to start format spec
    /// i.e., at the same level of nesting as the opening parentheses token.
    /// Increments the format spec depth if it is.
    ///
    /// This assumes that the current character for the lexer is a colon (`:`).
    pub(crate) fn try_start_format_spec(&mut self) -> bool {
        if self
            .open_parentheses_count
            .saturating_sub(self.format_spec_depth)
            == 1
        {
            self.format_spec_depth += 1;
            true
        } else {
            false
        }
    }

    /// Decrements the format spec depth if the lexer is in a format spec.
    pub(crate) fn try_end_format_spec(&mut self) {
        if self.is_in_format_spec() {
            self.format_spec_depth = self.format_spec_depth.saturating_sub(1);
        }
    }
}

/// The f-strings stack is used to keep track of all the f-strings that the
/// lexer encounters. This is necessary because f-strings can be nested.
#[derive(Debug, Default)]
pub(crate) struct FStrings {
    stack: Vec<FStringContext>,
}

impl FStrings {
    pub(crate) fn push(&mut self, context: FStringContext) {
        self.stack.push(context);
    }

    pub(crate) fn pop(&mut self) -> Option<FStringContext> {
        self.stack.pop()
    }

    pub(crate) fn current(&self) -> Option<&FStringContext> {
        self.stack.last()
    }

    pub(crate) fn current_mut(&mut self) -> Option<&mut FStringContext> {
        self.stack.last_mut()
    }
}
