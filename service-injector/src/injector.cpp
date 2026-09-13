#include "injector.h"

#include "fixed_module.h"
#include "module_inventory.h"
#include "process_lifecycle.h"
#include "remote_injection.h"
#include "safety_policy.h"
#include "unique_handle.h"

#include <windows.h>

#include <cstdint>
#include <filesystem>
#include <limits>
#include <optional>

namespace mactype::injector {
namespace {

#ifdef _WIN64
constexpr USHORT kExpectedMachine = IMAGE_FILE_MACHINE_AMD64;
#else
constexpr USHORT kExpectedMachine = IMAGE_FILE_MACHINE_I386;
#endif

[[nodiscard]] std::uint64_t as_integer(const FILETIME value) noexcept {
    ULARGE_INTEGER integer{};
    integer.LowPart = value.dwLowDateTime;
    integer.HighPart = value.dwHighDateTime;
    return integer.QuadPart;
}

[[nodiscard]] std::optional<std::uint64_t> process_creation_time(HANDLE process) noexcept {
    FILETIME created{};
    FILETIME exited{};
    FILETIME kernel{};
    FILETIME user{};
    if (!GetProcessTimes(process, &created, &exited, &kernel, &user)) {
        return std::nullopt;
    }
    return as_integer(created);
}

[[nodiscard]] std::optional<USHORT> process_machine(HANDLE process) noexcept {
    using IsWow64Process2Function = BOOL(WINAPI*)(HANDLE, USHORT*, USHORT*);
    const auto kernel = GetModuleHandleW(L"kernel32.dll");
    if (kernel == nullptr) {
        return std::nullopt;
    }
    const auto function = reinterpret_cast<IsWow64Process2Function>(
        GetProcAddress(kernel, "IsWow64Process2"));
    if (function == nullptr) {
        BOOL wow64 = FALSE;
        if (!IsWow64Process(process, &wow64)) {
            return std::nullopt;
        }
        SYSTEM_INFO system{};
        GetNativeSystemInfo(&system);
        if (wow64) {
            return static_cast<USHORT>(IMAGE_FILE_MACHINE_I386);
        }
        if (system.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_AMD64) {
            return static_cast<USHORT>(IMAGE_FILE_MACHINE_AMD64);
        }
        if (system.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_INTEL) {
            return static_cast<USHORT>(IMAGE_FILE_MACHINE_I386);
        }
        return std::nullopt;
    }

    USHORT process_value = IMAGE_FILE_MACHINE_UNKNOWN;
    USHORT native_value = IMAGE_FILE_MACHINE_UNKNOWN;
    if (!function(process, &process_value, &native_value)) {
        return std::nullopt;
    }
    return process_value == IMAGE_FILE_MACHINE_UNKNOWN ? native_value : process_value;
}

}  // namespace

Result inject_fixed_adjacent_module(const BrokerRequest& request) noexcept {
    const UniqueHandle process{reinterpret_cast<HANDLE>(request.process_handle)};
    if (!process) {
        return make_result(request, ResultStatus::rejected, "process-handle-invalid",
                           kFixedModuleNameUtf8, ERROR_INVALID_HANDLE);
    }
    const DWORD actual_pid = GetProcessId(process.get());
    if (actual_pid == 0U) {
        return make_result(request, ResultStatus::rejected, "process-handle-invalid",
                           kFixedModuleNameUtf8, GetLastError());
    }
    if (actual_pid != request.pid) {
        return make_result(request, ResultStatus::rejected, "process-handle-pid-mismatch",
                           kFixedModuleNameUtf8);
    }

    DWORD session_id = 0U;
    if (!ProcessIdToSessionId(actual_pid, &session_id)) {
        return make_result(request, ResultStatus::failed, "session-unavailable",
                           kFixedModuleNameUtf8, GetLastError());
    }
    if (session_id != request.expected_session_id) {
        return make_result(request, ResultStatus::rejected, "session-mismatch",
                           kFixedModuleNameUtf8);
    }
    if (session_id == 0U) {
        return make_result(request, ResultStatus::skipped, "session-zero",
                           kFixedModuleNameUtf8);
    }

    PROCESS_PROTECTION_LEVEL_INFORMATION protection{};
    const bool protection_query_succeeded =
        GetProcessInformation(process.get(), ProcessProtectionLevelInfo, &protection,
                              sizeof(protection)) != FALSE;
    const DWORD protection_error = protection_query_succeeded ? 0U : GetLastError();
    if (!protection_state_allows_injection(
            protection_query_succeeded,
            protection.ProtectionLevel == PROTECTION_LEVEL_NONE)) {
        return make_result(request, ResultStatus::skipped,
                           protection_query_succeeded ? "protected-process"
                                                      : "protection-state-unavailable",
                           kFixedModuleNameUtf8, protection_error);
    }

    BOOL critical = FALSE;
    if (!IsProcessCritical(process.get(), &critical)) {
        return make_result(request, ResultStatus::skipped, "critical-state-unavailable",
                           kFixedModuleNameUtf8, GetLastError());
    }
    if (critical != FALSE) {
        return make_result(request, ResultStatus::skipped, "critical-process",
                           kFixedModuleNameUtf8);
    }

    const auto creation_time = process_creation_time(process.get());
    if (!creation_time) {
        return make_result(request, ResultStatus::failed, "identity-unavailable",
                           kFixedModuleNameUtf8, GetLastError());
    }
    if (*creation_time != request.expected_creation_time) {
        return make_result(request, ResultStatus::rejected, "creation-time-mismatch",
                           kFixedModuleNameUtf8);
    }

    const auto machine = process_machine(process.get());
    if (!machine) {
        return make_result(request, ResultStatus::failed, "architecture-unavailable",
                           kFixedModuleNameUtf8, GetLastError());
    }
    if (*machine != kExpectedMachine) {
        return make_result(request, ResultStatus::skipped, "architecture-mismatch",
                           kFixedModuleNameUtf8);
    }

    const auto lifecycle = query_process_lifecycle(process.get());
    if (lifecycle && lifecycle->deleting) {
        return make_result(request, ResultStatus::skipped, "process-exiting",
                           kFixedModuleNameUtf8);
    }
    if (lifecycle && lifecycle->frozen) {
        return make_result(request, ResultStatus::skipped, "process-frozen",
                           kFixedModuleNameUtf8);
    }

    const auto module_path = fixed_module_path();
    std::error_code module_error;
    if (!module_path || !std::filesystem::is_regular_file(*module_path, module_error)) {
        return make_result(request, ResultStatus::failed, "fixed-module-missing",
                           kFixedModuleNameUtf8,
                           static_cast<std::uint32_t>(module_error.value()));
    }
    switch (fixed_module_state(process.get(), *module_path)) {
        case FixedModuleState::Absent:
            break;
        case FixedModuleState::ExpectedModuleLoaded:
            return make_result(request, ResultStatus::skipped, "module-already-loaded",
                               kFixedModuleNameUtf8);
        case FixedModuleState::SameBasenameDifferentPath:
            return make_result(request, ResultStatus::skipped,
                               "conflicting-mactype-module-loaded",
                               kFixedModuleNameUtf8);
        case FixedModuleState::InventoryUnavailable:
            return make_result(request, ResultStatus::failed,
                               "module-inventory-unavailable", kFixedModuleNameUtf8,
                               GetLastError());
    }
    const auto verified_creation_time = process_creation_time(process.get());
    if (!verified_creation_time || *verified_creation_time != request.expected_creation_time) {
        return make_result(request, ResultStatus::rejected, "creation-time-mismatch",
                           kFixedModuleNameUtf8);
    }
    return inject_module(process.get(), request, *module_path);
}

}  // namespace mactype::injector
