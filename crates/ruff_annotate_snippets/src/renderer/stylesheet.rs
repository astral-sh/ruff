use anstyle::Style;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Stylesheet {
    pub(crate) error: Style,
    pub(crate) warning: Style,
    pub(crate) info: Style,
    pub(crate) note: Style,
    pub(crate) help: Style,
    pub(crate) line_no: Style,
    pub(crate) emphasis: Style,
    pub(crate) none: Style,
}

impl Default for Stylesheet {
    fn default() -> Self {
        Self::plain()
    }
}

impl Stylesheet {
    pub(crate) const fn plain() -> Self {
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

impl Stylesheet {
    pub(crate) fn error(&self) -> &Style {
        &self.error
    }

    pub(crate) fn warning(&self) -> &Style {
        &self.warning
    }

    pub(crate) fn info(&self) -> &Style {
        &self.info
    }

    pub(crate) fn note(&self) -> &Style {
        &self.note
    }

    pub(crate) fn help(&self) -> &Style {
        &self.help
    }

    pub(crate) fn line_no(&self) -> &Style {
        &self.line_no
    }

    pub(crate) fn emphasis(&self) -> &Style {
        &self.emphasis
    }

    pub(crate) fn none(&self) -> &Style {
        &self.none
    }
}
