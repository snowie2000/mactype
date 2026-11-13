#include "win32_support.h"

#include "probe_common.h"

#include <iomanip>
#include <sstream>
#include <vector>

namespace mactype::service_probe {
namespace internal {

std::wstring Win32ErrorMessage(const DWORD code) {
  wchar_t* buffer = nullptr;
  const DWORD count = FormatMessageW(
      FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM |
          FORMAT_MESSAGE_IGNORE_INSERTS,
      nullptr, code, 0, reinterpret_cast<wchar_t*>(&buffer), 0, nullptr);
  std::wstring message = count == 0 || buffer == nullptr
                             ? L"Win32 error " + std::to_wstring(code)
                             : std::wstring(buffer, count);
  if (buffer != nullptr) {
    LocalFree(buffer);
  }
  while (!message.empty() &&
         (message.back() == L'\r' || message.back() == L'\n' ||
          message.back() == L' ')) {
    message.pop_back();
  }
  return message;
}

std::string FileTimeToIso8601(const FILETIME& file_time) {
  SYSTEMTIME system_time{};
  if (FileTimeToSystemTime(&file_time, &system_time) == FALSE) {
    return {};
  }
  std::ostringstream output;
  output << std::setfill('0') << std::setw(4) << system_time.wYear << '-'
         << std::setw(2) << system_time.wMonth << '-' << std::setw(2)
         << system_time.wDay << 'T' << std::setw(2) << system_time.wHour << ':'
         << std::setw(2) << system_time.wMinute << ':' << std::setw(2)
         << system_time.wSecond << '.' << std::setw(3)
         << system_time.wMilliseconds << 'Z';
  return output.str();
}

}  // namespace internal

std::wstring CurrentExecutablePath() {
  std::vector<wchar_t> buffer(32768);
  const DWORD count = GetModuleFileNameW(nullptr, buffer.data(),
                                         static_cast<DWORD>(buffer.size()));
  return count == 0 || count >= buffer.size() ? std::wstring{}
                                              : std::wstring(buffer.data(), count);
}

std::string CurrentArchitecture() {
#if defined(_M_X64)
  return "x64";
#elif defined(_M_IX86)
  return "x86";
#elif defined(_M_ARM64)
  return "arm64";
#else
  return "unknown";
#endif
}

std::string UtcNow() {
  FILETIME file_time{};
  GetSystemTimeAsFileTime(&file_time);
  return internal::FileTimeToIso8601(file_time);
}

}  // namespace mactype::service_probe
