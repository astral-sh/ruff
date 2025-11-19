use anstyle::{AnsiColor, Effects, Style};
use std::fmt::Formatter;

pub(super) const fn fmt_styled<'a, T>(
    content: T,
    style: anstyle::Style,
) -> impl std::fmt::Display + 'a
where
    T: std::fmt::Display + 'a,
{
    struct FmtStyled<T> {
        content: T,
        style: anstyle::Style,
    }

    impl<T> std::fmt::Display for FmtStyled<T>
    where
        T: std::fmt::Display,
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{style_start}{content}{style_end}",
                style_start = self.style.render(),
                content = self.content,
                style_end = self.style.render_reset()
            )
        }
    }

    FmtStyled { content, style }
}

pub(super) fn fmt_with_hyperlink<'a, T>(
    content: T,
    url: Option<&'a str>,
    stylesheet: &DiagnosticStylesheet,
) -> impl std::fmt::Display + 'a
where
    T: std::fmt::Display + 'a,
{
    struct FmtHyperlink<'a, T> {
        content: T,
        url: Option<&'a str>,
    }

    impl<T> std::fmt::Display for FmtHyperlink<'_, T>
    where
        T: std::fmt::Display,
    {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            if let Some(url) = self.url {
                write!(f, "\x1B]8;;{url}\x1B\\")?;
            }

            self.content.fmt(f)?;

            if self.url.is_some() {
                f.write_str("\x1B]8;;\x1B\\")?;
            }

            Ok(())
        }
    }

    let url = if stylesheet.hyperlink { url } else { None };

    FmtHyperlink { content, url }
}

#[derive(Clone, Debug)]
pub struct DiagnosticStylesheet {
    pub(crate) error: Style,
    pub(crate) warning: Style,
    pub(crate) info: Style,
    pub(crate) note: Style,
    pub(crate) help: Style,
    pub(crate) line_no: Style,
    pub(crate) emphasis: Style,
    pub(crate) none: Style,
    pub(crate) separator: Style,
    pub(crate) secondary_code: Style,
    pub(crate) insertion: Style,
    pub(crate) deletion: Style,
    pub(crate) insertion_line_no: Style,
    pub(crate) deletion_line_no: Style,
    pub(crate) hyperlink: bool,
}

impl Default for DiagnosticStylesheet {
    fn default() -> Self {
        Self::plain()
    }
}

impl DiagnosticStylesheet {
    /// Default terminal styling
    pub fn styled() -> Self {
        let bright_blue = AnsiColor::BrightBlue.on_default();

        let hyperlink = supports_hyperlinks::supports_hyperlinks();
        Self {
            error: AnsiColor::BrightRed.on_default().effects(Effects::BOLD),
            warning: AnsiColor::Yellow.on_default().effects(Effects::BOLD),
            info: bright_blue.effects(Effects::BOLD),
            note: AnsiColor::BrightGreen.on_default().effects(Effects::BOLD),
            help: AnsiColor::BrightCyan.on_default().effects(Effects::BOLD),
            line_no: bright_blue.effects(Effects::BOLD),
            emphasis: Style::new().effects(Effects::BOLD),
            none: Style::new(),
            separator: AnsiColor::Cyan.on_default(),
            secondary_code: AnsiColor::Red.on_default().effects(Effects::BOLD),
            insertion: AnsiColor::Green.on_default(),
            deletion: AnsiColor::Red.on_default(),
            insertion_line_no: AnsiColor::Green.on_default().effects(Effects::BOLD),
            deletion_line_no: AnsiColor::Red.on_default().effects(Effects::BOLD),
            hyperlink,
        }
    }

    pub fn plain() -> Self {
        Self {
            error: Style::new(),
            warning: Style::new(),
            info: Style::new(),
            note: Style::new(),
            help: Style::new(),
            line_no: Style::new(),
            emphasis: Style::new(),
            none: Style::new(),
            separator: Style::new(),
            secondary_code: Style::new(),
            insertion: Style::new(),
            deletion: Style::new(),
            insertion_line_no: Style::new(),
            deletion_line_no: Style::new(),
            hyperlink: false,
        }
    }
}
