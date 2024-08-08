pub use self::vendored::vendored_typeshed_stubs;
pub(super) use self::versions::{
    parse_typeshed_versions, LazyTypeshedVersions, TypeshedVersionsParseError,
    TypeshedVersionsQueryResult,
};

mod vendored;
mod versions;
