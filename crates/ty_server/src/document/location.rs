use crate::PositionEncoding;
use crate::document::{FileRangeExt, ToRangeExt};
use crate::system::file_to_url;
use lsp_types::Location;
use ruff_db::files::FileRange;
use ruff_db::source::{line_index, source_text};
use ruff_text_size::Ranged;
use ty_ide::{NavigationTarget, ReferenceTarget};
use ty_project::Db;

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
        FileRange::new(self.file(), self.focus_range()).to_location(db, encoding)
    }

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        let file = self.file();
        let uri = file_to_url(db, file)?;
        let source = source_text(db, file);
        let index = line_index(db, file);

        let target_range = self.full_range().to_lsp_range(&source, &index, encoding);
        let selection_range = self.focus_range().to_lsp_range(&source, &index, encoding);

        let src = src.map(|src| {
            let source = source_text(db, src.file());
            let index = line_index(db, src.file());

            src.range().to_lsp_range(&source, &index, encoding)
        });

        Some(lsp_types::LocationLink {
            target_uri: uri,
            target_range,
            target_selection_range: selection_range,
            origin_selection_range: src,
        })
    }
}

impl ToLink for ReferenceTarget {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        self.file_range().to_location(db, encoding)
    }

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        let uri = file_to_url(db, self.file())?;
        let source = source_text(db, self.file());
        let index = line_index(db, self.file());

        let target_range = self.range().to_lsp_range(&source, &index, encoding);
        let selection_range = target_range;

        let src = src.map(|src| {
            let source = source_text(db, src.file());
            let index = line_index(db, src.file());

            src.range().to_lsp_range(&source, &index, encoding)
        });

        Some(lsp_types::LocationLink {
            target_uri: uri,
            target_range,
            target_selection_range: selection_range,
            origin_selection_range: src,
        })
    }
}
