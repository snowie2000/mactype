#include <windows.h>

#include <cstdint>
#include <iostream>
#include <limits>
#include <string>
#include <string_view>
#include <vector>

namespace {

class UniqueHandle final {
public:
    explicit UniqueHandle(HANDLE handle = nullptr) noexcept : handle_{handle} {}
    ~UniqueHandle() {
        if (handle_ != nullptr && handle_ != INVALID_HANDLE_VALUE) {
            CloseHandle(handle_);
        }
    }
    UniqueHandle(const UniqueHandle&) = delete;
    UniqueHandle& operator=(const UniqueHandle&) = delete;
    [[nodiscard]] HANDLE get() const noexcept { return handle_; }
    [[nodiscard]] explicit operator bool() const noexcept {
        return handle_ != nullptr && handle_ != INVALID_HANDLE_VALUE;
    }
    void close() noexcept {
        if (handle_ != nullptr && handle_ != INVALID_HANDLE_VALUE) {
            CloseHandle(handle_);
            handle_ = nullptr;
        }
    }

private:
    HANDLE handle_{};
};

[[nodiscard]] bool parse_pid(const std::wstring_view text, DWORD& pid) noexcept {
    if (text.empty()) {
        return false;
    }
    std::uint64_t value = 0;
    for (const wchar_t character : text) {
        if (character < L'0' || character > L'9') {
            return false;
        }
        value = value * 10U + static_cast<unsigned>(character - L'0');
        if (value > std::numeric_limits<DWORD>::max()) {
            return false;
        }
    }
    pid = static_cast<DWORD>(value);
    return pid != 0U;
}

[[nodiscard]] std::wstring quote(const std::wstring_view argument) {
    std::wstring result{L'"'};
    std::size_t slashes = 0;
    for (const wchar_t character : argument) {
        if (character == L'\\') {
            ++slashes;
            continue;
        }
        if (character == L'"') {
            result.append(slashes * 2U + 1U, L'\\');
            result.push_back(character);
            slashes = 0;
            continue;
        }
        result.append(slashes, L'\\');
        slashes = 0;
        result.push_back(character);
    }
    result.append(slashes * 2U, L'\\');
    result.push_back(L'"');
    return result;
}

}  // namespace

int wmain(const int count, wchar_t** values) {
    if (count < 4) {
        std::cerr << "usage: inherited-handle-launcher <injector> <handle-pid> <args...>\n";
        return 125;
    }
    DWORD handle_pid = 0;
    if (!parse_pid(values[2], handle_pid)) {
        std::cerr << "invalid handle PID\n";
        return 125;
    }
    constexpr DWORD access = PROCESS_CREATE_THREAD | PROCESS_QUERY_INFORMATION |
                             PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_OPERATION |
                             PROCESS_VM_WRITE | PROCESS_VM_READ | SYNCHRONIZE;
    const UniqueHandle target{OpenProcess(access, TRUE, handle_pid)};
    if (!target) {
        std::cerr << "OpenProcess failed: " << GetLastError() << '\n';
        return 125;
    }

    std::wstring command = quote(values[1]);
    command += L" --process-handle ";
    command += std::to_wstring(reinterpret_cast<std::uintptr_t>(target.get()));
    for (int index = 3; index < count; ++index) {
        command.push_back(L' ');
        command += quote(values[index]);
    }
    command.push_back(L'\0');

    SECURITY_ATTRIBUTES security{};
    security.nLength = sizeof(security);
    security.bInheritHandle = TRUE;
    HANDLE raw_read = nullptr;
    HANDLE raw_write = nullptr;
    if (!CreatePipe(&raw_read, &raw_write, &security, 0U)) {
        return 125;
    }
    const UniqueHandle output_read{raw_read};
    UniqueHandle output_write{raw_write};
    if (!SetHandleInformation(output_read.get(), HANDLE_FLAG_INHERIT, 0U)) {
        return 125;
    }

    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    startup.hStdOutput = output_write.get();
    startup.hStdError = output_write.get();
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(values[1], command.data(), nullptr, nullptr, TRUE, CREATE_NO_WINDOW,
                        nullptr, nullptr, &startup, &process)) {
        std::cerr << "CreateProcess failed: " << GetLastError() << '\n';
        return 125;
    }
    output_write.close();
    const UniqueHandle child_process{process.hProcess};
    const UniqueHandle child_thread{process.hThread};
    if (WaitForSingleObject(child_process.get(), 25'000U) != WAIT_OBJECT_0) {
        TerminateProcess(child_process.get(), ERROR_TIMEOUT);
        WaitForSingleObject(child_process.get(), 5'000U);
        return 125;
    }
    DWORD exit_code = 125U;
    if (!GetExitCodeProcess(child_process.get(), &exit_code)) {
        return 125;
    }
    std::string output;
    char buffer[512]{};
    for (;;) {
        DWORD read = 0U;
        if (!ReadFile(output_read.get(), buffer, sizeof(buffer), &read, nullptr) || read == 0U) {
            break;
        }
        output.append(buffer, buffer + read);
    }
    std::cout << output;
    return static_cast<int>(exit_code);
}
