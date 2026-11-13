#pragma once

#include <string>

namespace mactype {

std::wstring full_path(const std::wstring& path);
bool regular_file(const std::wstring& path);
bool x86_image(const std::wstring& path);

}  // namespace mactype
