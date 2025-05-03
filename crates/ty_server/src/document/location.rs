use crate::document::{FileRangeExt, ToRangeExt};
use crate::system::file_to_url;
use crate::PositionEncoding;
use lsp_types::Location;
use ruff_db::files::FileRange;
use ruff_db::source::{line_index, source_text};
use ruff_text_size::Ranged;
use ty_ide::{Db, NavigationTarget};

pub(crate) trait ToLink {
    fn to_location(&self, db: &dyn ty_ide::Db, encoding: PositionEncoding) -> Option<Location>;

    fn to_link(
        &self,
        db: &dyn ty_ide::Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink>;
}

impl ToLink for NavigationTarget {
    fn to_location(&self, db: &dyn Db, encoding: PositionEncoding) -> Option<Location> {
        FileRange::new(self.file(), self.focus_range()).to_location(db.upcast(), encoding)
    }

    fn to_link(
        &self,
        db: &dyn Db,
        src: Option<FileRange>,
        encoding: PositionEncoding,
    ) -> Option<lsp_types::LocationLink> {
        let file = self.file();
        let uri = file_to_url(db.upcast(), file)?;
        let source = source_text(db.upcast(), file);
        let index = line_index(db.upcast(), file);

        let target_range = self.full_range().to_lsp_range(&source, &index, encoding);
        let selection_range = self.focus_range().to_lsp_range(&source, &index, encoding);

        let src = src.map(|src| {
            let source = source_text(db.upcast(), src.file());
            let index = line_index(db.upcast(), src.file());

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
