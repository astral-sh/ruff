use salsa;

use ruff_db::{files::File, parsed::comment_ranges, source::source_text};
use ruff_index::{newtype_index, IndexVec};

use crate::{lint::LintId, Db};

#[salsa::tracked(return_ref)]
pub(crate) fn suppressions(db: &dyn Db, file: File) -> IndexVec<SuppressionIndex, Suppression> {
    let comments = comment_ranges(db.upcast(), file);
    let source = source_text(db.upcast(), file);

    let mut suppressions = IndexVec::default();

    for range in comments {
        let text = &source[range];

        if text.starts_with("# type: ignore") {
            suppressions.push(Suppression {
                target: None,
                kind: SuppressionKind::TypeIgnore,
            });
        } else if text.starts_with("# knot: ignore") {
            suppressions.push(Suppression {
                target: None,
                kind: SuppressionKind::KnotIgnore,
            });
        }
    }

    suppressions
}

#[newtype_index]
pub(crate) struct SuppressionIndex;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct Suppression {
    target: Option<LintId>,
    kind: SuppressionKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum SuppressionKind {
    /// A `type: ignore` comment
    TypeIgnore,

    /// A `knot: ignore` comment
    KnotIgnore,
}
