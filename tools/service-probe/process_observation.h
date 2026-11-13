#pragma once

#include <windows.h>

#include <map>
#include <string>
#include <vector>

namespace mactype::service_probe::internal {

struct ModuleObservation {
  std::wstring name;
  std::wstring path;
  std::wstring version;
  std::wstring version_source;
  std::string first_observed_at;
};

using ObservedModules = std::map<std::wstring, ModuleObservation>;

struct ProcessObservation {
  DWORD pid = 0;
  DWORD parent_pid = 0;
  DWORD session_id = 0;
  std::string architecture;
  std::string process_machine;
  std::string native_machine;
  std::wstring integrity;
  std::string started_at;
};

std::vector<ModuleObservation> FindMacTypeModules();
ProcessObservation CaptureProcessObservation();

}  // namespace mactype::service_probe::internal
