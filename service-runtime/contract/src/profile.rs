use std::collections::BTreeMap;
use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize};

use crate::sha256_digest;

pub const MAX_PROFILE_BYTES: usize = 4 * 1024 * 1024;
pub const PROFILE_POINTER_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct GenerationId(String);

impl<'de> Deserialize<'de> for GenerationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

impl GenerationId {
    pub fn from_profile_bytes(bytes: &[u8]) -> Self {
        Self(sha256_digest(bytes))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, ProfileError> {
        let value = value.into();
        if canonical_digest(&value) {
            Ok(Self(value))
        } else {
            Err(ProfileError::InvalidGenerationId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn directory_name(&self) -> &str {
        &self.0[7..]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenerationPointer {
    schema: u32,
    generation: GenerationId,
}

impl GenerationPointer {
    pub const fn new(generation: GenerationId) -> Self {
        Self {
            schema: PROFILE_POINTER_SCHEMA,
            generation,
        }
    }

    pub fn generation(&self) -> &GenerationId {
        &self.generation
    }
}

impl<'de> Deserialize<'de> for GenerationPointer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawPointer {
            schema: u32,
            generation: GenerationId,
        }

        let raw = RawPointer::deserialize(deserializer)?;
        if raw.schema != PROFILE_POINTER_SCHEMA {
            return Err(de::Error::custom("unsupported profile pointer schema"));
        }
        Ok(Self::new(raw.generation))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SourceMetadata {
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PublishedProfile {
    bytes: Vec<u8>,
    source: SourceMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileCatalog {
    generations: BTreeMap<GenerationId, PublishedProfile>,
    active: Option<GenerationId>,
    previous: Option<GenerationId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileError {
    InvalidSize,
    InvalidIni,
    InvalidGenerationId,
    UnknownGeneration,
    NoPreviousGeneration,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "profile generation operation failed: {self:?}")
    }
}

impl std::error::Error for ProfileError {}

impl ProfileCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish_machine_profile(
        &mut self,
        profile_bytes: &[u8],
        source_metadata: SourceMetadata,
    ) -> Result<GenerationId, ProfileError> {
        validate_profile(profile_bytes)?;
        let generation = GenerationId::from_profile_bytes(profile_bytes);
        self.generations
            .entry(generation.clone())
            .or_insert_with(|| PublishedProfile {
                bytes: profile_bytes.to_vec(),
                source: source_metadata,
            });
        Ok(generation)
    }

    pub fn activate_machine_generation(
        &mut self,
        generation: &GenerationId,
    ) -> Result<(), ProfileError> {
        if !self.generations.contains_key(generation) {
            return Err(ProfileError::UnknownGeneration);
        }
        if self.active.as_ref() == Some(generation) {
            return Ok(());
        }

        self.previous = self.active.replace(generation.clone());
        Ok(())
    }

    pub fn rollback_machine_generation(&mut self) -> Result<GenerationId, ProfileError> {
        let previous = self
            .previous
            .clone()
            .ok_or(ProfileError::NoPreviousGeneration)?;
        let active = self.active.replace(previous.clone());
        self.previous = active;
        Ok(previous)
    }

    pub fn active(&self) -> Option<&GenerationId> {
        self.active.as_ref()
    }

    pub fn previous(&self) -> Option<&GenerationId> {
        self.previous.as_ref()
    }

    pub fn profile_bytes(&self, generation: &GenerationId) -> Option<&[u8]> {
        self.generations
            .get(generation)
            .map(|profile| profile.bytes.as_slice())
    }

    pub fn source_metadata(&self, generation: &GenerationId) -> Option<&SourceMetadata> {
        self.generations
            .get(generation)
            .map(|profile| &profile.source)
    }
}

fn validate_profile(bytes: &[u8]) -> Result<(), ProfileError> {
    if bytes.is_empty() || bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProfileError::InvalidSize);
    }

    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little_endian = bytes.starts_with(&[0xff, 0xfe]);
        let body = &bytes[2..];
        if body.len() % 2 != 0 {
            return Err(ProfileError::InvalidIni);
        }
        let units = body.chunks_exact(2).map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        let decoded =
            String::from_utf16(&units.collect::<Vec<_>>()).map_err(|_| ProfileError::InvalidIni)?;
        return validate_ini_structure(decoded.as_bytes());
    }

    let structure_bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if structure_bytes.contains(&0) {
        return Err(ProfileError::InvalidIni);
    }
    validate_ini_structure(structure_bytes)
}

fn validate_ini_structure(bytes: &[u8]) -> Result<(), ProfileError> {
    let mut saw_section = false;
    let mut saw_assignment = false;
    let mut accepts_bare_entries = false;

    for raw_line in bytes.split(|byte| *byte == b'\n') {
        let line = trim_ascii(raw_line);
        if line.is_empty() || line[0] == b';' || line[0] == b'#' {
            continue;
        }
        if line.len() >= 3 && line[0] == b'[' && line[line.len() - 1] == b']' {
            saw_section = true;
            accepts_bare_entries = is_mactype_list_section(line);
            continue;
        }
        let Some(separator) = line.iter().position(|byte| *byte == b'=') else {
            if saw_section && accepts_bare_entries {
                continue;
            }
            return Err(ProfileError::InvalidIni);
        };
        if !saw_section || trim_ascii(&line[..separator]).is_empty() {
            return Err(ProfileError::InvalidIni);
        }
        saw_assignment = true;
    }

    if saw_section && saw_assignment {
        Ok(())
    } else {
        Err(ProfileError::InvalidIni)
    }
}

fn is_mactype_list_section(line: &[u8]) -> bool {
    let name = &line[1..line.len() - 1];
    [
        b"Exclude".as_slice(),
        b"Include".as_slice(),
        b"ExcludeModule".as_slice(),
        b"IncludeModule".as_slice(),
        b"UnloadDLL".as_slice(),
        b"ExcludeSub".as_slice(),
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
