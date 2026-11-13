#pragma once

#include <windows.h>

#include <optional>

namespace mactype::injector {

/// The access mask the kernel actually granted this handle. A product that
/// defends its own processes can register a kernel callback that strips rights
/// while `OpenProcess` still succeeds, so the mask that was asked for is not
/// the mask that is held, and the first call needing a stripped right fails
/// with ERROR_ACCESS_DENIED far from the cause.
[[nodiscard]] std::optional<ACCESS_MASK> granted_access(HANDLE object) noexcept;

/// Whether a granted mask still carries every right remote injection needs.
[[nodiscard]] bool injection_rights_present(ACCESS_MASK granted) noexcept;

}  // namespace mactype::injector
