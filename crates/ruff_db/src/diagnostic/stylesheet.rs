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
        Self {
            error: AnsiColor::BrightRed.on_default().effects(Effects::BOLD),
            warning: AnsiColor::Yellow.on_default().effects(Effects::BOLD),
            info: bright_blue.effects(Effects::BOLD),
            note: AnsiColor::BrightGreen.on_default().effects(Effects::BOLD),
            help: AnsiColor::BrightCyan.on_default().effects(Effects::BOLD),
            line_no: bright_blue.effects(Effects::BOLD),
            emphasis: Style::new().effects(Effects::BOLD),
            none: Style::new(),
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
        }
    }
}
