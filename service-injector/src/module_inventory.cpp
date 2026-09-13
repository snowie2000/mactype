#include "module_inventory.h"

#include <psapi.h>

#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cwchar>
#include <filesystem>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace mactype::injector {
namespace {

constexpr std::size_t kMaxModules = 4'096U;
constexpr std::size_t kMaxModulePathCharacters = 32'768U;

[[nodiscard]] bool starts_with_case_insensitive(const std::wstring_view text,
                                                const std::wstring_view prefix) noexcept {
    return text.size() >= prefix.size() &&
           _wcsnicmp(text.data(), prefix.data(), prefix.size()) == 0;
}

[[nodiscard]] std::optional<std::wstring> normalized_module_path(
    const std::wstring_view input) noexcept {
    if (input.empty() || input.find(L'\0') != std::wstring_view::npos) {
        return std::nullopt;
    }
    try {
        std::wstring normalized;
        if (starts_with_case_insensitive(input, LR"(\\?\UNC\)") ||
            starts_with_case_insensitive(input, LR"(\??\UNC\)")) {
            normalized = LR"(\\)";
            normalized.append(input.substr(8U));
        } else if (starts_with_case_insensitive(input, LR"(\\?\)") ||
                   starts_with_case_insensitive(input, LR"(\??\)")) {
            normalized.assign(input.substr(4U));
        } else {
            normalized.assign(input);
        }
        const std::filesystem::path path{normalized};
        if (!path.is_absolute()) {
            return std::nullopt;
        }
        return path.lexically_normal().native();
    } catch (...) {
        return std::nullopt;
    }
}

// Runs `attempt` until it stops failing transiently. `failed` reports whether a
// result is an inventory failure; the Win32 error of that failure is captured
// before any wait can overwrite it and is restored as the last error when the
// bound is exhausted, so callers keep reading `GetLastError()` as before.
template <typename Attempt, typename Failed>
[[nodiscard]] auto retry_transient_inventory(const HANDLE process, Attempt attempt,
                                             Failed failed) noexcept {
    const auto started = std::chrono::steady_clock::now();
    for (;;) {
        auto result = attempt();
        if (!failed(result)) {
            return result;
        }
        const DWORD error = GetLastError();
        const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - started);
        const bool target_signaled = WaitForSingleObject(process, 0U) == WAIT_OBJECT_0;
        if (inventory_retry_action(error, elapsed, target_signaled) == InventoryRetry::GiveUp) {
            SetLastError(error);
            return result;
        }
        Sleep(static_cast<DWORD>(kInventoryRetryStep.count()));
    }
}

[[nodiscard]] std::optional<std::vector<HMODULE>> enumerate_modules(
    HANDLE process) noexcept {
    try {
        std::vector<HMODULE> modules(kMaxModules);
        DWORD needed = 0U;
        const auto capacity = static_cast<DWORD>(modules.size() * sizeof(HMODULE));
        if (!K32EnumProcessModulesEx(process, modules.data(), capacity, &needed,
                                     LIST_MODULES_ALL) ||
            needed > capacity || needed % sizeof(HMODULE) != 0U) {
            return std::nullopt;
        }
        modules.resize(needed / sizeof(HMODULE));
        return modules;
    } catch (...) {
        return std::nullopt;
    }
}

[[nodiscard]] std::optional<std::wstring_view> module_path(
    HANDLE process, const HMODULE module, std::vector<wchar_t>& path) noexcept {
    const DWORD length = K32GetModuleFileNameExW(
        process, module, path.data(), static_cast<DWORD>(path.size()));
    if (length == 0U || length >= path.size()) {
        return std::nullopt;
    }
    return std::wstring_view{path.data(), length};
}

[[nodiscard]] std::optional<std::uintptr_t> remote_module_base(
    HANDLE process, const std::wstring_view module_name) noexcept {
    const auto inventory = retry_transient_inventory(
        process, [process]() noexcept { return enumerate_modules(process); },
        [](const std::optional<std::vector<HMODULE>>& modules) noexcept {
            return !modules.has_value();
        });
    if (!inventory) {
        return std::nullopt;
    }
    try {
        std::vector<wchar_t> path(kMaxModulePathCharacters);
        for (const HMODULE module : *inventory) {
            const auto current_path = module_path(process, module, path);
            if (!current_path) {
                return std::nullopt;
            }
            const std::filesystem::path parsed{*current_path};
            if (_wcsicmp(parsed.filename().c_str(), module_name.data()) == 0) {
                return reinterpret_cast<std::uintptr_t>(module);
            }
        }
    } catch (...) {
        return std::nullopt;
    }
    return std::nullopt;
}

bool module_paths_equal_impl(const std::wstring_view left,
                             const std::wstring_view right) noexcept {
    const auto normalized_left = normalized_module_path(left);
    const auto normalized_right = normalized_module_path(right);
    return normalized_left && normalized_right &&
           _wcsicmp(normalized_left->c_str(), normalized_right->c_str()) == 0;
}

[[nodiscard]] FixedModuleState fixed_module_state_once(
    HANDLE process, const std::filesystem::path& expected_path) noexcept {
    const auto normalized_expected = normalized_module_path(expected_path.native());
    if (!normalized_expected) {
        return FixedModuleState::InventoryUnavailable;
    }
    const auto inventory = enumerate_modules(process);
    if (!inventory) {
        return FixedModuleState::InventoryUnavailable;
    }
    try {
        const std::filesystem::path expected{*normalized_expected};
        if (expected.filename().empty()) {
            return FixedModuleState::InventoryUnavailable;
        }
        bool expected_module_loaded = false;
        bool conflicting_module_loaded = false;
        std::vector<wchar_t> path(kMaxModulePathCharacters);
        for (const HMODULE module : *inventory) {
            const auto current_path = module_path(process, module, path);
            if (!current_path) {
                return FixedModuleState::InventoryUnavailable;
            }
            const auto normalized_current = normalized_module_path(*current_path);
            if (!normalized_current) {
                return FixedModuleState::InventoryUnavailable;
            }
            const std::filesystem::path current{*normalized_current};
            if (_wcsicmp(current.filename().c_str(), expected.filename().c_str()) != 0) {
                continue;
            }
            if (_wcsicmp(normalized_current->c_str(), normalized_expected->c_str()) == 0) {
                expected_module_loaded = true;
            } else {
                conflicting_module_loaded = true;
            }
        }
        if (conflicting_module_loaded) {
            return FixedModuleState::SameBasenameDifferentPath;
        }
        return expected_module_loaded ? FixedModuleState::ExpectedModuleLoaded
                                      : FixedModuleState::Absent;
    } catch (...) {
        return FixedModuleState::InventoryUnavailable;
    }
}

}  // namespace

bool inventory_failure_is_transient(const DWORD error) noexcept {
    return error == ERROR_PARTIAL_COPY;
}

InventoryRetry inventory_retry_action(const DWORD error,
                                      const std::chrono::milliseconds elapsed,
                                      const bool target_signaled) noexcept {
    if (target_signaled || !inventory_failure_is_transient(error) ||
        elapsed >= kInventoryRetryBudget) {
        return InventoryRetry::GiveUp;
    }
    return InventoryRetry::Retry;
}

bool module_paths_equal(const std::wstring_view left,
                        const std::wstring_view right) noexcept {
    return module_paths_equal_impl(left, right);
}

FixedModuleState fixed_module_state(
    HANDLE process, const std::filesystem::path& expected_path) noexcept {
    return retry_transient_inventory(
        process,
        [process, &expected_path]() noexcept {
            return fixed_module_state_once(process, expected_path);
        },
        [](const FixedModuleState state) noexcept {
            return state == FixedModuleState::InventoryUnavailable;
        });
}

std::optional<LPTHREAD_START_ROUTINE> remote_load_library(HANDLE process) noexcept {
    const auto local_kernel = GetModuleHandleW(L"kernel32.dll");
    if (local_kernel == nullptr) {
        return std::nullopt;
    }
    const auto local_load_library = GetProcAddress(local_kernel, "LoadLibraryW");
    if (local_load_library == nullptr) {
        return std::nullopt;
    }

    MEMORY_BASIC_INFORMATION implementation{};
    if (VirtualQuery(reinterpret_cast<const void*>(local_load_library), &implementation,
                     sizeof(implementation)) != sizeof(implementation) ||
        implementation.AllocationBase == nullptr) {
        return std::nullopt;
    }
    const auto implementation_module = static_cast<HMODULE>(implementation.AllocationBase);
    std::array<wchar_t, MAX_PATH> implementation_path{};
    const DWORD length = GetModuleFileNameW(implementation_module, implementation_path.data(),
                                            static_cast<DWORD>(implementation_path.size()));
    if (length == 0U || length >= implementation_path.size()) {
        return std::nullopt;
    }
    const wchar_t* filename = std::wcsrchr(implementation_path.data(), L'\\');
    filename = filename == nullptr ? implementation_path.data() : filename + 1;
    const auto remote_implementation = remote_module_base(process, filename);
    if (!remote_implementation) {
        return std::nullopt;
    }
    const auto offset = reinterpret_cast<std::uintptr_t>(local_load_library) -
                        reinterpret_cast<std::uintptr_t>(implementation.AllocationBase);
    return reinterpret_cast<LPTHREAD_START_ROUTINE>(*remote_implementation + offset);
}

}  // namespace mactype::injector
