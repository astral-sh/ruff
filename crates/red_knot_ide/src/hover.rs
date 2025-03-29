use crate::goto::find_goto_target;
use crate::{Db, MarkupKind, RangedValue};
use red_knot_python_semantic::types::Type;
use red_knot_python_semantic::SemanticModel;
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_text_size::{Ranged, TextSize};
use std::fmt;
use std::fmt::Write as _;

pub fn hover(db: &dyn Db, file: File, offset: TextSize) -> Option<RangedValue<Hover>> {
    let parsed = parsed_module(db.upcast(), file);
    let goto_target = find_goto_target(parsed, offset)?;

    let model = SemanticModel::new(db.upcast(), file);
    let ty = goto_target.inferred_type(&model)?;

    tracing::debug!(
        "Inferred type of covering node is {}",
        ty.display(db.upcast())
    );

    // TODO: Add documentation of the symbol (not the type's definition).
    // TODO: Render the symbol's signature instead of just its type.
    let contents = vec![HoverContent::Type(ty)];

    Some(RangedValue {
        range: FileRange::new(file, goto_target.range()),
        value: Hover { contents },
    })
}

pub struct Hover<'db> {
    contents: Vec<HoverContent<'db>>,
}

impl<'db> Hover<'db> {
    /// Renders the hover to a string using the specified markup kind.
    pub fn render(&self, db: &dyn Db, kind: MarkupKind) -> String {
        let mut output = String::new();

        for content in &self.contents {
            if !output.is_empty() {
                output.push('\n');
            }

            write!(
                &mut output,
                "{content}",
                content = content.display(db, kind)
            )
            .unwrap();
        }

        output
    }

    fn iter(&self) -> std::slice::Iter<'_, HoverContent<'db>> {
        self.contents.iter()
    }
}

impl<'db> IntoIterator for Hover<'db> {
    type Item = HoverContent<'db>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.contents.into_iter()
    }
}

impl<'a, 'db> IntoIterator for &'a Hover<'db> {
    type Item = &'a HoverContent<'db>;
    type IntoIter = std::slice::Iter<'a, HoverContent<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HoverContent<'db> {
    Type(Type<'db>),
}

impl<'db> HoverContent<'db> {
    fn display(&self, db: &'db dyn Db, kind: MarkupKind) -> DisplayHoverContent<'_, 'db> {
        DisplayHoverContent {
            db,
            content: self,
            kind,
        }
    }
}

pub(crate) struct DisplayHoverContent<'a, 'db> {
    db: &'db dyn Db,
    content: &'a HoverContent<'db>,
    kind: MarkupKind,
}

impl fmt::Display for DisplayHoverContent<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.content {
            HoverContent::Type(ty) => self
                .kind
                .fenced_code_block(ty.display(self.db.upcast()), "python")
                .fmt(f),
        }
    }
}
