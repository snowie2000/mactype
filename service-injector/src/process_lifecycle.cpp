#include "process_lifecycle.h"

#include <winternl.h>

namespace mactype::injector {
namespace {

struct ExtendedBasicInformation final {
    SIZE_T size;
    PROCESS_BASIC_INFORMATION basic;
    ULONG flags;
};

}  // namespace

ProcessLifecycle decode_process_lifecycle_flags(const std::uint32_t flags) noexcept {
    return ProcessLifecycle{
        (flags & 0x10U) != 0U,
        (flags & 0x04U) != 0U,
    };
}

std::optional<ProcessLifecycle> query_process_lifecycle(HANDLE process) noexcept {
    using NtQueryInformationProcessFunction = NTSTATUS(NTAPI*)(
        HANDLE, PROCESSINFOCLASS, PVOID, ULONG, PULONG);

    const HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    if (ntdll == nullptr) {
        return std::nullopt;
    }
    const auto query = reinterpret_cast<NtQueryInformationProcessFunction>(
        GetProcAddress(ntdll, "NtQueryInformationProcess"));
    if (query == nullptr) {
        return std::nullopt;
    }

    ExtendedBasicInformation information{};
    information.size = sizeof(information);
    ULONG returned_length = 0U;
    const NTSTATUS status = query(process, static_cast<PROCESSINFOCLASS>(0),
                                  &information, sizeof(information), &returned_length);
    if (!NT_SUCCESS(status) || returned_length < sizeof(information)) {
        return std::nullopt;
    }
    return decode_process_lifecycle_flags(information.flags);
}

}  // namespace mactype::injector
