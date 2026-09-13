#include "broker_request.h"
#include "module_inventory.h"
#include "process_lifecycle.h"
#include "remote_injection_verdict.h"
#include "result.h"
#include "safety_policy.h"

#include <array>
#include <chrono>
#include <filesystem>
#include <iostream>
#include <optional>
#include <string_view>
#include <type_traits>
#include <vector>

namespace {

bool valid_fixed_broker_request_is_accepted() {
    constexpr std::array arguments{
        std::wstring_view{L"mactype-injector.exe"},
        std::wstring_view{L"--process-handle"},
        std::wstring_view{L"4096"},
        std::wstring_view{L"--pid"},
        std::wstring_view{L"1234"},
        std::wstring_view{L"--creation-time"},
        std::wstring_view{L"133967890123456789"},
        std::wstring_view{L"--session-id"},
        std::wstring_view{L"2"},
        std::wstring_view{L"--generation-id"},
        std::wstring_view{L"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
    };

    const auto parsed = mactype::injector::parse_broker_request(arguments);
    return parsed.has_value() && parsed->process_handle == 4096U && parsed->pid == 1234U &&
           parsed->expected_creation_time == 133967890123456789ULL &&
           parsed->expected_session_id == 2U;
}

bool arbitrary_runtime_selectors_are_rejected() {
    constexpr std::array base_arguments{
        std::wstring_view{L"mactype-injector.exe"},
        std::wstring_view{L"--process-handle"},
        std::wstring_view{L"4096"},
        std::wstring_view{L"--pid"},
        std::wstring_view{L"1234"},
        std::wstring_view{L"--creation-time"},
        std::wstring_view{L"133967890123456789"},
        std::wstring_view{L"--session-id"},
        std::wstring_view{L"2"},
        std::wstring_view{L"--generation-id"},
        std::wstring_view{L"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},
    };
    constexpr std::array selectors{
        std::array{std::wstring_view{L"--dll"},
                   std::wstring_view{L"C:\\untrusted\\evil.dll"}},
        std::array{std::wstring_view{L"--executable"},
                   std::wstring_view{L"C:\\untrusted\\evil.exe"}},
        std::array{std::wstring_view{L"--service-name"},
                   std::wstring_view{L"UntrustedService"}},
    };
    for (const auto& selector : selectors) {
        std::vector<std::wstring_view> arguments{base_arguments.begin(), base_arguments.end()};
        arguments.insert(arguments.end(), selector.begin(), selector.end());
        if (mactype::injector::parse_broker_request(arguments).has_value()) {
            return false;
        }
    }
    return true;
}

bool malformed_generation_is_rejected() {
    constexpr std::array arguments{
        std::wstring_view{L"mactype-injector.exe"},
        std::wstring_view{L"--process-handle"},
        std::wstring_view{L"4096"},
        std::wstring_view{L"--pid"},
        std::wstring_view{L"1234"},
        std::wstring_view{L"--creation-time"},
        std::wstring_view{L"133967890123456789"},
        std::wstring_view{L"--session-id"},
        std::wstring_view{L"2"},
        std::wstring_view{L"--generation-id"},
        std::wstring_view{L"not-a-digest"},
    };
    if (mactype::injector::parse_broker_request(arguments).has_value()) {
        return false;
    }
    constexpr std::array uppercase{
        std::wstring_view{L"mactype-injector.exe"},
        std::wstring_view{L"--process-handle"},
        std::wstring_view{L"4096"},
        std::wstring_view{L"--pid"},
        std::wstring_view{L"1234"},
        std::wstring_view{L"--creation-time"},
        std::wstring_view{L"133967890123456789"},
        std::wstring_view{L"--session-id"},
        std::wstring_view{L"2"},
        std::wstring_view{L"--generation-id"},
        std::wstring_view{L"0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef"},
    };
    return !mactype::injector::parse_broker_request(uppercase).has_value();
}

bool malformed_or_missing_process_handle_is_rejected() {
    constexpr std::wstring_view digest =
        L"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    constexpr std::array invalid_handles{
        std::wstring_view{L"0"},
        std::wstring_view{L"-1"},
        std::wstring_view{L"not-a-handle"},
        std::wstring_view{L"18446744073709551616"},
    };
    for (const auto handle : invalid_handles) {
        const std::array arguments{
            std::wstring_view{L"mactype-injector.exe"},
            std::wstring_view{L"--process-handle"},
            handle,
            std::wstring_view{L"--pid"},
            std::wstring_view{L"1234"},
            std::wstring_view{L"--creation-time"},
            std::wstring_view{L"133967890123456789"},
            std::wstring_view{L"--session-id"},
            std::wstring_view{L"2"},
            std::wstring_view{L"--generation-id"},
            digest,
        };
        if (mactype::injector::parse_broker_request(arguments).has_value()) {
            return false;
        }
    }
    constexpr std::array missing{
        std::wstring_view{L"mactype-injector.exe"},
        std::wstring_view{L"--pid"},
        std::wstring_view{L"1234"},
        std::wstring_view{L"--creation-time"},
        std::wstring_view{L"133967890123456789"},
        std::wstring_view{L"--session-id"},
        std::wstring_view{L"2"},
        std::wstring_view{L"--generation-id"},
        digest,
    };
    return !mactype::injector::parse_broker_request(missing).has_value();
}

bool structured_result_is_bounded() {
    const mactype::injector::BrokerRequest request{
        4096U,
        1234U,
        133967890123456789ULL,
        2U,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    };
    const auto result = mactype::injector::make_result(
        request, mactype::injector::ResultStatus::injected, "module-loaded", "MacType64.dll");
    const auto json = mactype::injector::to_json(result);
    return json.size() <= 1024U && json.find("\"schemaVersion\":1") != std::string::npos &&
           json.find("\"cleanupComplete\":true") != std::string::npos;
}

bool unknown_process_protection_is_rejected() {
    using mactype::injector::protection_state_allows_injection;
    return protection_state_allows_injection(true, true) &&
           !protection_state_allows_injection(true, false) &&
           !protection_state_allows_injection(false, true) &&
           !protection_state_allows_injection(false, false);
}

bool process_lifecycle_flags_are_decoded_independently() {
    struct Case final {
        std::uint32_t flags;
        bool frozen;
        bool deleting;
    };
    constexpr std::array cases{
        Case{0x00U, false, false},
        Case{0x04U, false, true},
        Case{0x10U, true, false},
        Case{0x14U, true, true},
        Case{0x58U, true, false},
        Case{0x48U, false, false},
    };
    for (const auto& item : cases) {
        const auto lifecycle =
            mactype::injector::decode_process_lifecycle_flags(item.flags);
        if (lifecycle.frozen != item.frozen || lifecycle.deleting != item.deleting) {
            return false;
        }
    }
    return true;
}

bool current_process_lifecycle_is_queryable_and_active() {
    const auto lifecycle =
        mactype::injector::query_process_lifecycle(GetCurrentProcess());
    return lifecycle.has_value() && !lifecycle->frozen && !lifecycle->deleting;
}

bool completed_remote_execution_is_cross_checked_against_module_inventory() {
    using mactype::injector::ModuleInventoryEvidence;
    using mactype::injector::RemoteCompletion;
    using mactype::injector::RemoteInjectionEvidence;
    using mactype::injector::ThreadResultEvidence;
    using mactype::injector::adjudicate_remote_injection;

    const auto late_success = adjudicate_remote_injection(RemoteInjectionEvidence{
        RemoteCompletion::completed_after_deadline,
        ThreadResultEvidence::loaded,
        ModuleInventoryEvidence::loaded,
        true,
    });
    const auto definitive_failure = adjudicate_remote_injection(RemoteInjectionEvidence{
        RemoteCompletion::completed_on_time,
        ThreadResultEvidence::not_loaded,
        ModuleInventoryEvidence::not_loaded,
        true,
    });
    return late_success.status == mactype::injector::ResultStatus::injected &&
           late_success.code == "module-loaded-late" && late_success.cleanup_complete &&
           definitive_failure.status == mactype::injector::ResultStatus::failed &&
           definitive_failure.code == "module-load-failed" &&
           definitive_failure.cleanup_complete;
}

bool unverifiable_post_injection_state_fails_closed() {
    using mactype::injector::ModuleInventoryEvidence;
    using mactype::injector::RemoteCompletion;
    using mactype::injector::RemoteInjectionEvidence;
    using mactype::injector::ThreadResultEvidence;
    using mactype::injector::adjudicate_remote_injection;

    constexpr std::array evidence{
        RemoteInjectionEvidence{
            RemoteCompletion::completed_on_time,
            ThreadResultEvidence::unavailable,
            ModuleInventoryEvidence::loaded,
            true,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::completed_on_time,
            ThreadResultEvidence::loaded,
            ModuleInventoryEvidence::unavailable,
            true,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::grace_exhausted,
            ThreadResultEvidence::unavailable,
            ModuleInventoryEvidence::unavailable,
            false,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::wait_failed,
            ThreadResultEvidence::unavailable,
            ModuleInventoryEvidence::unavailable,
            false,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::completed_on_time,
            ThreadResultEvidence::loaded,
            ModuleInventoryEvidence::loaded,
            false,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::completed_on_time,
            ThreadResultEvidence::loaded,
            ModuleInventoryEvidence::not_loaded,
            true,
        },
        RemoteInjectionEvidence{
            RemoteCompletion::completed_on_time,
            ThreadResultEvidence::not_loaded,
            ModuleInventoryEvidence::loaded,
            true,
        },
    };
    for (const auto& item : evidence) {
        const auto verdict = adjudicate_remote_injection(item);
        if (verdict.status == mactype::injector::ResultStatus::injected ||
            !verdict.code.ends_with("-cleanup-unknown") || verdict.cleanup_complete) {
            return false;
        }
    }
    return true;
}

bool fixed_module_identity_is_an_exact_normalized_full_path() {
    using InventoryFunction = mactype::injector::FixedModuleState (*)(
        HANDLE, const std::filesystem::path&) noexcept;
    static_assert(std::is_same_v<
                  decltype(&mactype::injector::fixed_module_state),
                  InventoryFunction>);
    using mactype::injector::module_paths_equal;
    return module_paths_equal(LR"(C:\Program Files\MacType\MacType64.dll)",
                              LR"(c:\PROGRAM FILES\MacType\.\MacType64.dll)") &&
           module_paths_equal(LR"(\\?\C:\Program Files\MacType\MacType64.dll)",
                              LR"(C:\Program Files\MacType\MacType64.dll)") &&
           !module_paths_equal(LR"(C:\Other\MacType64.dll)",
                               LR"(C:\Program Files\MacType\MacType64.dll)") &&
           !module_paths_equal(LR"(C:\Other\Program Files\MacType\MacType64.dll)",
                               LR"(C:\Program Files\MacType\MacType64.dll)");
}

bool transient_inventory_failures_are_retried_only_within_the_bound() {
    using mactype::injector::inventory_failure_is_transient;
    using mactype::injector::inventory_retry_action;
    using mactype::injector::kInventoryRetryBudget;
    using mactype::injector::InventoryRetry;
    using std::chrono::milliseconds;
    constexpr milliseconds just_inside = kInventoryRetryBudget - milliseconds{1};
    return inventory_failure_is_transient(ERROR_PARTIAL_COPY) &&
           !inventory_failure_is_transient(ERROR_ACCESS_DENIED) &&
           !inventory_failure_is_transient(ERROR_SUCCESS) &&
           inventory_retry_action(ERROR_PARTIAL_COPY, milliseconds{0}, false) ==
               InventoryRetry::Retry &&
           inventory_retry_action(ERROR_PARTIAL_COPY, just_inside, false) ==
               InventoryRetry::Retry &&
           inventory_retry_action(ERROR_PARTIAL_COPY, kInventoryRetryBudget, false) ==
               InventoryRetry::GiveUp &&
           inventory_retry_action(ERROR_PARTIAL_COPY, milliseconds{0}, true) ==
               InventoryRetry::GiveUp &&
           inventory_retry_action(ERROR_ACCESS_DENIED, milliseconds{0}, false) ==
               InventoryRetry::GiveUp &&
           inventory_retry_action(ERROR_SUCCESS, milliseconds{0}, false) ==
               InventoryRetry::GiveUp;
}

}  // namespace

int wmain() {
    if (!valid_fixed_broker_request_is_accepted()) {
        std::cerr << "valid fixed broker request was rejected\n";
        return 1;
    }
    if (!arbitrary_runtime_selectors_are_rejected()) {
        std::cerr << "arbitrary DLL path was accepted\n";
        return 2;
    }
    if (!malformed_generation_is_rejected()) {
        std::cerr << "malformed generation ID was accepted\n";
        return 3;
    }
    if (!structured_result_is_bounded()) {
        std::cerr << "structured result violated its public bound\n";
        return 4;
    }
    if (!unknown_process_protection_is_rejected()) {
        std::cerr << "unknown process protection was allowed to inject\n";
        return 5;
    }
    if (!malformed_or_missing_process_handle_is_rejected()) {
        std::cerr << "malformed or missing inherited process handle was accepted\n";
        return 6;
    }
    if (!process_lifecycle_flags_are_decoded_independently()) {
        std::cerr << "process lifecycle flags were decoded incorrectly\n";
        return 14;
    }
    if (!current_process_lifecycle_is_queryable_and_active()) {
        std::cerr << "current process lifecycle was unavailable or inactive\n";
        return 15;
    }
    if (!completed_remote_execution_is_cross_checked_against_module_inventory()) {
        std::cerr << "completed remote execution was not cross-checked\n";
        return 7;
    }
    if (!unverifiable_post_injection_state_fails_closed()) {
        std::cerr << "unverifiable post-injection state did not fail closed\n";
        return 8;
    }
    if (!fixed_module_identity_is_an_exact_normalized_full_path()) {
        std::cerr << "fixed module identity accepted a basename or suffix match\n";
        return 9;
    }
    if (!transient_inventory_failures_are_retried_only_within_the_bound()) {
        std::cerr << "transient module inventory failures were not retried within the bound\n";
        return 10;
    }
    return 0;
}
