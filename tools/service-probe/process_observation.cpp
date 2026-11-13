#include "process_observation.h"

#include "probe_common.h"
#include "win32_support.h"

#include <shlwapi.h>
#include <tlhelp32.h>

#include <algorithm>
#include <cstddef>
#include <memory>
#include <sstream>
#include <string_view>
#include <utility>

namespace mactype::service_probe::internal {
namespace {

struct HandleCloser {
  void operator()(HANDLE handle) const noexcept {
    if (handle != nullptr && handle != INVALID_HANDLE_VALUE) {
      CloseHandle(handle);
    }
  }
};

using UniqueHandle = std::unique_ptr<void, HandleCloser>;

std::string ProcessStartedAt() {
  FILETIME created{};
  FILETIME exited{};
  FILETIME kernel{};
  FILETIME user{};
  if (GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user) ==
      FALSE) {
    return {};
  }
  return FileTimeToIso8601(created);
}

DWORD ParentProcessId() {
  const UniqueHandle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
  if (snapshot.get() == INVALID_HANDLE_VALUE) {
    return 0;
  }
  PROCESSENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (Process32FirstW(snapshot.get(), &entry) == FALSE) {
    return 0;
  }
  const DWORD current = GetCurrentProcessId();
  do {
    if (entry.th32ProcessID == current) {
      return entry.th32ParentProcessID;
    }
  } while (Process32NextW(snapshot.get(), &entry) != FALSE);
  return 0;
}

std::wstring IntegrityLevel() {
  HANDLE raw_token = nullptr;
  if (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw_token) == FALSE) {
    return L"unknown";
  }
  const UniqueHandle token(raw_token);
  DWORD required = 0;
  GetTokenInformation(token.get(), TokenIntegrityLevel, nullptr, 0, &required);
  if (required == 0) {
    return L"unknown";
  }
  std::vector<std::byte> buffer(required);
  if (GetTokenInformation(token.get(), TokenIntegrityLevel, buffer.data(), required,
                          &required) == FALSE) {
    return L"unknown";
  }
  const auto* label =
      reinterpret_cast<const TOKEN_MANDATORY_LABEL*>(buffer.data());
  const DWORD count = *GetSidSubAuthorityCount(label->Label.Sid);
  if (count == 0) {
    return L"unknown";
  }
  const DWORD rid = *GetSidSubAuthority(label->Label.Sid, count - 1);
  if (rid < SECURITY_MANDATORY_LOW_RID) {
    return L"untrusted";
  }
  if (rid < SECURITY_MANDATORY_MEDIUM_RID) {
    return L"low";
  }
  if (rid < SECURITY_MANDATORY_HIGH_RID) {
    return L"medium";
  }
  if (rid < SECURITY_MANDATORY_SYSTEM_RID) {
    return L"high";
  }
  if (rid < SECURITY_MANDATORY_PROTECTED_PROCESS_RID) {
    return L"system";
  }
  return L"protected";
}

std::string MachineName(const USHORT machine) {
  switch (machine) {
    case IMAGE_FILE_MACHINE_I386:
      return "x86";
    case IMAGE_FILE_MACHINE_AMD64:
      return "x64";
    case IMAGE_FILE_MACHINE_ARM64:
      return "arm64";
    case IMAGE_FILE_MACHINE_UNKNOWN:
      return "native";
    default: {
      std::ostringstream value;
      value << "0x" << std::hex << machine;
      return value.str();
    }
  }
}

std::pair<std::string, std::string> ProcessMachines() {
  using IsWow64Process2Function = BOOL(WINAPI*)(HANDLE, USHORT*, USHORT*);
  const HMODULE kernel = GetModuleHandleW(L"kernel32.dll");
  const auto function = kernel == nullptr
                            ? nullptr
                            : reinterpret_cast<IsWow64Process2Function>(
                                  GetProcAddress(kernel, "IsWow64Process2"));
  if (function != nullptr) {
    USHORT process_machine = IMAGE_FILE_MACHINE_UNKNOWN;
    USHORT native_machine = IMAGE_FILE_MACHINE_UNKNOWN;
    if (function(GetCurrentProcess(), &process_machine, &native_machine) != FALSE) {
      return {MachineName(process_machine), MachineName(native_machine)};
    }
  }
  BOOL wow64 = FALSE;
  if (IsWow64Process(GetCurrentProcess(), &wow64) != FALSE && wow64 != FALSE) {
    return {"x86", "x64"};
  }
  return {CurrentArchitecture(), CurrentArchitecture()};
}

bool IsMacTypeModule(const std::wstring_view name) {
  std::wstring lower(name);
  std::transform(lower.begin(), lower.end(), lower.begin(),
                 [](const wchar_t value) {
                   return static_cast<wchar_t>(towlower(value));
                 });
  return lower.starts_with(L"mactype") && lower.ends_with(L".dll");
}

std::pair<std::wstring, std::wstring> ReadModuleVersion(
    const HMODULE module, const std::wstring& path) {
  using DllGetVersionFunction = HRESULT(WINAPI*)(DLLVERSIONINFO*);
  const auto dll_get_version = reinterpret_cast<DllGetVersionFunction>(
      GetProcAddress(module, "DllGetVersion"));
  if (dll_get_version != nullptr) {
    DLLVERSIONINFO version{};
    version.cbSize = sizeof(version);
    if (SUCCEEDED(dll_get_version(&version))) {
      return {std::to_wstring(version.dwMajorVersion) + L"." +
                  std::to_wstring(version.dwMinorVersion) + L"." +
                  std::to_wstring(version.dwBuildNumber),
              L"DllGetVersion"};
    }
  }
  DWORD ignored = 0;
  const DWORD size = GetFileVersionInfoSizeW(path.c_str(), &ignored);
  if (size == 0) {
    return {};
  }
  std::vector<std::byte> bytes(size);
  if (GetFileVersionInfoW(path.c_str(), 0, size, bytes.data()) == FALSE) {
    return {};
  }
  VS_FIXEDFILEINFO* fixed = nullptr;
  UINT fixed_size = 0;
  if (VerQueryValueW(bytes.data(), L"\\", reinterpret_cast<void**>(&fixed),
                     &fixed_size) == FALSE ||
      fixed == nullptr || fixed_size < sizeof(VS_FIXEDFILEINFO)) {
    return {};
  }
  return {std::to_wstring(HIWORD(fixed->dwFileVersionMS)) + L"." +
              std::to_wstring(LOWORD(fixed->dwFileVersionMS)) + L"." +
              std::to_wstring(HIWORD(fixed->dwFileVersionLS)) + L"." +
              std::to_wstring(LOWORD(fixed->dwFileVersionLS)),
          L"file-version"};
}

}  // namespace

std::vector<ModuleObservation> FindMacTypeModules() {
  std::vector<ModuleObservation> modules;
  const auto append_module = [&modules](const HMODULE module,
                                         const std::wstring& name,
                                         const std::wstring& fallback_path) {
    std::vector<wchar_t> path_buffer(32768);
    const DWORD path_count = GetModuleFileNameW(
        module, path_buffer.data(), static_cast<DWORD>(path_buffer.size()));
    const std::wstring path =
        path_count > 0 && path_count < path_buffer.size()
            ? std::wstring(path_buffer.data(), path_count)
            : fallback_path;
    if (path.empty() ||
        std::ranges::any_of(modules, [&path](const ModuleObservation& existing) {
          return _wcsicmp(existing.path.c_str(), path.c_str()) == 0;
        })) {
      return;
    }
    ModuleObservation record;
    record.name = name;
    record.path = path;
    const auto [version, source] = ReadModuleVersion(module, record.path);
    record.version = version;
    record.version_source = source;
    modules.push_back(std::move(record));
  };

  for (const wchar_t* name : {L"MacType.dll", L"MacType64.dll"}) {
    const HMODULE module = GetModuleHandleW(name);
    if (module != nullptr) {
      append_module(module, name, {});
    }
  }

  const UniqueHandle snapshot(CreateToolhelp32Snapshot(
      TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, GetCurrentProcessId()));
  if (snapshot.get() == INVALID_HANDLE_VALUE) {
    return modules;
  }
  MODULEENTRY32W entry{};
  entry.dwSize = sizeof(entry);
  if (Module32FirstW(snapshot.get(), &entry) == FALSE) {
    return modules;
  }
  do {
    if (IsMacTypeModule(entry.szModule)) {
      append_module(reinterpret_cast<HMODULE>(entry.modBaseAddr), entry.szModule,
                    entry.szExePath);
    }
  } while (Module32NextW(snapshot.get(), &entry) != FALSE);
  return modules;
}

ProcessObservation CaptureProcessObservation() {
  ProcessObservation observation;
  observation.pid = GetCurrentProcessId();
  observation.parent_pid = ParentProcessId();
  ProcessIdToSessionId(observation.pid, &observation.session_id);
  observation.architecture = CurrentArchitecture();
  std::tie(observation.process_machine, observation.native_machine) =
      ProcessMachines();
  observation.integrity = IntegrityLevel();
  observation.started_at = ProcessStartedAt();
  return observation;
}

}  // namespace mactype::service_probe::internal
