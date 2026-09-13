#include "probe_common.h"

#include "win32_support.h"

#include <fstream>
#include <iomanip>
#include <sstream>
#include <system_error>

namespace mactype::service_probe {

std::string Utf8(const std::wstring_view value) {
  if (value.empty()) {
    return {};
  }
  const int size = WideCharToMultiByte(
      CP_UTF8, WC_ERR_INVALID_CHARS, value.data(),
      static_cast<int>(value.size()), nullptr, 0, nullptr, nullptr);
  if (size <= 0) {
    return {};
  }
  std::string output(static_cast<std::size_t>(size), '\0');
  WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value.data(),
                      static_cast<int>(value.size()), output.data(), size,
                      nullptr, nullptr);
  return output;
}

std::string EscapeJson(const std::wstring_view value) {
  const std::string utf8 = Utf8(value);
  std::ostringstream output;
  for (const unsigned char character : utf8) {
    switch (character) {
      case '\"':
        output << "\\\"";
        break;
      case '\\':
        output << "\\\\";
        break;
      case '\b':
        output << "\\b";
        break;
      case '\f':
        output << "\\f";
        break;
      case '\n':
        output << "\\n";
        break;
      case '\r':
        output << "\\r";
        break;
      case '\t':
        output << "\\t";
        break;
      default:
        if (character < 0x20U) {
          output << "\\u" << std::hex << std::setw(4) << std::setfill('0')
                 << static_cast<unsigned int>(character) << std::dec;
        } else {
          output << static_cast<char>(character);
        }
        break;
    }
  }
  return output.str();
}

bool WriteUtf8Json(const std::filesystem::path& path,
                   const std::string_view json, std::wstring& error) {
  std::error_code filesystem_error;
  if (path.has_parent_path()) {
    std::filesystem::create_directories(path.parent_path(), filesystem_error);
    if (filesystem_error) {
      error = L"Could not create output directory: " +
              std::wstring(path.parent_path()) + L" (" +
              std::to_wstring(filesystem_error.value()) + L")";
      return false;
    }
  }
  std::filesystem::path temporary = path;
  temporary += L".tmp." + std::to_wstring(GetCurrentProcessId());
  {
    std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
    if (!output) {
      error =
          L"Could not open temporary output file: " + std::wstring(temporary);
      return false;
    }
    output.write(json.data(), static_cast<std::streamsize>(json.size()));
    if (!output) {
      error = L"Could not write temporary output file: " +
              std::wstring(temporary);
      return false;
    }
  }
  if (MoveFileExW(temporary.c_str(), path.c_str(),
                  MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) == FALSE) {
    const DWORD code = GetLastError();
    DeleteFileW(temporary.c_str());
    error = L"Could not publish output JSON: " +
            internal::Win32ErrorMessage(code);
    return false;
  }
  return true;
}

}  // namespace mactype::service_probe
