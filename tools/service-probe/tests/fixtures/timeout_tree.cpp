#include <windows.h>

#include <filesystem>
#include <fstream>
#include <string>
#include <string_view>
#include <vector>

namespace {

std::wstring QuoteArgument(const std::wstring& value) {
  std::wstring quoted = L"\"";
  quoted += value;
  quoted += L"\"";
  return quoted;
}

std::filesystem::path ReadOutputPath(const int argc, wchar_t** argv) {
  for (int index = 1; index + 1 < argc; ++index) {
    if (std::wstring_view(argv[index]) == L"--out") {
      return argv[index + 1];
    }
  }
  return {};
}

bool IsDescendant(const int argc, wchar_t** argv) {
  for (int index = 1; index < argc; ++index) {
    if (std::wstring_view(argv[index]) == L"--fixture-descendant") {
      return true;
    }
  }
  return false;
}

void WritePid(const std::filesystem::path& output, const bool descendant) {
  std::filesystem::path path = output;
  path += descendant ? L".fixture-descendant.pid" : L".fixture-child.pid";
  std::ofstream stream(path, std::ios::trunc);
  stream << GetCurrentProcessId();
}

void SpawnDescendant(const std::filesystem::path& output) {
  std::vector<wchar_t> executable(32768);
  const DWORD size = GetModuleFileNameW(
      nullptr, executable.data(), static_cast<DWORD>(executable.size()));
  if (size == 0 || size >= executable.size()) {
    ExitProcess(10);
  }
  const std::wstring path(executable.data(), size);
  std::wstring command = QuoteArgument(path) + L" --fixture-descendant --out " +
                         QuoteArgument(output.wstring());
  std::vector<wchar_t> mutable_command(command.begin(), command.end());
  mutable_command.push_back(L'\0');
  STARTUPINFOW startup{};
  startup.cb = sizeof(startup);
  PROCESS_INFORMATION process{};
  if (CreateProcessW(path.c_str(), mutable_command.data(), nullptr, nullptr,
                     FALSE, CREATE_UNICODE_ENVIRONMENT, nullptr, nullptr,
                     &startup, &process) == FALSE) {
    ExitProcess(11);
  }
  CloseHandle(process.hThread);
  CloseHandle(process.hProcess);
}

}  // namespace

int wmain(const int argc, wchar_t** argv) {
  const std::filesystem::path output = ReadOutputPath(argc, argv);
  if (output.empty()) {
    return 64;
  }
  const bool descendant = IsDescendant(argc, argv);
  WritePid(output, descendant);
  if (!descendant) {
    SpawnDescendant(output);
  }
  Sleep(INFINITE);
  return 0;
}
