#pragma once

#include <windows.h>

#include <string>

namespace mactype::service_probe::internal {

std::wstring Win32ErrorMessage(DWORD code);
std::string FileTimeToIso8601(const FILETIME& file_time);

}  // namespace mactype::service_probe::internal
