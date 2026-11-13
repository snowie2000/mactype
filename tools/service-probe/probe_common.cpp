#include "probe_common.h"

#include "process_observation.h"
#include "render_probe.h"
#include "snapshot_json.h"

#include <chrono>
#include <map>
#include <string_view>
#include <thread>

namespace mactype::service_probe {
namespace {

bool ParseUnsigned(const wchar_t* text, DWORD& value) {
  if (text == nullptr || *text == L'\0') {
    return false;
  }
  wchar_t* end = nullptr;
  const unsigned long parsed = wcstoul(text, &end, 10);
  if (end == text || *end != L'\0' || parsed > MAXDWORD) {
    return false;
  }
  value = static_cast<DWORD>(parsed);
  return true;
}

void PumpPendingWindowMessages() {
  MSG message{};
  while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE) != FALSE) {
    TranslateMessage(&message);
    DispatchMessageW(&message);
  }
}

}  // namespace

bool ParseCommonArguments(const int argc, wchar_t** argv,
                          CommonArguments& result, std::wstring& error) {
  for (int index = 1; index < argc; ++index) {
    const std::wstring_view argument(argv[index]);
    if (argument == L"--help" || argument == L"-h") {
      result.show_help = true;
      continue;
    }
    if (argument == L"--out" && index + 1 < argc) {
      result.options.output_path = argv[++index];
      continue;
    }
    if (argument == L"--wait-ms" && index + 1 < argc) {
      if (!ParseUnsigned(argv[++index], result.options.wait_milliseconds)) {
        error = L"--wait-ms must be an unsigned integer";
        return false;
      }
      continue;
    }
    if (argument == L"--role" && index + 1 < argc) {
      result.options.role = argv[++index];
      continue;
    }
    if (argument == L"--tree-level" && index + 1 < argc) {
      DWORD level = 0;
      if (!ParseUnsigned(argv[++index], level)) {
        error = L"--tree-level must be an unsigned integer";
        return false;
      }
      result.options.tree_level = level;
      continue;
    }
    error = L"Unknown or incomplete argument: " + std::wstring(argument);
    return false;
  }
  if (!result.show_help && result.options.output_path.empty()) {
    error = L"--out <path> is required";
    return false;
  }
  return true;
}

int ObserveAndWrite(const ProbeOptions& options,
                    const bool pump_window_messages, std::wstring& error) {
  internal::ObservedModules observed;
  const ULONGLONG start = GetTickCount64();
  for (;;) {
    const std::string timestamp = UtcNow();
    for (internal::ModuleObservation& module :
         internal::FindMacTypeModules()) {
      if (!observed.contains(module.path)) {
        module.first_observed_at = timestamp;
        observed.emplace(module.path, std::move(module));
      }
    }
    if (pump_window_messages) {
      PumpPendingWindowMessages();
    }
    if (GetTickCount64() - start >= options.wait_milliseconds) {
      break;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(25));
  }

  const std::string fingerprint = internal::RenderFingerprint(error);
  if (fingerprint.empty()) {
    return 2;
  }
  const std::string json =
      internal::BuildSnapshotJson(options, observed, fingerprint, UtcNow());
  if (!WriteUtf8Json(options.output_path, json, error)) {
    return 3;
  }
  return 0;
}

}  // namespace mactype::service_probe
