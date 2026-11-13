use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::path::Path;

use mactype_service_contract::MAX_RUNTIME_POINTER_BYTES;
#[cfg(windows)]
use mactype_service_platform::is_reparse_point;

pub(crate) const MAX_POINTER_BYTES: u64 = MAX_RUNTIME_POINTER_BYTES;

pub(crate) fn read_bounded_regular_file(path: &Path, maximum_bytes: u64) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new().read(true).open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protected file is not a bounded regular file",
        ));
    }
    read_bounded_contents(file, maximum_bytes)
}

pub(crate) fn read_bounded_contents(reader: impl Read, maximum_bytes: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    // The max+1 read must remain even after metadata validation because files can grow in place.
    reader
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "protected file is outside the bounded size",
        ));
    }
    Ok(bytes)
}

pub(crate) fn has_reparse_ancestor(path: &Path) -> io::Result<bool> {
    for ancestor in path.ancestors().filter(|candidate| candidate.exists()) {
        if is_reparse_point(ancestor)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.file_type().is_symlink())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::read_bounded_contents;

    #[test]
    fn bounded_reader_rejects_a_stream_that_grows_after_the_metadata_size_check() {
        assert!(read_bounded_contents(Cursor::new(vec![b'x'; 65]), 64).is_err());
    }
}
