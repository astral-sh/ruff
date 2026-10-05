#[salsa::tracked]
struct UnknownAttributeTrackedStruct<'db> {
    #[tracked]
    tracked: bool,
    #[unknown_attr]
    field: bool,
    #[salsa::tracked]
    wrong_tracked: bool,
    /// TestDocComment
    /// TestDocComment
    field_with_doc: bool
}

fn main() {}
