#pragma once

#include "result.h"

#include <string_view>

namespace mactype::injector {

enum class RemoteCompletion {
    completed_on_time,
    completed_after_deadline,
    grace_exhausted,
    wait_failed,
};

enum class ThreadResultEvidence {
    loaded,
    not_loaded,
    unavailable,
};

enum class ModuleInventoryEvidence {
    loaded,
    not_loaded,
    unavailable,
};

struct RemoteInjectionEvidence final {
    RemoteCompletion completion{RemoteCompletion::wait_failed};
    ThreadResultEvidence thread_result{ThreadResultEvidence::unavailable};
    ModuleInventoryEvidence module_inventory{ModuleInventoryEvidence::unavailable};
    bool remote_memory_released{};
};

struct RemoteInjectionVerdict final {
    ResultStatus status{ResultStatus::failed};
    std::string_view code{"post-injection-state-cleanup-unknown"};
    bool cleanup_complete{};
};

[[nodiscard]] RemoteInjectionVerdict adjudicate_remote_injection(
    const RemoteInjectionEvidence& evidence) noexcept;

}  // namespace mactype::injector
