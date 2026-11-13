#pragma once

#include <cstdint>
#include <string>
#include <vector>

namespace mactype {

std::vector<std::uint8_t> encode_png(std::uint32_t width, std::uint32_t height,
                                     std::uint32_t stride, const std::uint8_t* pixels,
                                     std::string& error);

}  // namespace mactype
