#include "result.h"

#include <stdexcept>

namespace mactype::injector {
namespace {

std::string_view status_name(const ResultStatus status) noexcept {
    switch (status) {
        case ResultStatus::injected:
            return "injected";
        case ResultStatus::skipped:
            return "skipped";
        case ResultStatus::rejected:
            return "rejected";
        case ResultStatus::failed:
            return "failed";
        case ResultStatus::timed_out:
            return "timeout";
    }
    return "failed";
}

}  // namespace

Result make_result(const BrokerRequest& request, const ResultStatus status,
                   const std::string_view code, const std::string_view module,
                   const std::uint32_t windows_error, const bool cleanup_complete) {
    return Result{status,
                  code,
                  request.pid,
                  request.expected_session_id,
                  request.generation_id,
                  module,
                  windows_error,
                  cleanup_complete};
}

std::string to_json(const Result& result) {
    std::string json;
    json.reserve(384U);
    json += "{\"schemaVersion\":1,\"status\":\"";
    json += status_name(result.status);
    json += "\",\"code\":\"";
    json += result.code;
    json += "\",\"pid\":";
    json += std::to_string(result.pid);
    json += ",\"sessionId\":";
    json += std::to_string(result.session_id);
    json += ",\"generationId\":\"";
    json += result.generation_id;
    json += "\",\"module\":\"";
    json += result.module;
    json += "\",\"windowsError\":";
    json += std::to_string(result.windows_error);
    json += ",\"cleanupComplete\":";
    json += result.cleanup_complete ? "true}" : "false}";
    if (json.size() > 1024U) {
        throw std::length_error{"injector result exceeded its public bound"};
    }
    return json;
}

int exit_code(const ResultStatus status) noexcept {
    switch (status) {
        case ResultStatus::injected:
        case ResultStatus::skipped:
            return 0;
        case ResultStatus::rejected:
            return 2;
        case ResultStatus::failed:
            return 3;
        case ResultStatus::timed_out:
            return 4;
    }
    return 3;
}

}  // namespace mactype::injector
