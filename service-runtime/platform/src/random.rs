//! Random bytes from the operating system's preferred cryptographic provider.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};

/// Fills `buffer` with bytes from the system-preferred cryptographic RNG.
pub fn system_random_bytes(buffer: &mut [u8]) -> io::Result<()> {
    let length = u32::try_from(buffer.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "random byte buffer exceeds the Win32 length type",
        )
    })?;
    // SAFETY: a null algorithm handle is required with the system-preferred RNG
    // flag; `buffer` is writable for exactly `length` bytes.
    let status = unsafe {
        BCryptGenRandom(
            null_mut(),
            buffer.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "BCryptGenRandom failed with NTSTATUS {status:#x}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::system_random_bytes;

    #[test]
    fn independent_system_random_draws_are_nonzero_and_different() {
        let mut first = [0_u8; 32];
        let mut second = [0_u8; 32];
        system_random_bytes(&mut first).unwrap();
        system_random_bytes(&mut second).unwrap();
        assert!(first.iter().any(|byte| *byte != 0));
        assert!(second.iter().any(|byte| *byte != 0));
        assert_ne!(first, second);
    }
}
