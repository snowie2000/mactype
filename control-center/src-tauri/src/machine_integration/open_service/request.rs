use serde::Deserialize;
use std::ffi::{OsStr, OsString};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SystemServiceAction {
    Install,
    Upgrade,
    Repair,
    Remove,
    Start,
    Stop,
    PublishProfile,
    MigrateFromLegacy,
    Rollback,
    RemoveLegacy,
    DisableLegacyTrayAutostart,
    RestoreLegacyTrayAutostart,
}

impl SystemServiceAction {
    pub(super) fn setup_verb(self) -> Option<&'static str> {
        match self {
            Self::Install => Some("install"),
            Self::Upgrade => Some("upgrade"),
            Self::Repair => Some("repair"),
            Self::Remove => Some("remove"),
            Self::Start => Some("start"),
            Self::Stop => Some("stop"),
            Self::PublishProfile => Some("publish-profile"),
            Self::Rollback => Some("rollback"),
            Self::MigrateFromLegacy
            | Self::RemoveLegacy
            | Self::DisableLegacyTrayAutostart
            | Self::RestoreLegacyTrayAutostart => None,
        }
    }

    pub(super) fn broker_verb(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Upgrade => "upgrade",
            Self::Repair => "repair",
            Self::Remove => "remove",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::PublishProfile => "publish-profile",
            Self::MigrateFromLegacy => "migrate-from-legacy",
            Self::Rollback => "rollback",
            Self::RemoveLegacy => "remove-legacy",
            Self::DisableLegacyTrayAutostart => "disable-legacy-tray-autostart",
            Self::RestoreLegacyTrayAutostart => "restore-legacy-tray-autostart",
        }
    }

    pub(super) fn from_broker_verb(value: &str) -> Option<Self> {
        Some(match value {
            "install" => Self::Install,
            "upgrade" => Self::Upgrade,
            "repair" => Self::Repair,
            "remove" => Self::Remove,
            "start" => Self::Start,
            "stop" => Self::Stop,
            "publish-profile" => Self::PublishProfile,
            "migrate-from-legacy" => Self::MigrateFromLegacy,
            "rollback" => Self::Rollback,
            "remove-legacy" => Self::RemoveLegacy,
            "disable-legacy-tray-autostart" => Self::DisableLegacyTrayAutostart,
            "restore-legacy-tray-autostart" => Self::RestoreLegacyTrayAutostart,
            _ => return None,
        })
    }

    pub(super) fn needs_profile_input(self) -> bool {
        matches!(
            self,
            Self::PublishProfile | Self::MigrateFromLegacy | Self::RemoveLegacy
        )
    }
}

pub(super) const BROKER_SWITCH: &str = "--control-center-service-broker";
pub(super) const BROKER_TRANSFER_SWITCH: &str = "--broker-transfer-v1";
pub(super) const PROFILE_TRANSFER_NONCE_BYTES: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProfileTransferToken {
    pub(super) server_pid: u32,
    pub(super) nonce: [u8; PROFILE_TRANSFER_NONCE_BYTES],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PrivilegedRequest {
    pub(super) action: SystemServiceAction,
    pub(super) transfer: ProfileTransferToken,
}

pub(super) fn parse_profile_transfer_nonce(
    value: &str,
) -> Result<[u8; PROFILE_TRANSFER_NONCE_BYTES], String> {
    if value.len() != PROFILE_TRANSFER_NONCE_BYTES * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("the profile transfer nonce is not canonical".to_owned());
    }
    let mut nonce = [0_u8; PROFILE_TRANSFER_NONCE_BYTES];
    for (index, output) in nonce.iter_mut().enumerate() {
        let pair = &value[index * 2..index * 2 + 2];
        *output = u8::from_str_radix(pair, 16)
            .map_err(|_| "the profile transfer nonce is not canonical".to_owned())?;
    }
    Ok(nonce)
}

pub(super) const PROFILE_TRANSFER_MAGIC: &[u8; 4] = b"MTPT";
pub(super) const PROFILE_TRANSFER_VERSION: u16 = 1;
pub(super) const PROFILE_TRANSFER_HEADER_BYTES: usize =
    4 + 2 + 2 + 4 + PROFILE_TRANSFER_NONCE_BYTES + 32;

pub(super) fn profile_transfer_digest(bytes: &[u8]) -> Result<[u8; 32], String> {
    let digest = mactype_service_contract::sha256_digest(bytes);
    let hex = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| "profile transfer digest is invalid".to_owned())?;
    if hex.len() != 64 {
        return Err("profile transfer digest is invalid".to_owned());
    }
    let mut result = [0_u8; 32];
    for (index, output) in result.iter_mut().enumerate() {
        *output = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "profile transfer digest is invalid".to_owned())?;
    }
    Ok(result)
}

pub(super) fn encode_profile_transfer_frame(
    profile: &[u8],
    nonce: &[u8; PROFILE_TRANSFER_NONCE_BYTES],
) -> Result<Vec<u8>, String> {
    if profile.is_empty() || profile.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err("profile transfer payload size is outside the allowed range".to_owned());
    }
    let mut frame = Vec::with_capacity(PROFILE_TRANSFER_HEADER_BYTES + profile.len());
    frame.extend_from_slice(PROFILE_TRANSFER_MAGIC);
    frame.extend_from_slice(&PROFILE_TRANSFER_VERSION.to_le_bytes());
    frame.extend_from_slice(&0_u16.to_le_bytes());
    frame.extend_from_slice(&(profile.len() as u32).to_le_bytes());
    frame.extend_from_slice(nonce);
    frame.extend_from_slice(&profile_transfer_digest(profile)?);
    frame.extend_from_slice(profile);
    Ok(frame)
}

pub(super) fn decode_profile_transfer_frame(
    frame: &[u8],
    expected_nonce: &[u8; PROFILE_TRANSFER_NONCE_BYTES],
) -> Result<Vec<u8>, String> {
    if frame.len() < PROFILE_TRANSFER_HEADER_BYTES
        || &frame[..4] != PROFILE_TRANSFER_MAGIC
        || u16::from_le_bytes(frame[4..6].try_into().expect("fixed frame version"))
            != PROFILE_TRANSFER_VERSION
        || u16::from_le_bytes(frame[6..8].try_into().expect("fixed reserved field")) != 0
        || &frame[12..28] != expected_nonce
    {
        return Err("profile transfer frame header is invalid".to_owned());
    }
    let payload_len =
        u32::from_le_bytes(frame[8..12].try_into().expect("fixed frame length")) as usize;
    if payload_len == 0
        || payload_len > mactype_service_contract::MAX_PROFILE_BYTES
        || frame.len() != PROFILE_TRANSFER_HEADER_BYTES + payload_len
    {
        return Err("profile transfer frame length is invalid".to_owned());
    }
    let payload = &frame[PROFILE_TRANSFER_HEADER_BYTES..];
    if frame[28..60] != profile_transfer_digest(payload)? {
        return Err("profile transfer frame digest does not match".to_owned());
    }
    Ok(payload.to_vec())
}

pub(super) fn privileged_request_from_arguments<I>(
    arguments: I,
) -> Result<Option<PrivilegedRequest>, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut arguments = arguments.into_iter();
    let _executable = arguments.next();
    let Some(switch) = arguments.next() else {
        return Ok(None);
    };
    if switch != OsStr::new(BROKER_SWITCH) {
        return Ok(None);
    }
    let verb = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "the elevated service broker requires one fixed verb".to_owned())?;
    let action = SystemServiceAction::from_broker_verb(&verb)
        .ok_or_else(|| "unsupported elevated service broker verb".to_owned())?;
    let remaining = arguments.collect::<Vec<_>>();
    if remaining.len() != 3 || remaining[0] != OsStr::new(BROKER_TRANSFER_SWITCH) {
        return Err("service broker actions require one versioned transfer token".to_owned());
    }
    let pid_text = remaining[1]
        .to_str()
        .ok_or_else(|| "the broker transfer PID is not Unicode".to_owned())?;
    let server_pid = pid_text
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid != 0 && pid.to_string() == pid_text)
        .ok_or_else(|| "the broker transfer PID is not canonical".to_owned())?;
    let nonce_text = remaining[2]
        .to_str()
        .ok_or_else(|| "the broker transfer nonce is not Unicode".to_owned())?;
    let transfer = ProfileTransferToken {
        server_pid,
        nonce: parse_profile_transfer_nonce(nonce_text)?,
    };
    Ok(Some(PrivilegedRequest { action, transfer }))
}
