//! The renderer for [`Message`]s
//!
//! # Example
//! ```
//! use ruff_annotate_snippets::{Renderer, Snippet, Level};
//! let snippet = Level::Error.title("mismatched types")
//!     .snippet(Snippet::source("Foo").line_start(51).origin("src/format.rs"))
//!     .snippet(Snippet::source("Faa").line_start(129).origin("src/display.rs"));
//!
//!  let renderer = Renderer::styled();
//!  println!("{}", renderer.render(snippet));
//! ```

mod display_list;
mod margin;
mod styled_buffer;
pub(crate) mod stylesheet;

use crate::snippet::Message;
pub use anstyle::*;
use display_list::DisplayList;
use margin::Margin;
use std::fmt::Display;
use stylesheet::Stylesheet;

pub const DEFAULT_TERM_WIDTH: usize = 140;

/// A renderer for [`Message`]s
#[derive(Clone, Debug)]
pub struct Renderer {
    anonymized_line_numbers: bool,
    term_width: usize,
    stylesheet: Stylesheet,
    cut_indicator: &'static str,
}

impl Renderer {
    /// No terminal styling
    pub const fn plain() -> Self {
        Self {
            anonymized_line_numbers: false,
            term_width: DEFAULT_TERM_WIDTH,
            stylesheet: Stylesheet::plain(),
            cut_indicator: "...",
        }
    }

    /// Default terminal styling
    ///
    /// # Note
    /// When testing styled terminal output, see the [`testing-colors` feature](crate#features)
    pub const fn styled() -> Self {
        const USE_WINDOWS_COLORS: bool = cfg!(windows) && !cfg!(feature = "testing-colors");
        const BRIGHT_BLUE: Style = if USE_WINDOWS_COLORS {
            AnsiColor::BrightCyan.on_default()
        } else {
            AnsiColor::BrightBlue.on_default()
        };
        Self {
            stylesheet: Stylesheet {
                error: AnsiColor::BrightRed.on_default().effects(Effects::BOLD),
                warning: if USE_WINDOWS_COLORS {
                    AnsiColor::BrightYellow.on_default()
                } else {
                    AnsiColor::Yellow.on_default()
                }
                .effects(Effects::BOLD),
                info: BRIGHT_BLUE.effects(Effects::BOLD),
                note: AnsiColor::BrightGreen.on_default().effects(Effects::BOLD),
                help: AnsiColor::BrightCyan.on_default().effects(Effects::BOLD),
                line_no: BRIGHT_BLUE.effects(Effects::BOLD),
                emphasis: if USE_WINDOWS_COLORS {
                    AnsiColor::BrightWhite.on_default()
                } else {
                    Style::new()
                }
                .effects(Effects::BOLD),
                none: Style::new(),
            },
            ..Self::plain()
        }
    }

    /// Anonymize line numbers
    ///
    /// This enables (or disables) line number anonymization. When enabled, line numbers are replaced
    /// with `LL`.
    ///
    /// # Example
    ///
    /// ```text
    ///   --> $DIR/whitespace-trimming.rs:4:193
    ///    |
    /// LL | ...                   let _: () = 42;
    ///    |                                   ^^ expected (), found integer
    ///    |
    /// ```
    pub const fn anonymized_line_numbers(mut self, anonymized_line_numbers: bool) -> Self {
        self.anonymized_line_numbers = anonymized_line_numbers;
        self
    }

    /// Set the terminal width
    pub const fn term_width(mut self, term_width: usize) -> Self {
        self.term_width = term_width;
        self
    }

    /// Set the output style for `error`
    pub const fn error(mut self, style: Style) -> Self {
        self.stylesheet.error = style;
        self
    }

    /// Set the output style for `warning`
    pub const fn warning(mut self, style: Style) -> Self {
        self.stylesheet.warning = style;
        self
    }

    /// Set the output style for `info`
    pub const fn info(mut self, style: Style) -> Self {
        self.stylesheet.info = style;
        self
    }

    /// Set the output style for `note`
    pub const fn note(mut self, style: Style) -> Self {
        self.stylesheet.note = style;
        self
    }

    /// Set the output style for `help`
    pub const fn help(mut self, style: Style) -> Self {
        self.stylesheet.help = style;
        self
    }

    /// Set the output style for line numbers
    pub const fn line_no(mut self, style: Style) -> Self {
        self.stylesheet.line_no = style;
        self
    }

    /// Set the output style for emphasis
    pub const fn emphasis(mut self, style: Style) -> Self {
        self.stylesheet.emphasis = style;
        self
    }

    /// Set the output style for none
    pub const fn none(mut self, style: Style) -> Self {
        self.stylesheet.none = style;
        self
    }

    /// Set the string used for when a long line is cut.
    ///
    /// The default is `...` (three `U+002E` characters).
    pub const fn cut_indicator(mut self, string: &'static str) -> Self {
        self.cut_indicator = string;
        self
    }

    /// Render a snippet into a `Display`able object
    pub fn render<'a>(&'a self, msg: Message<'a>) -> impl Display + 'a {
        DisplayList::new(
            msg,
            &self.stylesheet,
            self.anonymized_line_numbers,
            self.term_width,
            self.cut_indicator,
        )
    }
}
