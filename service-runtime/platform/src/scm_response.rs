//! An SCM query response held in word-aligned storage, with every embedded
//! pointer checked against the allocation before it is read.

use std::io;
use std::mem::{align_of, size_of, size_of_val};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;

/// The largest response any SCM query may hand back. `QueryServiceConfigW`
/// documents 8 KiB; the Config2 levels are larger, so one generous cap
/// covers them all.
pub(crate) const MAX_SCM_RESPONSE_BYTES: u32 = 64 * 1024;

/// A response buffer: `words` gives pointer alignment for the fixed header,
/// `byte_length` is how many bytes the SCM reported filling (<= capacity).
pub(crate) struct ScmResponse {
    words: Vec<usize>,
    byte_length: usize,
}

impl ScmResponse {
    /// Runs the two-call size-probe-then-fill protocol shared by
    /// `QueryServiceConfigW`, `QueryServiceConfig2W`, and
    /// `QueryServiceObjectSecurity`.
    ///
    /// `call(buffer, capacity, needed)` must perform exactly one Win32 call
    /// with those arguments and return its BOOL. The first invocation passes a
    /// null buffer and zero capacity; it must fail with
    /// `ERROR_INSUFFICIENT_BUFFER` and report a size in
    /// `minimum_bytes..=MAX_SCM_RESPONSE_BYTES`. The second passes the real,
    /// word-aligned buffer and its full capacity.
    pub(crate) fn query(
        minimum_bytes: usize,
        mut call: impl FnMut(*mut u8, u32, &mut u32) -> i32,
    ) -> io::Result<Self> {
        let mut needed = 0_u32;
        let result = call(null_mut(), 0, &mut needed);
        let error = io::Error::last_os_error();
        if result != 0 {
            return Err(invalid_data("SCM size probe succeeded without a buffer"));
        }
        if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
            return Err(error);
        }
        if (needed as usize) < minimum_bytes || needed > MAX_SCM_RESPONSE_BYTES {
            return Err(invalid_data("SCM response size is out of range"));
        }

        let word_count = (needed as usize).div_ceil(size_of::<usize>());
        let mut words = vec![0_usize; word_count];
        let capacity = size_of_val(words.as_slice()) as u32;
        let result = call(words.as_mut_ptr().cast::<u8>(), capacity, &mut needed);
        let error = io::Error::last_os_error();
        if result == 0 {
            return Err(error);
        }

        let byte_length = usize::min(needed as usize, capacity as usize);
        if byte_length < minimum_bytes {
            return Err(invalid_data("SCM response is shorter than its header"));
        }
        Ok(Self { words, byte_length })
    }

    fn start(&self) -> usize {
        self.words.as_ptr() as usize
    }

    fn end(&self) -> usize {
        self.start() + self.byte_length
    }

    /// Returns the fixed header at the start of the response.
    pub(crate) fn header<T: Copy>(&self) -> io::Result<T> {
        if align_of::<T>() > align_of::<usize>() {
            return Err(invalid_data("SCM response header alignment is unsupported"));
        }
        if self.byte_length < size_of::<T>() {
            return Err(invalid_data("SCM response is shorter than its header"));
        }
        // SAFETY: `words` is aligned for `T`, and the successful query wrote
        // every byte of the header inside `byte_length`.
        Ok(unsafe { self.words.as_ptr().cast::<T>().read() })
    }

    /// Copies a fixed-size array addressed inside this response.
    pub(crate) fn array<T: Copy>(
        &self,
        pointer: *const T,
        count: u32,
        max_entries: usize,
    ) -> io::Result<Vec<T>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let count = count as usize;
        if pointer.is_null() || count > max_entries {
            return Err(invalid_data("SCM response array is invalid"));
        }
        let address = pointer as usize;
        if address < self.start() || address >= self.end() {
            return Err(invalid_data(
                "SCM response array pointer is outside the response",
            ));
        }
        if address % align_of::<T>() != 0 {
            return Err(invalid_data("SCM response array pointer is misaligned"));
        }
        let byte_length = size_of::<T>()
            .checked_mul(count)
            .ok_or_else(|| invalid_data("SCM response array size overflowed"))?;
        let array_end = address
            .checked_add(byte_length)
            .ok_or_else(|| invalid_data("SCM response array range overflowed"))?;
        if array_end > self.end() {
            return Err(invalid_data("SCM response array extends past the response"));
        }
        // SAFETY: the pointer is aligned and lies inside `words`; the checked
        // byte range for all `count` elements ends no later than `byte_length`.
        Ok(unsafe { std::slice::from_raw_parts(pointer, count) }.to_vec())
    }

    /// Returns a NUL-terminated UTF-16 string addressed inside this response.
    pub(crate) fn wide_units(&self, pointer: *const u16) -> io::Result<Option<&[u16]>> {
        if pointer.is_null() {
            return Ok(None);
        }
        let address = pointer as usize;
        if address < self.start() || address >= self.end() {
            return Err(invalid_data("UTF-16 pointer is outside the SCM response"));
        }
        if address % align_of::<u16>() != 0 {
            return Err(invalid_data("UTF-16 pointer is misaligned"));
        }

        let available = (self.end() - address) / size_of::<u16>();
        // SAFETY: the pointer is aligned and lies inside `words`; `available`
        // ends no later than `byte_length`, which never exceeds the allocation.
        let units = unsafe { std::slice::from_raw_parts(pointer, available) };
        let length = units.iter().position(|unit| *unit == 0).ok_or_else(|| {
            invalid_data("UTF-16 string is not terminated inside the SCM response")
        })?;
        Ok(Some(&units[..length]))
    }

    /// Returns strings from a double-NUL-terminated UTF-16 block.
    pub(crate) fn multi_units(
        &self,
        pointer: *const u16,
        max_entries: usize,
    ) -> io::Result<Vec<&[u16]>> {
        if pointer.is_null() {
            return Ok(Vec::new());
        }

        let mut current = pointer;
        let mut entries = Vec::new();
        loop {
            if current as usize >= self.end() {
                return Err(invalid_data("MULTI_SZ terminator is missing"));
            }
            let units = self
                .wide_units(current)?
                .expect("a non-null MULTI_SZ pointer stays non-null");
            if units.is_empty() {
                return Ok(entries);
            }
            if entries.len() >= max_entries {
                return Err(invalid_data("MULTI_SZ has too many entries"));
            }
            entries.push(units);
            // SAFETY: `wide_units` found the NUL inside this allocation, so
            // advancing past the string stays within it or reaches one-past-end.
            current = unsafe { current.add(units.len() + 1) };
        }
    }

    #[cfg(test)]
    pub(crate) fn from_words(words: Vec<usize>, byte_length: usize) -> Self {
        assert!(byte_length <= size_of_val(words.as_slice()));
        Self { words, byte_length }
    }
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::mem::{size_of, size_of_val};

    use super::ScmResponse;

    fn response_with_units(units: &[u16], byte_length: usize) -> ScmResponse {
        let word_count = size_of_val(units).div_ceil(size_of::<usize>());
        let mut words = vec![0_usize; word_count.max(1)];
        // SAFETY: the slice covers the live word allocation as UTF-16 units;
        // `usize` alignment satisfies `u16` alignment.
        let storage = unsafe {
            std::slice::from_raw_parts_mut(
                words.as_mut_ptr().cast::<u16>(),
                words.len() * size_of::<usize>() / size_of::<u16>(),
            )
        };
        storage[..units.len()].copy_from_slice(units);
        ScmResponse::from_words(words, byte_length)
    }

    #[test]
    fn terminated_string_inside_the_response_reads_back() {
        let units: Vec<u16> = "inside\0".encode_utf16().collect();
        let response = response_with_units(&units, units.len() * size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        let read = response.wide_units(pointer).unwrap().unwrap();
        assert_eq!(String::from_utf16_lossy(read), "inside");
    }

    #[test]
    fn string_with_a_terminator_past_the_reported_length_fails() {
        let units: Vec<u16> = "outside\0".encode_utf16().collect();
        let response = response_with_units(&units, (units.len() - 1) * size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        assert!(response.wide_units(pointer).is_err());
    }

    #[test]
    fn outside_and_misaligned_string_pointers_fail() {
        let units: Vec<u16> = "x\0".encode_utf16().collect();
        let response = response_with_units(&units, units.len() * size_of::<u16>());
        let start = response.start();
        let end = response.end();

        assert!(response.wide_units((start - 2) as *const u16).is_err());
        assert!(response.wide_units(end as *const u16).is_err());
        assert!(response.wide_units((end + 2) as *const u16).is_err());
        assert!(response.wide_units((start + 1) as *const u16).is_err());
    }

    #[test]
    fn multi_string_reads_every_entry_and_accepts_an_empty_block() {
        let units: Vec<u16> = "one\0two\0\0".encode_utf16().collect();
        let response = response_with_units(&units, units.len() * size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        let entries: Vec<String> = response
            .multi_units(pointer, 2)
            .unwrap()
            .into_iter()
            .map(String::from_utf16_lossy)
            .collect();
        assert_eq!(entries, ["one", "two"]);

        let empty = [0_u16];
        let response = response_with_units(&empty, size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        assert!(response.multi_units(pointer, 0).unwrap().is_empty());
    }

    #[test]
    fn multi_string_without_its_empty_terminator_fails() {
        let units: Vec<u16> = "one\0two\0".encode_utf16().collect();
        let response = response_with_units(&units, units.len() * size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        assert!(response.multi_units(pointer, 2).is_err());
    }

    #[test]
    fn multi_string_entry_limit_is_enforced() {
        let units: Vec<u16> = "one\0two\0\0".encode_utf16().collect();
        let response = response_with_units(&units, units.len() * size_of::<u16>());
        let pointer = response.words.as_ptr().cast::<u16>();
        assert!(response.multi_units(pointer, 1).is_err());
    }

    #[test]
    fn an_in_range_array_reads_back() {
        let response = ScmResponse::from_words(vec![11, 22, 33], 3 * size_of::<usize>());
        let pointer = response.words.as_ptr();
        assert_eq!(response.array(pointer, 3, 3).unwrap(), [11, 22, 33]);
        assert!(response
            .array::<usize>(std::ptr::null(), 0, 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn outside_and_misaligned_array_pointers_fail() {
        let response = ScmResponse::from_words(vec![11, 22], 2 * size_of::<usize>());
        let start = response.start();
        let end = response.end();

        assert!(response
            .array::<usize>((start - size_of::<usize>()) as *const usize, 1, 1)
            .is_err());
        assert!(response.array::<usize>(end as *const usize, 1, 1).is_err());
        assert!(response
            .array::<usize>((start + 1) as *const usize, 1, 1)
            .is_err());
        assert!(response.array(response.words.as_ptr(), 3, 3).is_err());
        assert!(response.array(response.words.as_ptr(), 2, 1).is_err());
    }

    #[test]
    fn an_empty_response_is_shorter_than_a_u32_header() {
        let response = ScmResponse::from_words(Vec::new(), 0);
        assert!(response.header::<u32>().is_err());
    }
}
