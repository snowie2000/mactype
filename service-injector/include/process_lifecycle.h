#pragma once

#include <windows.h>

#include <cstdint>
#include <optional>

namespace mactype::injector {

struct ProcessLifecycle final {
    bool frozen{};
    bool deleting{};
};

// Decodes IsProcessDeleting (0x4) and IsFrozen (0x10) from the
// PROCESS_EXTENDED_BASIC_INFORMATION flags word. Other flags are ignored.
[[nodiscard]] ProcessLifecycle decode_process_lifecycle_flags(
    std::uint32_t flags) noexcept;

// Returns no value when extended process information cannot be obtained.
[[nodiscard]] std::optional<ProcessLifecycle> query_process_lifecycle(
    HANDLE process) noexcept;

}  // namespace mactype::injector
