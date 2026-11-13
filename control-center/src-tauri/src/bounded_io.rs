use std::{fs::File, io::Read, path::Path};

pub(crate) fn read_bounded_file(
    path: &Path,
    maximum_bytes: usize,
    description: &str,
) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    read_open_file_bounded(file, maximum_bytes, true, description)
}

pub(crate) fn read_open_file_bounded(
    file: File,
    maximum_bytes: usize,
    allow_empty: bool,
    description: &str,
) -> Result<Vec<u8>, String> {
    read_open_file_bounded_after_metadata(file, maximum_bytes, allow_empty, description, || {})
}

fn read_open_file_bounded_after_metadata(
    file: File,
    maximum_bytes: usize,
    allow_empty: bool,
    description: &str,
    after_metadata: impl FnOnce(),
) -> Result<Vec<u8>, String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("{description} is not a regular file"));
    }
    if metadata.len() > maximum_bytes as u64 {
        return Err(format!(
            "{description} exceeds its {maximum_bytes}-byte limit"
        ));
    }
    after_metadata();
    let initial_capacity = usize::try_from(metadata.len())
        .unwrap_or(maximum_bytes)
        .min(maximum_bytes);
    let mut bytes = Vec::with_capacity(initial_capacity);
    file.take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum_bytes {
        return Err(format!(
            "{description} exceeds its {maximum_bytes}-byte limit"
        ));
    }
    if !allow_empty && bytes.is_empty() {
        return Err(format!("{description} must not be empty"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::{Seek, SeekFrom, Write},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mactype-bounded-io-{name}-{unique}"))
    }

    #[test]
    fn growth_after_metadata_is_rejected_by_the_read_limit() {
        let path = temp_path("growth");
        fs::write(&path, b"1234").unwrap();
        let input = File::open(&path).unwrap();

        let error = read_open_file_bounded_after_metadata(input, 4, true, "test file", || {
            let mut output = OpenOptions::new().write(true).open(&path).unwrap();
            output.seek(SeekFrom::End(0)).unwrap();
            output.write_all(b"5").unwrap();
            output.flush().unwrap();
        })
        .unwrap_err();

        assert!(error.contains("4-byte limit"));
        let _ = fs::remove_file(path);
    }
}
