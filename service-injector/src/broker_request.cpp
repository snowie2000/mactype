#include "broker_request.h"

#include <cerrno>
#include <cstdlib>
#include <limits>

namespace mactype::injector {
namespace {

template <typename Integer>
[[nodiscard]] std::optional<Integer> parse_decimal(std::wstring_view text) noexcept {
    if (text.empty() || text.front() == L'-' || text.size() > 20U) {
        return std::nullopt;
    }
    for (const wchar_t character : text) {
        if (character < L'0' || character > L'9') {
            return std::nullopt;
        }
    }

    std::wstring value{text};
    wchar_t* end = nullptr;
    errno = 0;
    const auto parsed = std::wcstoull(value.c_str(), &end, 10);
    if (errno == ERANGE || end != value.c_str() + value.size() ||
        parsed > static_cast<unsigned long long>(std::numeric_limits<Integer>::max())) {
        return std::nullopt;
    }
    return static_cast<Integer>(parsed);
}

[[nodiscard]] bool is_generation_id(std::wstring_view value) noexcept {
    if (value.size() != 64U) {
        return false;
    }
    for (const wchar_t character : value) {
        const bool digit = character >= L'0' && character <= L'9';
        const bool lower = character >= L'a' && character <= L'f';
        if (!digit && !lower) {
            return false;
        }
    }
    return true;
}

}  // namespace

std::optional<BrokerRequest> parse_broker_request(
    std::span<const std::wstring_view> arguments) noexcept {
    if (arguments.size() != 11U || arguments[1] != L"--process-handle" ||
        arguments[3] != L"--pid" || arguments[5] != L"--creation-time" ||
        arguments[7] != L"--session-id" || arguments[9] != L"--generation-id" ||
        !is_generation_id(arguments[10])) {
        return std::nullopt;
    }

    const auto process_handle = parse_decimal<std::uintptr_t>(arguments[2]);
    const auto pid = parse_decimal<std::uint32_t>(arguments[4]);
    const auto creation_time = parse_decimal<std::uint64_t>(arguments[6]);
    const auto session_id = parse_decimal<std::uint32_t>(arguments[8]);
    if (!process_handle || *process_handle == 0U || !pid || *pid == 0U || !creation_time ||
        *creation_time == 0U || !session_id) {
        return std::nullopt;
    }

    std::string generation_id;
    generation_id.reserve(arguments[10].size());
    for (const wchar_t character : arguments[10]) {
        generation_id.push_back(static_cast<char>(character));
    }
    return BrokerRequest{*process_handle, *pid, *creation_time, *session_id,
                         std::move(generation_id)};
}

}  // namespace mactype::injector
