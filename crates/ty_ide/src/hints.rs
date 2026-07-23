use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use ty_python_semantic::types::ide_support::{
    UnreachableKind, unreachable_ranges, unused_bindings, unused_imports,
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
    UnusedImport(Name),
    UnreachableCode(UnreachableKind),
}

impl HintKind {
    pub fn message(&self) -> String {
        match self {
            Self::UnusedBinding(name) => format!("`{name}` is unused"),
            Self::UnusedImport(name) => format!("Import `{name}` is unused"),
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
    if !db.should_check_file(file) {
        return Vec::new();
    }

    let unreachable = unreachable_ranges(db, file);
    let is_inside_unreachable = |range: TextRange| {
        unreachable
            .iter()
            .any(|unreachable| unreachable.range.contains_range(range))
    };

    let mut hints = unused_bindings(db, file)
        .iter()
        // Avoid narrower unused-binding/import hints inside code already reported as unreachable.
        .filter(|binding| !is_inside_unreachable(binding.range))
        .map(|binding| Hint {
            range: binding.range,
            kind: HintKind::UnusedBinding(binding.name.clone()),
        })
        .collect::<Vec<_>>();

    hints.extend(
        unused_imports(db, file)
            .iter()
            .filter(|import| !is_inside_unreachable(import.range))
            .map(|import| Hint {
                range: import.range,
                kind: HintKind::UnusedImport(import.name.clone()),
            }),
    );

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
