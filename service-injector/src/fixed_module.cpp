#include "fixed_module.h"

#include <windows.h>

#include <string>
#include <vector>

namespace mactype::injector {

std::optional<std::filesystem::path> fixed_module_path() {
    std::vector<wchar_t> executable_path(32'768U);
    const DWORD length = GetModuleFileNameW(nullptr, executable_path.data(),
                                            static_cast<DWORD>(executable_path.size()));
    if (length == 0U || length >= executable_path.size()) {
        return std::nullopt;
    }
    try {
        return std::filesystem::path{std::wstring_view{executable_path.data(), length}}
                   .parent_path() /
               kFixedModuleName;
    } catch (...) {
        return std::nullopt;
    }
}

}  // namespace mactype::injector
