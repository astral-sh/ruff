use std::ffi::{CStr, OsStr, OsString};
use std::fs::OpenOptions;
use std::io;
use std::mem::offset_of;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use filetime::FileTime;
use rustc_hash::FxHashMap;

use super::FileMetadata;

const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const VREG: u32 = 1;
const REQUIRED: u32 = libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_OBJTYPE
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_ACCESSMASK;

#[repr(align(8))]
struct Buffer([u8; 64 * 1024]);

/// Darwin's attribute packing order. Records are decoded through checked slices, never cast.
#[repr(C)]
struct Attributes {
    length: u32,
    returned: libc::attribute_set_t,
    error: u32,
    name: libc::attrreference_t,
    object_type: u32,
    modified: libc::timespec,
    mode: u32,
}

#[expect(unsafe_code)]
pub(super) fn read(directory: &Path) -> io::Result<Option<FxHashMap<OsString, FileMetadata>>> {
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(directory)?;
    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_ERROR | REQUIRED,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut buffer = Buffer([0; 64 * 1024]);
    let mut entries = FxHashMap::default();
    loop {
        // SAFETY: The descriptor is an open directory, the request is initialized, and the
        // writable buffer is eight-byte aligned and has the supplied length.
        let count = unsafe {
            libc::getattrlistbulk(
                directory.as_raw_fd(),
                (&raw mut attributes).cast(),
                buffer.0.as_mut_ptr().cast(),
                buffer.0.len(),
                u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
            )
        };
        let Ok(count) = usize::try_from(count) else {
            return Err(io::Error::last_os_error());
        };
        if count == 0 {
            return Ok(Some(entries));
        }
        if !decode(&buffer.0, count, &mut entries)? {
            return Ok(None);
        }
    }
}

fn decode(
    mut buffer: &[u8],
    count: usize,
    entries: &mut FxHashMap<OsString, FileMetadata>,
) -> io::Result<bool> {
    for _ in 0..count {
        let length = u32::from_ne_bytes(bytes(buffer, offset_of!(Attributes, length))?) as usize;
        let record = buffer.get(..length).ok_or_else(invalid_attributes)?;
        buffer = &buffer[length..];
        let returned =
            u32::from_ne_bytes(bytes(record, offset_of!(Attributes, returned.commonattr))?);
        if returned & ATTR_CMN_ERROR != 0
            && u32::from_ne_bytes(bytes(record, offset_of!(Attributes, error))?) != 0
        {
            // The individual lookup will report the error if this entry is needed.
            continue;
        }
        if returned & libc::ATTR_CMN_OBJTYPE == 0 {
            return Ok(false);
        }
        if u32::from_ne_bytes(bytes(record, offset_of!(Attributes, object_type))?) != VREG {
            continue;
        }
        if returned & REQUIRED != REQUIRED {
            return Ok(false);
        }

        let seconds = i64::from_ne_bytes(bytes(record, offset_of!(Attributes, modified.tv_sec))?);
        let nanos = i64::from_ne_bytes(bytes(record, offset_of!(Attributes, modified.tv_nsec))?);
        let nanos = u32::try_from(nanos)
            .ok()
            .filter(|nanos| *nanos < 1_000_000_000)
            .ok_or_else(invalid_attributes)?;
        let mode = u32::from_ne_bytes(bytes(record, offset_of!(Attributes, mode))?);
        entries.insert(
            file_name(record)?.to_os_string(),
            FileMetadata {
                last_modified: FileTime::from_unix_time(seconds, nanos),
                mode: (mode & !u32::from(libc::S_IFMT)) | u32::from(libc::S_IFREG),
            },
        );
    }
    Ok(true)
}

fn file_name(record: &[u8]) -> io::Result<&OsStr> {
    let offset = i32::from_ne_bytes(bytes(record, offset_of!(Attributes, name.attr_dataoffset))?);
    let length =
        u32::from_ne_bytes(bytes(record, offset_of!(Attributes, name.attr_length))?) as usize;
    let start = offset_of!(Attributes, name)
        .checked_add_signed(offset as isize)
        .ok_or_else(invalid_attributes)?;
    let end = start.checked_add(length).ok_or_else(invalid_attributes)?;
    let name = CStr::from_bytes_with_nul(record.get(start..end).ok_or_else(invalid_attributes)?)
        .map_err(|_| invalid_attributes())?
        .to_bytes();
    if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
        return Err(invalid_attributes());
    }
    Ok(OsStr::from_bytes(name))
}

fn bytes<const N: usize>(buffer: &[u8], offset: usize) -> io::Result<[u8; N]> {
    buffer
        .get(offset..offset + N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(invalid_attributes)
}

fn invalid_attributes() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bulk file attributes")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use filetime::{FileTime, set_file_mtime};
    use rustc_hash::FxHashMap;

    use super::{ATTR_CMN_ERROR, REQUIRED, VREG, decode, read};

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "Compare native bulk attributes with native filesystem metadata"
    )]
    fn matches_individual_metadata_across_batches() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        for index in 0..1500 {
            let path = root.path().join(format!("file-{index:064}"));
            fs::write(&path, "")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640 | (index & 0o111)))?;
            set_file_mtime(&path, FileTime::from_unix_time(1_700_000_000, index))?;
        }
        let name = OsStr::new("file-λ");
        fs::write(root.path().join(name), "")?;
        fs::create_dir(root.path().join("directory"))?;
        symlink(root.path().join(name), root.path().join("link"))?;
        symlink("missing", root.path().join("broken"))?;

        let entries = read(root.path())?.expect("bulk attributes supported on the test filesystem");
        assert_eq!(entries.len(), 1501);
        for (name, entry) in entries {
            let metadata = fs::metadata(root.path().join(name))?;
            assert_eq!(
                entry.last_modified,
                FileTime::from_last_modification_time(&metadata)
            );
            assert_eq!(entry.mode, metadata.permissions().mode());
        }
        Ok(())
    }

    /// Encode the documented wire layout independently of `Attributes`.
    fn record(name: &[u8]) -> Vec<u8> {
        let length = (60 + name.len() + 1).next_multiple_of(8);
        let fields = [
            u32::try_from(length).unwrap(),
            REQUIRED | ATTR_CMN_ERROR | libc::ATTR_CMN_RETURNED_ATTRS,
            0,
            0,
            0,
            0,
            0,
            32,
            u32::try_from(name.len() + 1).unwrap(),
            VREG,
        ];
        let mut record = fields
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect::<Vec<_>>();
        record.extend_from_slice(&(-1_i64).to_ne_bytes());
        record.extend_from_slice(&123_i64.to_ne_bytes());
        record.extend_from_slice(&0o640_u32.to_ne_bytes());
        record.extend_from_slice(name);
        record.resize(length, 0);
        record
    }

    #[test]
    fn validates_records_and_required_attributes() {
        let valid = record(b"file");
        let mut entries = FxHashMap::default();
        assert!(decode(&valid, 1, &mut entries).unwrap());
        assert_eq!(
            entries[OsStr::new("file")].last_modified,
            FileTime::from_unix_time(-1, 123)
        );
        for length in 0..valid.len() {
            assert!(decode(&valid[..length], 1, &mut FxHashMap::default()).is_err());
        }
        for name in [
            b"../outside".as_slice(),
            b"/outside",
            b".",
            b"..",
            b"",
            b"a\0b",
        ] {
            assert!(decode(&record(name), 1, &mut FxHashMap::default()).is_err());
        }
        let mut missing = valid;
        missing[4..8].copy_from_slice(&(REQUIRED & !libc::ATTR_CMN_MODTIME).to_ne_bytes());
        assert!(!decode(&missing, 1, &mut FxHashMap::default()).unwrap());

        let name = OsStr::from_bytes(b"file-\xff");
        let mut entries = FxHashMap::default();
        assert!(decode(&record(name.as_bytes()), 1, &mut entries).unwrap());
        assert!(entries.contains_key(name));
    }
}
