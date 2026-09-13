#pragma once

#include <windows.h>

#include <cstdint>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

namespace mactype::service_probe {

struct ProbeOptions {
  std::filesystem::path output_path;
  std::wstring probe_kind = L"console";
  std::wstring role = L"standalone";
  std::uint32_t tree_level = 0;
  DWORD wait_milliseconds = 3000;
};

struct CommonArguments {
  ProbeOptions options;
  bool show_help = false;
};

bool ParseCommonArguments(int argc, wchar_t** argv, CommonArguments& result,
                          std::wstring& error);
int ObserveAndWrite(const ProbeOptions& options, bool pump_window_messages,
                    std::wstring& error);

std::string EscapeJson(std::wstring_view value);
std::string Utf8(std::wstring_view value);
bool WriteUtf8Json(const std::filesystem::path& path, std::string_view json,
                   std::wstring& error);
std::wstring CurrentExecutablePath();
std::string CurrentArchitecture();
std::string UtcNow();

}  // namespace mactype::service_probe

