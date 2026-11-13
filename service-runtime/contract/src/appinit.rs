use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppInitValueError;

impl fmt::Display for AppInitValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("malformed AppInit_DLLs string value")
    }
}

impl std::error::Error for AppInitValueError {}

pub fn appinit_mactype_conflict(
    enabled: bool,
    value: Option<&[u16]>,
) -> Result<bool, AppInitValueError> {
    if !enabled {
        return Ok(false);
    }
    let Some(value) = value else {
        return Ok(false);
    };
    let Some((&0, content)) = value.split_last() else {
        return Err(AppInitValueError);
    };
    if content.contains(&0) {
        return Err(AppInitValueError);
    }
    let decoded = String::from_utf16(content).map_err(|_| AppInitValueError)?;
    Ok(decoded
        .split(|character: char| {
            character.is_whitespace() || matches!(character, ',' | ';' | '"' | '\'')
        })
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.rsplit(['\\', '/']).next())
        .any(|basename| {
            let basename = basename.to_ascii_lowercase();
            basename.starts_with("mactype") && basename.ends_with(".dll")
        }))
}
