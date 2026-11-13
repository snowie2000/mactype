//! Bounded storage for Win32 APIs that report a required byte count.

use std::io;
#[cfg(test)]
use std::mem::size_of_val;
use std::mem::{align_of, size_of};
use std::ptr::null_mut;

/// The result of one call that writes into a caller-owned buffer.
#[derive(Debug)]
pub(crate) enum CallOutcome {
    Complete,
    MoreData(io::Error),
}

/// A failure in the null-probe-then-fill protocol.
#[derive(Debug)]
pub(crate) enum ProbeError {
    Call(io::Error),
    ProbeCompleted(io::Error),
    SizeOutOfRange,
    FillNeedsMore(io::Error),
    ReturnedLengthOutOfRange { needed: usize },
}

/// A failure in the grow-and-retry protocol.
#[derive(Debug)]
pub(crate) enum RetryError {
    Call(io::Error),
    LimitExceeded,
    ReturnedLengthExceedsCapacity,
    ReturnedLengthExceedsLimit,
}

/// Word-aligned storage whose logical length is the byte count reported by the
/// API, not the allocation's rounded byte capacity.
#[derive(Debug)]
pub(crate) struct WordAlignedBuffer {
    words: Vec<usize>,
    byte_length: usize,
}

impl WordAlignedBuffer {
    pub(crate) fn len(&self) -> usize {
        self.byte_length
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.words.as_ptr().cast()
    }

    pub(crate) fn supports<T>(&self) -> bool {
        align_of::<T>() <= align_of::<usize>() && self.byte_length >= size_of::<T>()
    }

    /// Reads a fixed header after the caller validates the byte representation.
    ///
    /// # Safety
    /// The reported bytes at the start of the buffer must form a valid `T`.
    pub(crate) unsafe fn read<T: Copy>(&self) -> Option<T> {
        if !self.supports::<T>() {
            return None;
        }
        // SAFETY: `words` supplies at least `usize` alignment, `supports`
        // checked T's alignment and complete byte range, and the caller
        // guarantees that those initialized bytes form a valid T.
        Some(unsafe { self.words.as_ptr().cast::<T>().read() })
    }

    #[cfg(test)]
    pub(crate) fn from_words(words: Vec<usize>, byte_length: usize) -> Self {
        assert!(byte_length <= size_of_val(words.as_slice()));
        Self { words, byte_length }
    }
}

trait Storage: Sized {
    fn zeroed(byte_capacity: usize) -> Self;
    fn pointer(&mut self) -> *mut u8;
    fn set_length(&mut self, byte_length: usize);
}

struct ByteBuffer(Vec<u8>);

impl ByteBuffer {
    fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl Storage for ByteBuffer {
    fn zeroed(byte_capacity: usize) -> Self {
        Self(vec![0_u8; byte_capacity])
    }

    fn pointer(&mut self) -> *mut u8 {
        if self.0.is_empty() {
            null_mut()
        } else {
            self.0.as_mut_ptr()
        }
    }

    fn set_length(&mut self, byte_length: usize) {
        self.0.truncate(byte_length);
    }
}

impl Storage for WordAlignedBuffer {
    fn zeroed(byte_capacity: usize) -> Self {
        let word_count = byte_capacity.div_ceil(size_of::<usize>());
        Self {
            words: vec![0_usize; word_count],
            byte_length: byte_capacity,
        }
    }

    fn pointer(&mut self) -> *mut u8 {
        if self.words.is_empty() {
            null_mut()
        } else {
            self.words.as_mut_ptr().cast()
        }
    }

    fn set_length(&mut self, byte_length: usize) {
        self.byte_length = byte_length;
    }
}

/// Calls an API first with a null buffer and then with word-aligned storage.
/// The maximum applies to bytes passed to the API, excluding allocation-only
/// padding needed to align and contain the final partial word.
pub(crate) fn probe_word_aligned(
    minimum_bytes: usize,
    maximum_bytes: usize,
    mut call: impl FnMut(*mut usize, u32, &mut u32) -> io::Result<CallOutcome>,
) -> Result<WordAlignedBuffer, ProbeError> {
    probe(minimum_bytes, maximum_bytes, |buffer, capacity, needed| {
        call(buffer.cast(), capacity, needed)
    })
}

/// Calls an API first with a null buffer and then with byte storage. No buffer
/// is exposed unless the required and returned lengths both fit the caller's
/// minimum and maximum.
pub(crate) fn probe_bytes(
    minimum_bytes: usize,
    maximum_bytes: usize,
    call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> Result<Vec<u8>, ProbeError> {
    probe::<ByteBuffer>(minimum_bytes, maximum_bytes, call).map(ByteBuffer::into_vec)
}

fn probe<S: Storage>(
    minimum_bytes: usize,
    maximum_bytes: usize,
    mut call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> Result<S, ProbeError> {
    let maximum_bytes = maximum_bytes.min(u32::MAX as usize);
    let mut needed = 0_u32;
    match call(null_mut(), 0, &mut needed).map_err(ProbeError::Call)? {
        CallOutcome::Complete if needed == 0 && minimum_bytes == 0 => {
            return Ok(S::zeroed(0));
        }
        CallOutcome::Complete => {
            return Err(ProbeError::ProbeCompleted(io::Error::last_os_error()));
        }
        CallOutcome::MoreData(_) => {}
    }

    let needed = needed as usize;
    if needed < minimum_bytes || needed > maximum_bytes {
        return Err(ProbeError::SizeOutOfRange);
    }

    // The allocation is zero-initialized, so rounded word padding and any
    // bytes not overwritten by the API never contain uninitialized data.
    let mut storage = S::zeroed(needed);
    let capacity = needed as u32;
    let mut filled = capacity;
    match call(storage.pointer(), capacity, &mut filled).map_err(ProbeError::Call)? {
        CallOutcome::Complete => {}
        // A larger requirement after the probe is a two-call race; the short
        // allocation is discarded without exposing it.
        CallOutcome::MoreData(error) => return Err(ProbeError::FillNeedsMore(error)),
    }

    let filled = filled as usize;
    if filled < minimum_bytes || filled > needed || filled > maximum_bytes {
        return Err(ProbeError::ReturnedLengthOutOfRange { needed: filled });
    }
    storage.set_length(filled);
    Ok(storage)
}

/// Repeats an API call with a larger byte buffer after `MoreData`, without
/// ever passing more than `maximum_bytes` to the API. A completed call must
/// report no more than its allocation; only that reported prefix is returned.
pub(crate) fn retry_bytes(
    initial_bytes: usize,
    maximum_bytes: usize,
    mut call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> Result<Vec<u8>, RetryError> {
    let maximum_bytes = maximum_bytes.min(u32::MAX as usize);
    if initial_bytes > maximum_bytes {
        return Err(RetryError::LimitExceeded);
    }

    let mut capacity = initial_bytes;
    loop {
        let mut storage = ByteBuffer::zeroed(capacity);
        let mut filled = capacity as u32;
        match call(storage.pointer(), capacity as u32, &mut filled).map_err(RetryError::Call)? {
            CallOutcome::Complete => {
                if filled as usize > maximum_bytes {
                    return Err(RetryError::ReturnedLengthExceedsLimit);
                }
                if filled as usize > capacity {
                    return Err(RetryError::ReturnedLengthExceedsCapacity);
                }
                storage.set_length(filled as usize);
                return Ok(storage.into_vec());
            }
            CallOutcome::MoreData(_error) => {
                let reported = filled as usize;
                if reported > maximum_bytes || capacity == maximum_bytes {
                    return Err(RetryError::LimitExceeded);
                }
                let geometric = capacity.saturating_mul(2).max(1);
                let next = reported.max(geometric).min(maximum_bytes);
                if next <= capacity {
                    return Err(RetryError::LimitExceeded);
                }
                capacity = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::mem::{align_of, size_of};

    use super::{
        CallOutcome, ProbeError, RetryError, probe_bytes, probe_word_aligned, retry_bytes,
    };

    fn more_data() -> CallOutcome {
        CallOutcome::MoreData(io::Error::from_raw_os_error(234))
    }

    #[test]
    fn probe_exact_fit_exposes_only_the_reported_bytes() {
        let mut calls = 0;
        let bytes = probe_bytes(1, 16, |buffer, capacity, needed| {
            calls += 1;
            if buffer.is_null() {
                assert_eq!(capacity, 0);
                *needed = 4;
                return Ok(more_data());
            }
            assert_eq!(capacity, 4);
            // SAFETY: the protocol supplied a writable four-byte allocation.
            unsafe { std::ptr::copy_nonoverlapping([1_u8, 2, 3, 4].as_ptr(), buffer, 4) };
            *needed = 4;
            Ok(CallOutcome::Complete)
        })
        .unwrap();

        assert_eq!(calls, 2);
        assert_eq!(bytes, [1, 2, 3, 4]);
    }

    #[test]
    fn retry_grows_when_the_size_changes_between_calls() {
        let mut capacities = Vec::new();
        let bytes = retry_bytes(0, 8, |buffer, capacity, needed| {
            capacities.push(capacity);
            match capacity {
                0 => {
                    assert!(buffer.is_null());
                    *needed = 3;
                    Ok(more_data())
                }
                3 => {
                    *needed = 6;
                    Ok(more_data())
                }
                6 => {
                    *needed = 6;
                    Ok(CallOutcome::Complete)
                }
                _ => unreachable!(),
            }
        })
        .unwrap();

        assert_eq!(capacities, [0, 3, 6]);
        assert_eq!(bytes.len(), 6);
    }

    #[test]
    fn probe_rejects_growth_between_calls_without_exposing_the_short_buffer() {
        let mut calls = 0;
        let error = probe_bytes(1, 8, |buffer, capacity, needed| {
            calls += 1;
            if buffer.is_null() {
                *needed = 4;
                return Ok(more_data());
            }
            assert_eq!(capacity, 4);
            *needed = 6;
            Ok(more_data())
        })
        .unwrap_err();

        assert_eq!(calls, 2);
        assert!(matches!(error, ProbeError::FillNeedsMore(_)));
    }

    #[test]
    fn both_protocols_enforce_the_caller_cap() {
        let probe_error = probe_bytes(0, 4, |_, _, needed| {
            *needed = 5;
            Ok(more_data())
        })
        .unwrap_err();
        assert!(matches!(probe_error, ProbeError::SizeOutOfRange));

        let retry_error = retry_bytes(0, 4, |_, _, needed| {
            *needed = 5;
            Ok(more_data())
        })
        .unwrap_err();
        assert!(matches!(retry_error, RetryError::LimitExceeded));
    }

    #[test]
    fn an_api_cannot_report_more_bytes_than_the_buffer_it_received() {
        let retry_error = retry_bytes(4, 8, |_, capacity, needed| {
            assert_eq!(capacity, 4);
            *needed = 5;
            Ok(CallOutcome::Complete)
        })
        .unwrap_err();
        assert!(matches!(
            retry_error,
            RetryError::ReturnedLengthExceedsCapacity
        ));

        let limit_error = retry_bytes(4, 4, |_, capacity, needed| {
            assert_eq!(capacity, 4);
            *needed = 5;
            Ok(CallOutcome::Complete)
        })
        .unwrap_err();
        assert!(matches!(
            limit_error,
            RetryError::ReturnedLengthExceedsLimit
        ));

        let mut probe = true;
        let probe_error = probe_bytes(1, 8, |_, capacity, needed| {
            if probe {
                probe = false;
                *needed = 4;
                return Ok(more_data());
            }
            assert_eq!(capacity, 4);
            *needed = 5;
            Ok(CallOutcome::Complete)
        })
        .unwrap_err();
        assert!(matches!(
            probe_error,
            ProbeError::ReturnedLengthOutOfRange { needed: 5 }
        ));
    }

    #[test]
    fn a_successful_zero_length_probe_returns_an_empty_buffer() {
        let bytes = probe_bytes(0, 0, |buffer, capacity, needed| {
            assert!(buffer.is_null());
            assert_eq!(capacity, 0);
            *needed = 0;
            Ok(CallOutcome::Complete)
        })
        .unwrap();

        assert!(bytes.is_empty());
    }

    #[test]
    fn word_storage_aligns_the_fill_pointer_and_header() {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        struct Header {
            first: usize,
            second: u32,
        }

        let expected = Header {
            first: 0x1234,
            second: 0x5678,
        };
        let response = probe_word_aligned(
            size_of::<Header>(),
            size_of::<Header>(),
            |buffer, capacity, needed| {
                if buffer.is_null() {
                    *needed = size_of::<Header>() as u32;
                    return Ok(more_data());
                }
                assert_eq!((buffer as usize) % align_of::<usize>(), 0);
                assert_eq!(capacity as usize, size_of::<Header>());
                // SAFETY: the word-aligned allocation has room for Header.
                unsafe { buffer.cast::<Header>().write(expected) };
                Ok(CallOutcome::Complete)
            },
        )
        .unwrap();

        // SAFETY: the fake API wrote `expected` as a complete Header.
        assert_eq!(unsafe { response.read::<Header>() }, Some(expected));
    }
}
