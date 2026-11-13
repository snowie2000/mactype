#include "child_process.h"

#include <memory>

namespace mactype::service_probe::internal {
namespace {

constexpr DWORD kCleanupWaitMilliseconds = 5000;
// The launcher is outside this job; only its child and grandchild can coexist.
constexpr DWORD kMaximumJobProcesses = 2;

struct HandleCloser {
  void operator()(HANDLE handle) const noexcept {
    if (handle != nullptr && handle != INVALID_HANDLE_VALUE) {
      CloseHandle(handle);
    }
  }
};

using UniqueHandle = std::unique_ptr<void, HandleCloser>;

ChildProcessResult LaunchFailure(const DWORD code, const bool launched = false) {
  ChildProcessResult result;
  result.exit_code = code;
  result.launched = launched;
  return result;
}

bool ConfigureJob(const HANDLE job) {
  JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
  limits.BasicLimitInformation.LimitFlags =
      JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
  limits.BasicLimitInformation.ActiveProcessLimit = kMaximumJobProcesses;
  return SetInformationJobObject(job, JobObjectExtendedLimitInformation, &limits,
                                 sizeof(limits)) != FALSE;
}

void WaitForJobCleanup(const HANDLE job) {
  const ULONGLONG deadline = GetTickCount64() + kCleanupWaitMilliseconds;
  for (;;) {
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION accounting{};
    if (QueryInformationJobObject(job, JobObjectBasicAccountingInformation,
                                  &accounting, sizeof(accounting),
                                  nullptr) == FALSE ||
        accounting.ActiveProcesses == 0 || GetTickCount64() >= deadline) {
      return;
    }
    Sleep(10);
  }
}

void TerminateAssignedTree(const HANDLE job, const HANDLE root,
                           const DWORD exit_code) {
  TerminateJobObject(job, exit_code);
  WaitForSingleObject(root, kCleanupWaitMilliseconds);
  WaitForJobCleanup(job);
}

// CreateProcessW receives a single mutable command line. This quoting is the
// inverse of CommandLineToArgvW and must preserve trailing backslashes.
std::wstring QuoteArgument(const std::wstring& argument) {
  if (argument.find_first_of(L" \t\"") == std::wstring::npos) {
    return argument;
  }
  std::wstring quoted = L"\"";
  std::size_t backslashes = 0;
  for (const wchar_t character : argument) {
    if (character == L'\\') {
      ++backslashes;
      continue;
    }
    if (character == L'\"') {
      quoted.append(backslashes * 2U + 1U, L'\\');
      quoted.push_back(L'\"');
      backslashes = 0;
      continue;
    }
    quoted.append(backslashes, L'\\');
    backslashes = 0;
    quoted.push_back(character);
  }
  quoted.append(backslashes * 2U, L'\\');
  quoted.push_back(L'\"');
  return quoted;
}

}  // namespace

ChildProcessResult LaunchAndWait(
    const std::filesystem::path& executable,
    const std::vector<std::wstring>& arguments,
    const DWORD timeout_milliseconds) {
  std::wstring command = QuoteArgument(executable.wstring());
  for (const std::wstring& argument : arguments) {
    command.push_back(L' ');
    command += QuoteArgument(argument);
  }
  std::vector<wchar_t> mutable_command(command.begin(), command.end());
  mutable_command.push_back(L'\0');

  const UniqueHandle job(CreateJobObjectW(nullptr, nullptr));
  if (job == nullptr) {
    return LaunchFailure(GetLastError());
  }
  if (!ConfigureJob(job.get())) {
    return LaunchFailure(GetLastError());
  }

  STARTUPINFOW startup{};
  startup.cb = sizeof(startup);
  PROCESS_INFORMATION process{};
  if (CreateProcessW(executable.c_str(), mutable_command.data(), nullptr,
                     nullptr, FALSE,
                     CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED, nullptr,
                     nullptr, &startup, &process) == FALSE) {
    return LaunchFailure(GetLastError());
  }
  const UniqueHandle process_handle(process.hProcess);
  const UniqueHandle thread_handle(process.hThread);
  if (AssignProcessToJobObject(job.get(), process_handle.get()) == FALSE) {
    const DWORD code = GetLastError();
    TerminateProcess(process_handle.get(), code);
    WaitForSingleObject(process_handle.get(), kCleanupWaitMilliseconds);
    return LaunchFailure(code, true);
  }
  if (ResumeThread(thread_handle.get()) == static_cast<DWORD>(-1)) {
    const DWORD code = GetLastError();
    TerminateAssignedTree(job.get(), process_handle.get(), code);
    return LaunchFailure(code, true);
  }

  const DWORD wait =
      WaitForSingleObject(process_handle.get(), timeout_milliseconds);
  if (wait == WAIT_TIMEOUT) {
    TerminateAssignedTree(job.get(), process_handle.get(), ERROR_TIMEOUT);
    return LaunchFailure(ERROR_TIMEOUT, true);
  }
  if (wait != WAIT_OBJECT_0) {
    const DWORD code = GetLastError();
    TerminateAssignedTree(job.get(), process_handle.get(), code);
    return LaunchFailure(code, true);
  }

  ChildProcessResult result;
  result.launched = true;
  if (GetExitCodeProcess(process_handle.get(), &result.exit_code) == FALSE) {
    result.exit_code = GetLastError();
  }
  TerminateAssignedTree(job.get(), process_handle.get(),
                        ERROR_PROCESS_ABORTED);
  return result;
}

}  // namespace mactype::service_probe::internal
