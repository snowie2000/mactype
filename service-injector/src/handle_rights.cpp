#include "handle_rights.h"

namespace mactype::injector {
namespace {

// The public shape of `ObjectBasicInformation`. The query accepts exactly one
// length for this class: a larger buffer answers STATUS_INFO_LENGTH_MISMATCH
// instead of filling what it can.
struct ObjectBasicInformation final {
    ULONG attributes;
    ACCESS_MASK granted_access;
    ULONG handle_count;
    ULONG pointer_count;
    ULONG reserved[10];
};

using NtQueryObjectFunction = LONG(NTAPI*)(HANDLE, int, PVOID, ULONG, PULONG);

constexpr int kObjectBasicInformation = 0;

}  // namespace

std::optional<ACCESS_MASK> granted_access(HANDLE object) noexcept {
    const auto ntdll = GetModuleHandleW(L"ntdll.dll");
    if (ntdll == nullptr) {
        return std::nullopt;
    }
    const auto query =
        reinterpret_cast<NtQueryObjectFunction>(GetProcAddress(ntdll, "NtQueryObject"));
    if (query == nullptr) {
        return std::nullopt;
    }
    ObjectBasicInformation information{};
    ULONG returned = 0U;
    if (query(object, kObjectBasicInformation, &information,
              static_cast<ULONG>(sizeof(information)), &returned) < 0) {
        return std::nullopt;
    }
    return information.granted_access;
}

bool injection_rights_present(const ACCESS_MASK granted) noexcept {
    constexpr ACCESS_MASK required =
        PROCESS_VM_OPERATION | PROCESS_VM_WRITE | PROCESS_CREATE_THREAD;
    return (granted & required) == required;
}

}  // namespace mactype::injector
