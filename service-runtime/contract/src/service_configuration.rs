use std::path::{Component, Path};

use crate::valid_runtime_version_component;

pub const FIXED_SERVICE_TYPE: u32 = 0x10;
pub const FIXED_START_TYPE: u32 = 2;
pub const FIXED_ERROR_CONTROL: u32 = 1;
pub const FIXED_ACCOUNT: &str = "LocalSystem";
pub const SERVICE_DISPLAY_NAME: &str = "MacType Control Center Service";

#[derive(Clone, Copy, Debug)]
pub struct ObservedServiceConfiguration<'a> {
    pub service_type: u32,
    pub start_type: u32,
    pub error_control: u32,
    pub image_path: &'a str,
    pub account: &'a str,
    pub display_name: &'a str,
    pub load_order_group: &'a str,
    pub tag_id: u32,
    pub dependencies: &'a [String],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceConfigurationDrift {
    pub start_type: bool,
    pub error_control: bool,
    pub display_name: bool,
    pub load_order_group: bool,
    pub tag: bool,
    pub dependencies: bool,
}

impl ServiceConfigurationDrift {
    pub const fn is_drifted(self) -> bool {
        self.start_type
            || self.error_control
            || self.display_name
            || self.load_order_group
            || self.tag
            || self.dependencies
    }

    pub fn field_names(self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.start_type {
            fields.push("start-type");
        }
        if self.error_control {
            fields.push("error-control");
        }
        if self.display_name {
            fields.push("display-name");
        }
        if self.load_order_group {
            fields.push("load-order-group");
        }
        if self.tag {
            fields.push("tag");
        }
        if self.dependencies {
            fields.push("dependencies");
        }
        fields
    }
}

pub fn service_image_matches_protected_layout(protected_root: &Path, image_path: &str) -> bool {
    let Some(binary) = service_binary_from_image_path(image_path) else {
        return false;
    };
    service_binary_matches_protected_layout(protected_root, binary)
}

pub fn service_image_matches_protected_contract(protected_root: &Path, image_path: &str) -> bool {
    if !service_image_matches_protected_layout(protected_root, image_path) {
        return false;
    }
    let Some(binary) = service_binary_from_image_path(image_path) else {
        return false;
    };
    if !binary.is_file() {
        return false;
    }
    let Ok(root) = protected_root.canonicalize() else {
        return false;
    };
    let Ok(binary) = binary.canonicalize() else {
        return false;
    };
    binary.strip_prefix(root).is_ok()
}

fn service_binary_from_image_path(image_path: &str) -> Option<&Path> {
    let rest = image_path.strip_prefix('"')?;
    let end_quote = rest.find('"')?;
    if &rest[end_quote + 1..] != " --service" {
        return None;
    }
    Some(Path::new(&rest[..end_quote]))
}

fn service_binary_matches_protected_layout(protected_root: &Path, binary: &Path) -> bool {
    if !protected_root.is_absolute() || !binary.is_absolute() {
        return false;
    }
    let root = protected_root.components().collect::<Vec<_>>();
    let binary = binary.components().collect::<Vec<_>>();
    if binary.len() != root.len() + 3
        || !root
            .iter()
            .zip(binary.iter())
            .all(|(expected, observed)| components_equal(expected, observed))
    {
        return false;
    }
    let tail = &binary[root.len()..];
    component_text(tail[0]).is_some_and(|value| value.eq_ignore_ascii_case("bin"))
        && component_text(tail[1]).is_some_and(valid_runtime_version_component)
        && component_text(tail[2])
            .is_some_and(|value| value.eq_ignore_ascii_case("mactype-service.exe"))
}

fn components_equal(left: &Component<'_>, right: &Component<'_>) -> bool {
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

fn component_text(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }
}

pub fn owned_service_identity(
    observed: &ObservedServiceConfiguration<'_>,
    protected_root: &Path,
) -> bool {
    observed.service_type == FIXED_SERVICE_TYPE
        && observed.account.eq_ignore_ascii_case(FIXED_ACCOUNT)
        && service_image_matches_protected_layout(protected_root, observed.image_path)
}

pub fn service_configuration_drift(
    observed: &ObservedServiceConfiguration<'_>,
) -> ServiceConfigurationDrift {
    ServiceConfigurationDrift {
        start_type: observed.start_type != FIXED_START_TYPE,
        error_control: observed.error_control != FIXED_ERROR_CONTROL,
        display_name: observed.display_name != SERVICE_DISPLAY_NAME,
        load_order_group: !observed.load_order_group.is_empty(),
        tag: observed.tag_id != 0 && !observed.load_order_group.is_empty(),
        dependencies: !observed.dependencies.is_empty(),
    }
}
