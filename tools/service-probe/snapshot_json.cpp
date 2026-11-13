#include "snapshot_json.h"

#include <sstream>

namespace mactype::service_probe::internal {
namespace {

std::string OptionalJsonString(const std::wstring& value) {
  return value.empty() ? "null" : "\"" + EscapeJson(value) + "\"";
}

const ModuleObservation* SelectPrimaryModule(const ObservedModules& observed,
                                              const std::string& architecture) {
  const std::wstring preferred_name =
      architecture == "x64" ? L"MacType64.dll" : L"MacType.dll";
  for (const auto& [path, module] : observed) {
    static_cast<void>(path);
    if (_wcsicmp(module.name.c_str(), preferred_name.c_str()) == 0) {
      return &module;
    }
  }
  return observed.empty() ? nullptr : &observed.begin()->second;
}

}  // namespace

std::string BuildSnapshotJson(const ProbeOptions& options,
                              const ObservedModules& observed,
                              const std::string& render_fingerprint,
                              const std::string& observed_at) {
  const ProcessObservation process = CaptureProcessObservation();
  const ModuleObservation* primary =
      SelectPrimaryModule(observed, process.architecture);

  std::ostringstream json;
  json << "{\n"
       << "  \"schemaVersion\": 1,\n"
       << "  \"probeKind\": \"" << EscapeJson(options.probe_kind) << "\",\n"
       << "  \"role\": \"" << EscapeJson(options.role) << "\",\n"
       << "  \"treeLevel\": " << options.tree_level << ",\n"
       << "  \"pid\": " << process.pid << ",\n"
       << "  \"parentPid\": " << process.parent_pid << ",\n"
       << "  \"sessionId\": " << process.session_id << ",\n"
       << "  \"architecture\": \"" << process.architecture << "\",\n"
       << "  \"processMachine\": \"" << process.process_machine << "\",\n"
       << "  \"nativeMachine\": \"" << process.native_machine << "\",\n"
       << "  \"integrity\": \"" << EscapeJson(process.integrity) << "\",\n"
       << "  \"startedAt\": \"" << process.started_at << "\",\n"
       << "  \"observedAt\": \"" << observed_at << "\",\n"
       << "  \"waitMilliseconds\": " << options.wait_milliseconds << ",\n"
       << "  \"mactypeModuleLoaded\": "
       << (!observed.empty() ? "true" : "false") << ",\n"
       << "  \"mactypeModulePath\": "
       << (primary == nullptr ? "null" : OptionalJsonString(primary->path))
       << ",\n"
       << "  \"mactypeVersion\": "
       << (primary == nullptr ? "null" : OptionalJsonString(primary->version))
       << ",\n"
       << "  \"versionSource\": "
       << (primary == nullptr ? "null"
                              : OptionalJsonString(primary->version_source))
       << ",\n"
       << "  \"loadObservedAt\": "
       << (primary == nullptr ? "null"
                              : "\"" + primary->first_observed_at + "\"")
       << ",\n"
       << "  \"renderFingerprint\": \"" << render_fingerprint << "\",\n"
       << "  \"render\": {\n"
       << "    \"width\": 320,\n"
       << "    \"height\": 96,\n"
       << "    \"pixelFormat\": \"BGRA8-top-down\",\n"
       << "    \"font\": \"Segoe UI\",\n"
       << "    \"text\": \"MacType probe 0123456789 Aa 中 あ\"\n"
       << "  },\n"
       << "  \"modules\": [";

  bool first = true;
  for (const auto& [path, module] : observed) {
    static_cast<void>(path);
    if (!first) {
      json << ',';
    }
    first = false;
    json << "\n    {\"name\": \"" << EscapeJson(module.name)
         << "\", \"path\": \"" << EscapeJson(module.path)
         << "\", \"version\": " << OptionalJsonString(module.version)
         << ", \"versionSource\": "
         << OptionalJsonString(module.version_source)
         << ", \"firstObservedAt\": \"" << module.first_observed_at << "\"}";
  }
  if (!observed.empty()) {
    json << '\n';
  }
  json << "  ]\n}\n";
  return json.str();
}

}  // namespace mactype::service_probe::internal
