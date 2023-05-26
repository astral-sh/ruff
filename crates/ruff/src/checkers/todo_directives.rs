use std::str::FromStr;

use ruff_text_size::{TextLen, TextSize};

pub(crate) enum TodoDirective {
    Todo,
    Fixme,
    Xxx,
    Hack,
}

impl FromStr for TodoDirective {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fixme" => Ok(TodoDirective::Fixme),
            "hack" => Ok(TodoDirective::Hack),
            "todo" => Ok(TodoDirective::Todo),
            "xxx" => Ok(TodoDirective::Xxx),
            _ => Err(()),
        }
    }
}

impl TodoDirective {
    /// Extract a [`TodoDirective`] from a comment.
    ///
    /// Returns the offset of the directive within the comment, and the matching directive tag.
    pub(crate) fn from_comment(comment: &str) -> Option<(TodoDirective, TextSize)> {
        let mut subset_opt = Some(comment);
        let mut total_offset = TextSize::new(0);

        // Loop over the comment to catch cases like `# foo # TODO`.
        while let Some(subset) = subset_opt {
            let trimmed = subset.trim_start_matches('#').trim_start().to_lowercase();

            let offset = subset.text_len() - trimmed.text_len();
            total_offset += offset;

            // TODO: rework this
            let directive = if trimmed.starts_with("fixme") {
                Some((TodoDirective::Fixme, total_offset))
            } else if trimmed.starts_with("xxx") {
                Some((TodoDirective::Xxx, total_offset))
            } else if trimmed.starts_with("todo") {
                Some((TodoDirective::Todo, total_offset))
            } else if trimmed.starts_with("hack") {
                Some((TodoDirective::Hack, total_offset))
            } else {
                None
            };

            if directive.is_some() {
                return directive;
            }

            // Shrink the subset to check for the next phrase starting with "#".
            subset_opt = if let Some(new_offset) = trimmed.find('#') {
                total_offset += TextSize::try_from(new_offset).unwrap();
                subset.get(total_offset.to_usize()..)
            } else {
                None
            };
        }

        None
    }

    /// Returns the length of the directive tag.
    pub(crate) fn len(&self) -> TextSize {
        match self {
            TodoDirective::Fixme => TextSize::new(5),
            TodoDirective::Todo | TodoDirective::Hack => TextSize::new(4),
            TodoDirective::Xxx => TextSize::new(3),
        }
    }
}
