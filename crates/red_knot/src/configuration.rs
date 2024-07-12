use ruff_db::program::TargetVersion;
use ruff_db::system::SystemPathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Configuration {
    pub target_version: TargetVersion,
    pub custom_typeshed_dir: Option<SystemPathBuf>,
    pub extra_search_paths: Vec<SystemPathBuf>,
}
