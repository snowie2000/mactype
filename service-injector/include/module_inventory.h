#pragma once

#include <windows.h>

#include <chrono>
#include <filesystem>
#include <optional>
#include <string_view>

namespace mactype::injector {

enum class FixedModuleState {
    Absent,
    ExpectedModuleLoaded,
    SameBasenameDifferentPath,
    InventoryUnavailable,
};

// A module inventory read races the target's own loader: while the target is
// mapping or unmapping an image its loader list is briefly inconsistent and
// the read fails with ERROR_PARTIAL_COPY. That state resolves within
// milliseconds, so inventory callers retry it inside a fixed bound instead of
// reporting a target that is merely busy as unreadable.
enum class InventoryRetry {
    Retry,
    GiveUp,
};

constexpr std::chrono::milliseconds kInventoryRetryBudget{500};
constexpr std::chrono::milliseconds kInventoryRetryStep{10};

[[nodiscard]] bool inventory_failure_is_transient(DWORD error) noexcept;
[[nodiscard]] InventoryRetry inventory_retry_action(DWORD error,
                                                    std::chrono::milliseconds elapsed,
                                                    bool target_signaled) noexcept;

[[nodiscard]] bool module_paths_equal(std::wstring_view left,
                                      std::wstring_view right) noexcept;
[[nodiscard]] FixedModuleState fixed_module_state(
    HANDLE process, const std::filesystem::path& expected_path) noexcept;
[[nodiscard]] std::optional<LPTHREAD_START_ROUTINE> remote_load_library(
    HANDLE process) noexcept;

}  // namespace mactype::injector
