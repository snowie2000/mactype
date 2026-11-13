#pragma once

#include <filesystem>
#include <optional>
#include <string_view>

namespace mactype::injector {

#ifdef MACTYPE_FIXED_MODULE_BASENAME
inline constexpr std::wstring_view kFixedModuleName = MACTYPE_FIXED_MODULE_BASENAME;
inline constexpr std::string_view kFixedModuleNameUtf8 = MACTYPE_FIXED_MODULE_UTF8;
#elif defined(_WIN64)
inline constexpr std::wstring_view kFixedModuleName = L"MacType64.dll";
inline constexpr std::string_view kFixedModuleNameUtf8 = "MacType64.dll";
#else
inline constexpr std::wstring_view kFixedModuleName = L"MacType.dll";
inline constexpr std::string_view kFixedModuleNameUtf8 = "MacType.dll";
#endif

[[nodiscard]] std::optional<std::filesystem::path> fixed_module_path();

}  // namespace mactype::injector
