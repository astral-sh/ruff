use anstyle::{AnsiColor, Effects, Style};

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
    ///
    /// # Note
    /// When testing styled terminal output, see the [`testing-colors` feature](crate#features)
    pub fn styled() -> Self {
        #[cfg(windows)]
        const USE_WINDOWS_COLORS: bool = !cfg!(feature = "testing-colors");
        #[cfg(not(windows))]
        const USE_WINDOWS_COLORS: bool = false;

        let bright_blue = if USE_WINDOWS_COLORS {
            AnsiColor::BrightCyan.on_default()
        } else {
            AnsiColor::BrightBlue.on_default()
        };

        Self {
            error: AnsiColor::BrightRed.on_default().effects(Effects::BOLD),
            warning: if USE_WINDOWS_COLORS {
                AnsiColor::BrightYellow.on_default()
            } else {
                AnsiColor::Yellow.on_default()
            }
            .effects(Effects::BOLD),
            info: bright_blue.effects(Effects::BOLD),
            note: AnsiColor::BrightGreen.on_default().effects(Effects::BOLD),
            help: AnsiColor::BrightCyan.on_default().effects(Effects::BOLD),
            line_no: bright_blue.effects(Effects::BOLD),
            emphasis: if USE_WINDOWS_COLORS {
                AnsiColor::BrightWhite.on_default()
            } else {
                Style::new()
            }
            .effects(Effects::BOLD),
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
