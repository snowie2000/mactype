#pragma once

#include <cstdint>
#include <iosfwd>
#include <string>
#include <vector>

namespace mtpc {

constexpr std::uint32_t kMagic = 0x4350544D;
constexpr std::uint16_t kVersion = 1;
constexpr std::uint32_t kMaxJsonLength = 64U * 1024U;
constexpr std::uint32_t kMaxBinaryLength = 8U * 1024U * 1024U;

enum class MessageKind : std::uint16_t {
  hello = 1,
  ping = 2,
  render_preview = 3,
  shutdown = 4,
  load_profile = 5,
  show_native_preview = 6,
  hide_native_preview = 7,
  hello_ack = 101,
  pong = 102,
  preview_rendered = 103,
  ack = 104,
  native_preview_state = 105,
  error = 199,
};

struct Frame {
  MessageKind kind{};
  std::uint64_t request_id{};
  std::string json;
  std::vector<std::uint8_t> binary;
};

bool read_frame(std::istream& input, Frame& frame, std::string& error);
bool write_frame(std::ostream& output, const Frame& frame);
std::vector<std::uint8_t> placeholder_png();

}  // namespace mtpc
