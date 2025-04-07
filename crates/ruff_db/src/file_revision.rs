use crate::system::file_time_now;

/// A number representing the revision of a file.
///
/// Two revisions that don't compare equal signify that the file has been modified.
/// Revisions aren't guaranteed to be monotonically increasing or in any specific order.
///
/// Possible revisions are:
/// * The last modification time of the file.
/// * The hash of the file's content.
/// * The revision as it comes from an external system, for example the LSP.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct FileRevision(u128);

impl FileRevision {
    pub fn new(value: u128) -> Self {
        Self(value)
    }

    pub fn now() -> Self {
        Self::from(file_time_now())
    }

    pub const fn zero() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn as_u128(self) -> u128 {
        self.0
    }
}

impl From<u128> for FileRevision {
    fn from(value: u128) -> Self {
        FileRevision(value)
    }
}

impl From<u64> for FileRevision {
    fn from(value: u64) -> Self {
        FileRevision(u128::from(value))
    }
}

impl From<filetime::FileTime> for FileRevision {
    fn from(value: filetime::FileTime) -> Self {
        let seconds = value.seconds() as u128;
        let seconds = seconds << 64;
        let nanos = u128::from(value.nanoseconds());

        FileRevision(seconds | nanos)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn revision_from_file_time() {
        let file_time = file_time_now();
        let revision = FileRevision::from(file_time);

        let revision = revision.as_u128();

        let nano = revision & 0xFFFF_FFFF_FFFF_FFFF;
        let seconds = revision >> 64;

        assert_eq!(file_time.nanoseconds(), nano as u32);
        assert_eq!(file_time.seconds(), seconds as i64);
    }
}
