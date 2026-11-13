#pragma once

#include "json.hpp"

#include <cstddef>
#include <stdexcept>
#include <string>

namespace renderer {

// Allows a 32K-character path even when JSON escapes every character. The
// legacy window carries commands, never profile contents or arbitrary blobs.
constexpr std::size_t kMaxControlMessageBytes = 256 * 1024;

enum class ControlMessageKind { invalid, ping, loadProfile };

struct ControlMessage final
{
	ControlMessageKind kind = ControlMessageKind::invalid;
	std::string path;
};

inline ControlMessage ParseControlMessage(const void* data, std::size_t size) noexcept
{
	try
	{
		if (!data || size == 0 || size > kMaxControlMessageBytes)
			return {};
		const char* bytes = static_cast<const char*>(data);
		std::string payload(bytes, size);
		// COPYDATA callers may count their terminating NUL. Embedded NULs must
		// not make the JSON parser silently ignore a second payload.
		if (!payload.empty() && payload.back() == '\0')
			payload.pop_back();
		if (payload.find('\0') != std::string::npos)
			return {};
		const auto object = nlohmann::json::parse(payload,
			[](int depth, nlohmann::json::parse_event_t, nlohmann::json&) {
				if (depth > 8)
					throw std::invalid_argument("control message nesting limit");
				return true;
			});
		if (!object.is_object())
			return {};
		const auto command = object.find("command");
		if (command == object.end() || !command->is_string())
			return {};
		const std::string name = command->get<std::string>();
		if (name == "ping")
			return {ControlMessageKind::ping, {}};
		if (name != "loadprofile")
			return {};
		const auto file = object.find("file");
		if (file == object.end() || !file->is_string())
			return {};
		std::string path = file->get<std::string>();
		if (path.empty() || path.find('\0') != std::string::npos)
			return {};
		return {ControlMessageKind::loadProfile, std::move(path)};
	}
	catch (...)
	{
		return {};
	}
}

} // namespace renderer
