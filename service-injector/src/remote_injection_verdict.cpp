#include "remote_injection_verdict.h"

namespace mactype::injector {

RemoteInjectionVerdict adjudicate_remote_injection(
    const RemoteInjectionEvidence& evidence) noexcept {
    if (evidence.completion == RemoteCompletion::grace_exhausted) {
        return {ResultStatus::timed_out, "remote-load-cleanup-unknown", false};
    }
    if (evidence.completion == RemoteCompletion::wait_failed) {
        return {ResultStatus::failed, "remote-wait-cleanup-unknown", false};
    }
    if (!evidence.remote_memory_released) {
        return {ResultStatus::failed, "remote-memory-cleanup-unknown", false};
    }
    if (evidence.thread_result == ThreadResultEvidence::unavailable ||
        evidence.module_inventory == ModuleInventoryEvidence::unavailable) {
        return {ResultStatus::failed, "post-injection-state-cleanup-unknown", false};
    }
    if (evidence.thread_result == ThreadResultEvidence::loaded &&
        evidence.module_inventory == ModuleInventoryEvidence::loaded) {
        return {
            ResultStatus::injected,
            evidence.completion == RemoteCompletion::completed_after_deadline
                ? "module-loaded-late"
                : "module-loaded",
            true,
        };
    }
    if (evidence.thread_result == ThreadResultEvidence::not_loaded &&
        evidence.module_inventory == ModuleInventoryEvidence::not_loaded) {
        return {ResultStatus::failed, "module-load-failed", true};
    }
    return {ResultStatus::failed, "post-injection-state-cleanup-unknown", false};
}

}  // namespace mactype::injector
