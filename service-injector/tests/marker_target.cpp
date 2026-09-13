#include <windows.h>

#include "module_inventory.h"

#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <limits>
#include <optional>
#include <string>
#include <string_view>
#include <thread>

namespace {

struct Options final {
    std::filesystem::path metadata_path;
    std::filesystem::path result_path;
    DWORD wait_ms{};
    std::filesystem::path expected_module;
    std::optional<std::filesystem::path> preload_module;
};

std::optional<DWORD> parse_wait(std::wstring_view text) {
    try {
        std::size_t consumed = 0U;
        const auto parsed = std::stoull(std::wstring{text}, &consumed, 10);
        if (consumed != text.size() || parsed == 0U ||
            parsed > std::numeric_limits<DWORD>::max()) {
            return std::nullopt;
        }
        return static_cast<DWORD>(parsed);
    } catch (...) {
        return std::nullopt;
    }
}

std::optional<Options> parse_options(int count, wchar_t** values) {
    if ((count != 9 && count != 11) ||
        std::wstring_view{values[1]} != L"--metadata" ||
        std::wstring_view{values[3]} != L"--result" ||
        std::wstring_view{values[5]} != L"--wait-ms" ||
        std::wstring_view{values[7]} != L"--expected-module" ||
        (count == 11 && std::wstring_view{values[9]} != L"--preload")) {
        return std::nullopt;
    }
    const auto wait = parse_wait(values[6]);
    const std::filesystem::path expected_module{values[8]};
    if (!wait || !expected_module.is_absolute()) {
        return std::nullopt;
    }
    return Options{
        values[2],
        values[4],
        *wait,
        expected_module,
        count == 11 ? std::optional<std::filesystem::path>{values[10]} : std::nullopt,
    };
}

std::uint64_t file_time_to_integer(const FILETIME value) {
    ULARGE_INTEGER integer{};
    integer.LowPart = value.dwLowDateTime;
    integer.HighPart = value.dwHighDateTime;
    return integer.QuadPart;
}

bool write_text(const std::filesystem::path& path, const std::string& text) {
    std::ofstream output{path, std::ios::binary | std::ios::trunc};
    output << text;
    return output.good();
}

}  // namespace

int wmain(int count, wchar_t** values) {
    const auto options = parse_options(count, values);
    if (!options) {
        return 2;
    }
    HMODULE preloaded = nullptr;
    if (options->preload_module) {
        preloaded = LoadLibraryW(options->preload_module->c_str());
        if (preloaded == nullptr) {
            return 8;
        }
    }

    FILETIME created{};
    FILETIME exited{};
    FILETIME kernel{};
    FILETIME user{};
    if (!GetProcessTimes(GetCurrentProcess(), &created, &exited, &kernel, &user)) {
        return 3;
    }
    DWORD session_id = 0U;
    const DWORD pid = GetCurrentProcessId();
    if (!ProcessIdToSessionId(pid, &session_id)) {
        return 4;
    }

    const std::string metadata =
        "{\"pid\":" + std::to_string(pid) + ",\"creationTime\":" +
        std::to_string(file_time_to_integer(created)) + ",\"sessionId\":" +
        std::to_string(session_id) + "}";
    if (!write_text(options->metadata_path, metadata)) {
        return 5;
    }

    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::milliseconds{options->wait_ms};
    bool loaded = false;
    bool inventory_available = true;
    while (std::chrono::steady_clock::now() < deadline) {
        const auto inventory_result = mactype::injector::fixed_module_state(
            GetCurrentProcess(), options->expected_module);
        if (inventory_result ==
            mactype::injector::FixedModuleState::InventoryUnavailable) {
            inventory_available = false;
            break;
        }
        if (inventory_result ==
            mactype::injector::FixedModuleState::ExpectedModuleLoaded) {
            loaded = true;
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds{10});
    }

    if (loaded) {
        std::this_thread::sleep_for(std::chrono::milliseconds{1'500});
    }

    const std::string result = std::string{"{\"loaded\":"} +
                               (loaded ? "true" : "false") + "}";
    if (!write_text(options->result_path, result)) {
        return 6;
    }
    if (preloaded != nullptr) {
        FreeLibrary(preloaded);
    }
    return loaded ? 0 : (inventory_available ? 7 : 9);
}
