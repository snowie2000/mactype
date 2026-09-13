#pragma once

#include <cstdint>
#include <optional>
#include <span>
#include <string>
#include <string_view>

namespace mactype::injector {

struct BrokerRequest final {
    std::uintptr_t process_handle{};
    std::uint32_t pid{};
    std::uint64_t expected_creation_time{};
    std::uint32_t expected_session_id{};
    std::string generation_id;
};

[[nodiscard]] std::optional<BrokerRequest> parse_broker_request(
    std::span<const std::wstring_view> arguments) noexcept;

}  // namespace mactype::injector
