use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use ty_python_semantic::types::ide_support::{
    UnreachableKind, unreachable_ranges, unused_bindings,
};

use crate::Db;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Hint {
    pub range: TextRange,
    pub kind: HintKind,
}

impl Hint {
    pub fn message(&self) -> String {
        self.kind.message()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum HintKind {
    UnusedBinding(Name),
    UnreachableCode(UnreachableKind),
}

impl HintKind {
    pub fn message(&self) -> String {
        match self {
            Self::UnusedBinding(name) => format!("`{name}` is unused"),
            Self::UnreachableCode(UnreachableKind::Unconditional) => {
                "Code is always unreachable".to_owned()
            }
            Self::UnreachableCode(UnreachableKind::CurrentAnalysis) => {
                "Code is unreachable\nThis may depend on your current environment and settings"
                    .to_owned()
            }
        }
    }
}

pub fn hints(db: &dyn Db, file: File) -> Vec<Hint> {
    if !db.project().should_check_file(db, file) {
        return Vec::new();
    }

    let unreachable = unreachable_ranges(db, file);

    let mut hints = unused_bindings(db, file)
        .iter()
        // Avoid a narrower unused-binding hint inside code that is already reported as unreachable.
        .filter(|binding| {
            unreachable.is_empty()
                || !unreachable
                    .iter()
                    .any(|range| range.range.contains_range(binding.range))
        })
        .map(|binding| Hint {
            range: binding.range,
            kind: HintKind::UnusedBinding(binding.name.clone()),
        })
        .collect::<Vec<_>>();

    hints.extend(unreachable.iter().map(|range| Hint {
        range: range.range,
        kind: HintKind::UnreachableCode(range.kind),
    }));

    hints.sort_unstable_by(|left, right| {
        (left.range.start(), left.range.end(), &left.kind).cmp(&(
            right.range.start(),
            right.range.end(),
            &right.kind,
        ))
    });

    hints
}
