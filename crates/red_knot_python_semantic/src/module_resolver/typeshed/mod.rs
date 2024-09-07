pub use self::vendored::vendored_typeshed_stubs;
pub(super) use self::versions::{
    typeshed_versions, vendored_typeshed_versions, TypeshedVersions, TypeshedVersionsParseError,
    TypeshedVersionsQueryResult,
};

mod vendored;
mod versions;
