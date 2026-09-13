#include "protocol.h"

#include <array>
#include <istream>
#include <ostream>
#include <type_traits>

namespace mtpc {
namespace {

template <typename T>
bool read_little_endian(std::istream& input, T& value) {
  static_assert(std::is_unsigned_v<T>);
  std::array<unsigned char, sizeof(T)> bytes{};
  if (!input.read(reinterpret_cast<char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()))) {
    return false;
  }
  value = 0;
  for (std::size_t index = 0; index < bytes.size(); ++index) {
    value |= static_cast<T>(bytes[index]) << (index * 8U);
  }
  return true;
}

template <typename T>
void write_little_endian(std::ostream& output, T value) {
  static_assert(std::is_unsigned_v<T>);
  for (std::size_t index = 0; index < sizeof(T); ++index) {
    output.put(static_cast<char>((value >> (index * 8U)) & 0xFFU));
  }
}

}  // namespace

bool read_frame(std::istream& input, Frame& frame, std::string& error) {
  std::uint32_t magic{};
  std::uint16_t version{};
  std::uint16_t kind{};
  std::uint32_t json_length{};
  std::uint32_t binary_length{};
  if (!read_little_endian(input, magic)) return false;
  if (!read_little_endian(input, version) || !read_little_endian(input, kind) ||
      !read_little_endian(input, frame.request_id) || !read_little_endian(input, json_length) ||
      !read_little_endian(input, binary_length)) {
    error = "truncated frame header";
    return false;
  }
  if (magic != kMagic || version != kVersion) {
    error = "unsupported frame magic or version";
    return false;
  }
  if (json_length > kMaxJsonLength || binary_length > kMaxBinaryLength) {
    error = "frame length exceeds protocol limit";
    return false;
  }
  frame.kind = static_cast<MessageKind>(kind);
  frame.json.resize(json_length);
  frame.binary.resize(binary_length);
  if ((json_length != 0U && !input.read(frame.json.data(), json_length)) ||
      (binary_length != 0U && !input.read(reinterpret_cast<char*>(frame.binary.data()), binary_length))) {
    error = "truncated frame payload";
    return false;
  }
  return true;
}

bool write_frame(std::ostream& output, const Frame& frame) {
  if (frame.json.size() > kMaxJsonLength || frame.binary.size() > kMaxBinaryLength) return false;
  write_little_endian(output, kMagic);
  write_little_endian(output, kVersion);
  write_little_endian(output, static_cast<std::uint16_t>(frame.kind));
  write_little_endian(output, frame.request_id);
  write_little_endian(output, static_cast<std::uint32_t>(frame.json.size()));
  write_little_endian(output, static_cast<std::uint32_t>(frame.binary.size()));
  output.write(frame.json.data(), static_cast<std::streamsize>(frame.json.size()));
  output.write(reinterpret_cast<const char*>(frame.binary.data()), static_cast<std::streamsize>(frame.binary.size()));
  output.flush();
  return output.good();
}

std::vector<std::uint8_t> placeholder_png() {
  return {
      0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
      0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
      0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
      0x0D, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F,
      0x00, 0x05, 0xFE, 0x02, 0xFE, 0xA7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00, 0x00,
      0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82};
}

}  // namespace mtpc
