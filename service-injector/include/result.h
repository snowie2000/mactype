#pragma once

#include "broker_request.h"

#include <cstdint>
#include <string>
#include <string_view>

namespace mactype::injector {

enum class ResultStatus {
    injected,
    skipped,
    rejected,
    failed,
    timed_out,
};

struct Result final {
    ResultStatus status{ResultStatus::failed};
    std::string_view code{"internal-error"};
    std::uint32_t pid{};
    std::uint32_t session_id{};
    std::string generation_id;
    std::string_view module;
    std::uint32_t windows_error{};
    bool cleanup_complete{true};
};

[[nodiscard]] Result make_result(
    const BrokerRequest& request,
    ResultStatus status,
    std::string_view code,
    std::string_view module,
    std::uint32_t windows_error = 0U,
    bool cleanup_complete = true);

[[nodiscard]] std::string to_json(const Result& result);
[[nodiscard]] int exit_code(ResultStatus status) noexcept;

}  // namespace mactype::injector
