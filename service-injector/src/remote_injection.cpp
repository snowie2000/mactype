#include "remote_injection.h"

#include "fixed_module.h"
#include "module_inventory.h"
#include "process_lifecycle.h"
#include "remote_injection_verdict.h"
#include "unique_handle.h"

#include <cstddef>
#include <string>

namespace mactype::injector {
namespace {

#ifndef MACTYPE_REMOTE_TIMEOUT_MS
constexpr DWORD kRemoteTimeoutMs = 10'000U;
#else
constexpr DWORD kRemoteTimeoutMs = MACTYPE_REMOTE_TIMEOUT_MS;
#endif
constexpr DWORD kCleanupGraceMs = 5'000U;

class RemoteAllocation final {
public:
    RemoteAllocation(HANDLE process, void* address) noexcept
        : process_{process}, address_{address} {}
    ~RemoteAllocation() {
        if (process_ != nullptr && address_ != nullptr) {
            VirtualFreeEx(process_, address_, 0U, MEM_RELEASE);
        }
    }
    RemoteAllocation(const RemoteAllocation&) = delete;
    RemoteAllocation& operator=(const RemoteAllocation&) = delete;
    [[nodiscard]] void* get() const noexcept { return address_; }
    [[nodiscard]] bool release() noexcept {
        if (process_ == nullptr || address_ == nullptr) {
            return true;
        }
        if (!VirtualFreeEx(process_, address_, 0U, MEM_RELEASE)) {
            return false;
        }
        process_ = nullptr;
        address_ = nullptr;
        return true;
    }
    void abandon() noexcept {
        process_ = nullptr;
        address_ = nullptr;
    }

private:
    HANDLE process_{};
    void* address_{};
};

[[nodiscard]] Result make_verdict_result(HANDLE process,
                                         const BrokerRequest& request,
                                         const RemoteInjectionEvidence& evidence,
                                         const DWORD windows_error) noexcept {
    const auto verdict = adjudicate_remote_injection(evidence);
    if (verdict.code == "module-load-failed") {
        const auto lifecycle = query_process_lifecycle(process);
        if (lifecycle && lifecycle->deleting) {
            return make_result(request, ResultStatus::skipped, "process-exiting",
                               kFixedModuleNameUtf8, windows_error, true);
        }
    }
    return make_result(request, verdict.status, verdict.code, kFixedModuleNameUtf8,
                       windows_error, verdict.cleanup_complete);
}

[[nodiscard]] ModuleInventoryEvidence inventory_evidence(
    const FixedModuleState state, DWORD& error) noexcept {
    switch (state) {
        case FixedModuleState::Absent:
            return ModuleInventoryEvidence::not_loaded;
        case FixedModuleState::ExpectedModuleLoaded:
            return ModuleInventoryEvidence::loaded;
        case FixedModuleState::SameBasenameDifferentPath:
            return ModuleInventoryEvidence::unavailable;
        case FixedModuleState::InventoryUnavailable:
            error = GetLastError();
            return ModuleInventoryEvidence::unavailable;
    }
    error = ERROR_INVALID_DATA;
    return ModuleInventoryEvidence::unavailable;
}

}  // namespace

Result inject_module(HANDLE process, const BrokerRequest& request,
                     const std::filesystem::path& module_path) noexcept {
    const std::wstring module = module_path.native();
    const std::size_t byte_count = (module.size() + 1U) * sizeof(wchar_t);
    void* address = VirtualAllocEx(process, nullptr, byte_count, MEM_COMMIT | MEM_RESERVE,
                                   PAGE_READWRITE);
    if (address == nullptr) {
        return make_result(request, ResultStatus::failed, "remote-allocation-failed",
                           kFixedModuleNameUtf8, GetLastError());
    }
    RemoteAllocation allocation{process, address};
    SIZE_T written = 0U;
    if (!WriteProcessMemory(process, allocation.get(), module.c_str(), byte_count, &written) ||
        written != byte_count) {
        const DWORD error = GetLastError();
        const bool cleanup_complete = allocation.release();
        return make_result(request, ResultStatus::failed, "remote-write-failed",
                           kFixedModuleNameUtf8, error, cleanup_complete);
    }

    const auto load_library = remote_load_library(process);
    if (!load_library) {
        const DWORD error = GetLastError();
        const bool cleanup_complete = allocation.release();
        return make_result(request, ResultStatus::failed, "loader-address-unavailable",
                           kFixedModuleNameUtf8, error, cleanup_complete);
    }
    const UniqueHandle thread{CreateRemoteThread(process, nullptr, 0U, *load_library,
                                                  allocation.get(), 0U, nullptr)};
    if (!thread) {
        const DWORD error = GetLastError();
        const bool cleanup_complete = allocation.release();
        return make_result(request, ResultStatus::failed, "remote-thread-failed",
                           kFixedModuleNameUtf8, error, cleanup_complete);
    }

    RemoteCompletion completion = RemoteCompletion::completed_on_time;
    DWORD wait = WaitForSingleObject(thread.get(), kRemoteTimeoutMs);
    if (wait == WAIT_TIMEOUT) {
        wait = WaitForSingleObject(thread.get(), kCleanupGraceMs);
        if (wait != WAIT_OBJECT_0) {
            const DWORD error = wait == WAIT_FAILED ? GetLastError() : 0U;
            allocation.abandon();
            return make_verdict_result(
                process, request,
                {RemoteCompletion::grace_exhausted, ThreadResultEvidence::unavailable,
                 ModuleInventoryEvidence::unavailable, false},
                error);
        }
        completion = RemoteCompletion::completed_after_deadline;
    } else if (wait != WAIT_OBJECT_0) {
        const DWORD error = GetLastError();
        allocation.abandon();
        return make_verdict_result(
            process, request,
            {RemoteCompletion::wait_failed, ThreadResultEvidence::unavailable,
             ModuleInventoryEvidence::unavailable, false},
            error);
    }

    DWORD load_result = 0U;
    const bool result_available = GetExitCodeThread(thread.get(), &load_result) != FALSE;
    DWORD evidence_error = result_available ? 0U : GetLastError();
    const bool memory_released = allocation.release();
    if (!memory_released && evidence_error == 0U) {
        evidence_error = GetLastError();
    }
    const auto inventory_state = fixed_module_state(process, module_path);
    const auto inventory = inventory_evidence(inventory_state, evidence_error);
    const auto thread_result = !result_available
                                   ? ThreadResultEvidence::unavailable
                                   : load_result == 0U ? ThreadResultEvidence::not_loaded
                                                       : ThreadResultEvidence::loaded;
    if (inventory_state == FixedModuleState::SameBasenameDifferentPath &&
        memory_released) {
        return make_result(request, ResultStatus::skipped,
                           "conflicting-mactype-module-loaded", kFixedModuleNameUtf8,
                           evidence_error, true);
    }
    return make_verdict_result(
        process, request,
        {completion, thread_result, inventory, memory_released},
        evidence_error);
}

}  // namespace mactype::injector
