#include "probe_common.h"
#include "child_process.h"

#include <windows.h>

#include <filesystem>
#include <iostream>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

namespace {

struct TreeArguments {
  std::filesystem::path manifest_path;
  std::filesystem::path node_path;
  std::filesystem::path child_executable;
  std::filesystem::path grandchild_executable;
  DWORD wait_milliseconds = 3000;
  DWORD level = 0;
  bool show_help = false;
};

bool ParseUnsigned(const wchar_t* text, DWORD& result) {
  if (text == nullptr || *text == L'\0') {
    return false;
  }
  wchar_t* end = nullptr;
  const unsigned long value = wcstoul(text, &end, 10);
  if (end == text || *end != L'\0' || value > MAXDWORD) {
    return false;
  }
  result = static_cast<DWORD>(value);
  return true;
}

bool ParseArguments(const int argc, wchar_t** argv, TreeArguments& result,
                    std::wstring& error) {
  for (int index = 1; index < argc; ++index) {
    const std::wstring_view argument(argv[index]);
    if (argument == L"--help" || argument == L"-h") {
      result.show_help = true;
    } else if (argument == L"--out" && index + 1 < argc) {
      result.manifest_path = argv[++index];
    } else if (argument == L"--node-out" && index + 1 < argc) {
      result.node_path = argv[++index];
    } else if (argument == L"--child-exe" && index + 1 < argc) {
      result.child_executable = argv[++index];
    } else if (argument == L"--grandchild-exe" && index + 1 < argc) {
      result.grandchild_executable = argv[++index];
    } else if (argument == L"--wait-ms" && index + 1 < argc) {
      if (!ParseUnsigned(argv[++index], result.wait_milliseconds)) {
        error = L"--wait-ms must be an unsigned integer";
        return false;
      }
    } else if (argument == L"--level" && index + 1 < argc) {
      if (!ParseUnsigned(argv[++index], result.level) || result.level > 2) {
        error = L"--level must be 0, 1, or 2";
        return false;
      }
    } else {
      error = L"Unknown or incomplete argument: " + std::wstring(argument);
      return false;
    }
  }
  if (!result.show_help && result.manifest_path.empty()) {
    error = L"--out <manifest.json> is required";
    return false;
  }
  return true;
}

std::filesystem::path NodePath(const std::filesystem::path& manifest,
                               const std::wstring_view role) {
  const std::filesystem::path directory = manifest.parent_path();
  return directory /
         (manifest.stem().wstring() + L"." + std::wstring(role) + L".json");
}

std::wstring RoleForLevel(const DWORD level) {
  switch (level) {
    case 0:
      return L"parent";
    case 1:
      return L"child";
    default:
      return L"grandchild";
  }
}

bool FilePresent(const std::filesystem::path& path) {
  std::error_code error;
  return std::filesystem::is_regular_file(path, error) && !error;
}

std::string BuildManifest(
    const TreeArguments& arguments,
    const mactype::service_probe::internal::ChildProcessResult& child,
    const int root_probe_exit) {
  const auto root = NodePath(arguments.manifest_path, L"parent");
  const auto child_path = NodePath(arguments.manifest_path, L"child");
  const auto grandchild = NodePath(arguments.manifest_path, L"grandchild");
  std::ostringstream json;
  json << "{\n"
       << "  \"schemaVersion\": 1,\n"
       << "  \"probeKind\": \"spawn-tree\",\n"
       << "  \"architecture\": \""
       << mactype::service_probe::CurrentArchitecture() << "\",\n"
       << "  \"createdAt\": \"" << mactype::service_probe::UtcNow() << "\",\n"
       << "  \"waitMilliseconds\": " << arguments.wait_milliseconds << ",\n"
       << "  \"rootProbeExitCode\": " << root_probe_exit << ",\n"
       << "  \"childLaunched\": " << (child.launched ? "true" : "false") << ",\n"
       << "  \"childExitCode\": " << child.exit_code << ",\n"
       << "  \"nodes\": [\n"
       << "    {\"level\": 0, \"role\": \"parent\", \"artifact\": \""
       << mactype::service_probe::EscapeJson(root.wstring())
       << "\", \"present\": " << (FilePresent(root) ? "true" : "false") << "},\n"
       << "    {\"level\": 1, \"role\": \"child\", \"artifact\": \""
       << mactype::service_probe::EscapeJson(child_path.wstring())
       << "\", \"present\": " << (FilePresent(child_path) ? "true" : "false")
       << "},\n"
       << "    {\"level\": 2, \"role\": \"grandchild\", \"artifact\": \""
       << mactype::service_probe::EscapeJson(grandchild.wstring())
       << "\", \"present\": " << (FilePresent(grandchild) ? "true" : "false")
       << "}\n"
       << "  ]\n"
       << "}\n";
  return json.str();
}

void PrintUsage() {
  std::wcerr
      << L"Usage: probe-spawn-tree{32|64}.exe --out <manifest.json> "
         L"[--wait-ms <milliseconds>] [--child-exe <path>] "
         L"[--grandchild-exe <path>]\n";
}

}  // namespace

int wmain(const int argc, wchar_t** argv) {
  TreeArguments arguments;
  std::wstring error;
  if (!ParseArguments(argc, argv, arguments, error)) {
    std::wcerr << error << L'\n';
    PrintUsage();
    return 64;
  }
  if (arguments.show_help) {
    PrintUsage();
    return 0;
  }

  if (arguments.node_path.empty()) {
    arguments.node_path = NodePath(arguments.manifest_path, RoleForLevel(arguments.level));
  }
  const std::filesystem::path self =
      mactype::service_probe::CurrentExecutablePath();
  mactype::service_probe::ProbeOptions probe_options;
  probe_options.output_path = arguments.node_path;
  probe_options.probe_kind = L"spawn-tree-node";
  probe_options.role = RoleForLevel(arguments.level);
  probe_options.tree_level = arguments.level;
  probe_options.wait_milliseconds = arguments.wait_milliseconds;
  const int probe_exit = mactype::service_probe::ObserveAndWrite(
      probe_options, false, error);
  if (probe_exit != 0) {
    std::wcerr << error << L'\n';
  }

  mactype::service_probe::internal::ChildProcessResult child;
  if (probe_exit == 0 && arguments.level < 2) {
    std::filesystem::path executable = self;
    if (arguments.level == 0 && !arguments.child_executable.empty()) {
      executable = arguments.child_executable;
    } else if (arguments.level == 1 && !arguments.grandchild_executable.empty()) {
      executable = arguments.grandchild_executable;
    }
    const DWORD next_level = arguments.level + 1;
    std::vector<std::wstring> child_arguments{
        L"--out", arguments.manifest_path.wstring(),
        L"--node-out", NodePath(arguments.manifest_path, RoleForLevel(next_level)).wstring(),
        L"--level", std::to_wstring(next_level),
        L"--wait-ms", std::to_wstring(arguments.wait_milliseconds)};
    if (!arguments.grandchild_executable.empty()) {
      child_arguments.emplace_back(L"--grandchild-exe");
      child_arguments.emplace_back(arguments.grandchild_executable.wstring());
    }
    const ULONGLONG timeout64 =
        static_cast<ULONGLONG>(arguments.wait_milliseconds) * 4ULL + 15000ULL;
    const DWORD timeout =
        timeout64 > MAXDWORD ? MAXDWORD : static_cast<DWORD>(timeout64);
    child = mactype::service_probe::internal::LaunchAndWait(
        executable, child_arguments, timeout);
  }

  if (arguments.level == 0) {
    const std::string manifest = BuildManifest(arguments, child, probe_exit);
    if (!mactype::service_probe::WriteUtf8Json(arguments.manifest_path, manifest,
                                               error)) {
      std::wcerr << error << L'\n';
      return 3;
    }
    const bool nodes_present = FilePresent(NodePath(arguments.manifest_path, L"parent")) &&
                               FilePresent(NodePath(arguments.manifest_path, L"child")) &&
                               FilePresent(NodePath(arguments.manifest_path, L"grandchild"));
    if (!child.launched || child.exit_code != 0 || !nodes_present) {
      return 4;
    }
  } else if (arguments.level == 1 && (!child.launched || child.exit_code != 0)) {
    return 4;
  }
  return probe_exit;
}
