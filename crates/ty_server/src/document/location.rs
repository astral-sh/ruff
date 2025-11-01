use crate::PositionEncoding;
use crate::document::{FileRangeExt, ToRangeExt};
use lsp_types::Location;
use ruff_db::files::FileRange;
use ty_ide::{NavigationTarget, ReferenceTarget};
use crate::Db;

pub(crate) trait ToLink {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location>;

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink>;
}

impl ToLink for NavigationTarget {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        FileRange::new(self.file(), self.focus_range())
            .as_lsp_range(db, encoding)
            .to_location()
    }

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        let file = self.file();

        // Get target_range and URI together to ensure they're consistent (same cell for notebooks)
        let target_location = self
            .full_range()
            .as_lsp_range(db, file, encoding)
            .to_location()?;
        let target_range = target_location.range;

        // For selection_range, we can use as_local_range since we know it's in the same document/cell
        let selection_range = self
            .focus_range()
            .as_lsp_range(db, file, encoding)
            .to_local_range();

        let src = src.map(|src| src.as_lsp_range(db, encoding).to_local_range());

        Some(lsp_types::LocationLink {
            target_uri: target_location.uri,
            target_range,
            target_selection_range: selection_range,
            origin_selection_range: src,
        })
    }
}

impl ToLink for ReferenceTarget {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        self.file_range().as_lsp_range(db, encoding).to_location()
    }

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        // Get target_range and URI together to ensure they're consistent (same cell for notebooks)
        let target_location = self
            .range()
            .as_lsp_range(db, self.file(), encoding)
            .to_location()?;
        let target_range = target_location.range;
        let selection_range = target_range;

        let src = src.map(|src| src.as_lsp_range(db, encoding).to_local_range());

        Some(lsp_types::LocationLink {
            target_uri: target_location.uri,
            target_range,
            target_selection_range: selection_range,
            origin_selection_range: src,
        })
    }
}
