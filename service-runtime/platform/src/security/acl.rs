//! SIDs and DACLs as validated byte structures.

use std::ffi::c_void;
use std::io;
use std::mem::{offset_of, size_of};
use std::ptr::{self, NonNull};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::{
    GetLengthSid, IsValidSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, PSID,
};

use crate::wide::wide_null;

const ACCESS_ALLOWED_ACE_KIND: u8 = 0;
const SID_FIXED_BYTES: usize = 8;
const SID_SUB_AUTHORITY_BYTES: usize = size_of::<u32>();
const ACE_HEADER_BYTES: usize = size_of::<ACE_HEADER>();
const ACL_HEADER_BYTES: usize = size_of::<ACL>();
const ACCESS_ALLOWED_SID_OFFSET: usize = offset_of!(ACCESS_ALLOWED_ACE, SidStart);

/// Why a descriptor's bytes did not form the structure Windows claims.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AclValidationError {
    DescriptorRange,
    MissingDacl,
    MissingOwner,
    AclTooShort,
    InvalidAclSize,
    TruncatedAceHeader,
    InvalidAceSize,
    AceOutOfRange,
    UnsupportedAceType,
    TruncatedSid,
    InvalidSid,
}

#[derive(Debug)]
pub enum SecurityAclError {
    Io(io::Error),
    Invalid(AclValidationError),
}

impl From<io::Error> for SecurityAclError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<AclValidationError> for SecurityAclError {
    fn from(value: AclValidationError) -> Self {
        Self::Invalid(value)
    }
}

/// A SID converted from its string form, kept both as the Windows allocation
/// (for APIs that need a pointer) and as validated bytes (for comparison).
#[derive(Debug)]
pub struct OwnedSid {
    raw: NonNull<c_void>,
    bytes: Vec<u8>,
}

impl OwnedSid {
    pub fn from_string(value: &str) -> io::Result<Self> {
        let value = wide_null(value);
        let mut sid = ptr::null_mut();
        // SAFETY: the SDDL string is NUL-terminated and `sid` is a live out
        // pointer; on success the LocalAlloc block is owned below.
        if unsafe { ConvertStringSidToSidW(value.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err(io::Error::last_os_error());
        }
        let raw = NonNull::new(sid).expect("checked non-null SID");
        let free = |raw: NonNull<c_void>| {
            // SAFETY: the failed validation path still uniquely owns the block.
            unsafe { LocalFree(raw.as_ptr()) };
        };
        // SAFETY: `raw` was just returned by the converter.
        if unsafe { IsValidSid(raw.as_ptr()) } == 0 {
            free(raw);
            return Err(invalid_data("Windows returned an invalid converted SID"));
        }
        // SAFETY: IsValidSid accepted this block.
        let length = unsafe { GetLengthSid(raw.as_ptr()) } as usize;
        if length < SID_FIXED_BYTES {
            free(raw);
            return Err(invalid_data("Windows returned a truncated converted SID"));
        }
        // SAFETY: GetLengthSid reported the initialized length of this block.
        let bytes =
            unsafe { std::slice::from_raw_parts(raw.as_ptr().cast::<u8>(), length) }.to_vec();
        Ok(Self { raw, bytes })
    }

    pub(crate) fn as_ptr(&self) -> PSID {
        self.raw.as_ptr()
    }
}

impl Drop for OwnedSid {
    fn drop(&mut self) {
        // SAFETY: this value uniquely owns the converter's LocalAlloc block.
        unsafe { LocalFree(self.raw.as_ptr()) };
    }
}

/// A SID copied out of a descriptor after its structure was verified.
#[derive(Debug)]
pub struct ValidatedSid {
    bytes: Vec<u8>,
}

impl ValidatedSid {
    pub(crate) fn from_bounded_bytes(bytes: &[u8]) -> Result<Self, AclValidationError> {
        if bytes.len() < SID_FIXED_BYTES {
            return Err(AclValidationError::TruncatedSid);
        }
        let sub_authorities = usize::from(bytes[1]);
        let length = sub_authorities
            .checked_mul(SID_SUB_AUTHORITY_BYTES)
            .and_then(|tail| SID_FIXED_BYTES.checked_add(tail))
            .ok_or(AclValidationError::TruncatedSid)?;
        if length > bytes.len() {
            return Err(AclValidationError::TruncatedSid);
        }
        // Windows SID routines expect an aligned SID. Copying the bounded
        // bytes into u32 storage keeps that invariant local to this function.
        let mut aligned = vec![0_u32; length.div_ceil(size_of::<u32>())];
        // SAFETY: the byte view covers exactly the initialized u32 allocation,
        // does not outlive `aligned`, and keeps its four-byte alignment.
        let aligned_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                aligned.as_mut_ptr().cast::<u8>(),
                aligned.len() * size_of::<u32>(),
            )
        };
        aligned_bytes[..length].copy_from_slice(&bytes[..length]);
        let sid = aligned.as_mut_ptr().cast::<c_void>();
        // SAFETY: the structural check above proved every sub-authority byte
        // is present in this aligned allocation.
        if unsafe { IsValidSid(sid) } == 0 {
            return Err(AclValidationError::InvalidSid);
        }
        // SAFETY: IsValidSid accepted the bounded, aligned SID.
        let windows_length = unsafe { GetLengthSid(sid) } as usize;
        if windows_length != length {
            return Err(AclValidationError::InvalidSid);
        }
        Ok(Self {
            bytes: bytes[..length].to_vec(),
        })
    }

    pub fn matches(&self, expected: &OwnedSid) -> bool {
        self.bytes == expected.bytes
    }
}

/// One `ACCESS_ALLOWED_ACE`, reduced to the fields policy checks read.
#[derive(Debug)]
pub struct AllowedAce {
    flags: u8,
    mask: u32,
    trustee: ValidatedSid,
}

impl AllowedAce {
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    pub const fn mask(&self) -> u32 {
        self.mask
    }

    pub fn trustee_matches(&self, expected: &OwnedSid) -> bool {
        self.trustee.matches(expected)
    }
}

/// A DACL whose every ACE was an in-bounds `ACCESS_ALLOWED_ACE` with a
/// well-formed SID. Any other ACE type fails validation, so a policy that
/// iterates this list has already excluded deny and audit entries.
#[derive(Debug)]
pub struct ValidatedDacl {
    allowed_aces: Vec<AllowedAce>,
}

impl ValidatedDacl {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, AclValidationError> {
        if bytes.len() < ACL_HEADER_BYTES {
            return Err(AclValidationError::AclTooShort);
        }
        let acl_size = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
        if acl_size < ACL_HEADER_BYTES || acl_size > bytes.len() {
            return Err(AclValidationError::InvalidAclSize);
        }
        let ace_count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
        let bytes = &bytes[..acl_size];
        if ace_count > (bytes.len() - ACL_HEADER_BYTES) / ACE_HEADER_BYTES {
            return Err(AclValidationError::TruncatedAceHeader);
        }
        let mut offset = ACL_HEADER_BYTES;
        let mut allowed_aces = Vec::with_capacity(ace_count);
        for _ in 0..ace_count {
            let header_end = offset
                .checked_add(ACE_HEADER_BYTES)
                .filter(|end| *end <= bytes.len())
                .ok_or(AclValidationError::TruncatedAceHeader)?;
            let ace_type = bytes[offset];
            let flags = bytes[offset + 1];
            let ace_size = usize::from(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]));
            if ace_size < ACE_HEADER_BYTES || ace_size % size_of::<u32>() != 0 {
                return Err(AclValidationError::InvalidAceSize);
            }
            let ace_end = offset
                .checked_add(ace_size)
                .filter(|end| *end <= bytes.len())
                .ok_or(AclValidationError::AceOutOfRange)?;
            debug_assert!(header_end <= ace_end);
            if ace_type != ACCESS_ALLOWED_ACE_KIND {
                return Err(AclValidationError::UnsupportedAceType);
            }
            if ace_size < ACCESS_ALLOWED_SID_OFFSET + SID_FIXED_BYTES {
                return Err(AclValidationError::InvalidAceSize);
            }
            let mask_offset = offset + offset_of!(ACCESS_ALLOWED_ACE, Mask);
            let mask = u32::from_le_bytes(
                bytes[mask_offset..mask_offset + size_of::<u32>()]
                    .try_into()
                    .expect("validated access mask range"),
            );
            let trustee = ValidatedSid::from_bounded_bytes(
                &bytes[offset + ACCESS_ALLOWED_SID_OFFSET..ace_end],
            )?;
            allowed_aces.push(AllowedAce {
                flags,
                mask,
                trustee,
            });
            offset = ace_end;
        }
        Ok(Self { allowed_aces })
    }

    pub fn len(&self) -> usize {
        self.allowed_aces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.allowed_aces.is_empty()
    }

    pub fn allowed_aces(&self) -> impl Iterator<Item = &AllowedAce> {
        self.allowed_aces.iter()
    }
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::{AclValidationError, OwnedSid, ValidatedDacl};

    const SYSTEM_SID_BYTES: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0];

    fn valid_acl() -> Vec<u8> {
        let ace_size = 8 + SYSTEM_SID_BYTES.len();
        let acl_size = 8 + ace_size;
        let mut acl = vec![0_u8; acl_size];
        acl[0] = 2;
        acl[2..4].copy_from_slice(&(acl_size as u16).to_le_bytes());
        acl[4..6].copy_from_slice(&1_u16.to_le_bytes());
        acl[9] = 0x10;
        acl[10..12].copy_from_slice(&(ace_size as u16).to_le_bytes());
        acl[12..16].copy_from_slice(&0x001f_0001_u32.to_le_bytes());
        acl[16..].copy_from_slice(&SYSTEM_SID_BYTES);
        acl
    }

    #[test]
    fn a_valid_allowed_ace_exposes_only_validated_fields() {
        let dacl = ValidatedDacl::from_bytes(&valid_acl()).unwrap();
        let system = OwnedSid::from_string("S-1-5-18").unwrap();
        let ace = dacl.allowed_aces().next().unwrap();
        assert_eq!(dacl.len(), 1);
        assert_eq!(ace.flags(), 0x10);
        assert_eq!(ace.mask(), 0x001f_0001);
        assert!(ace.trustee_matches(&system));
    }

    #[test]
    fn malformed_acls_are_rejected_at_the_first_inconsistent_field() {
        assert_eq!(
            ValidatedDacl::from_bytes(&[0; 7]).unwrap_err(),
            AclValidationError::AclTooShort
        );
        let mut undersized = valid_acl();
        undersized[2..4].copy_from_slice(&7_u16.to_le_bytes());
        assert_eq!(
            ValidatedDacl::from_bytes(&undersized).unwrap_err(),
            AclValidationError::InvalidAclSize
        );
        let mut short_ace = valid_acl();
        short_ace[10..12].copy_from_slice(&12_u16.to_le_bytes());
        assert_eq!(
            ValidatedDacl::from_bytes(&short_ace).unwrap_err(),
            AclValidationError::InvalidAceSize
        );
        let mut long_ace = valid_acl();
        let ace_size = long_ace.len() as u16;
        long_ace[10..12].copy_from_slice(&ace_size.to_le_bytes());
        assert_eq!(
            ValidatedDacl::from_bytes(&long_ace).unwrap_err(),
            AclValidationError::AceOutOfRange
        );
        let mut truncated_sid = valid_acl();
        truncated_sid[10..12].copy_from_slice(&16_u16.to_le_bytes());
        assert_eq!(
            ValidatedDacl::from_bytes(&truncated_sid).unwrap_err(),
            AclValidationError::TruncatedSid
        );
        let mut invalid_sid = valid_acl();
        invalid_sid[16] = 0;
        assert_eq!(
            ValidatedDacl::from_bytes(&invalid_sid).unwrap_err(),
            AclValidationError::InvalidSid
        );
        let mut deny = valid_acl();
        deny[8] = 1;
        assert_eq!(
            ValidatedDacl::from_bytes(&deny).unwrap_err(),
            AclValidationError::UnsupportedAceType
        );
    }
}
