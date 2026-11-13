#include "installation_check.h"
#include "json_document.h"
#include "png_encoder.h"

#include <Windows.h>
#include <objbase.h>

#include <array>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace {

bool check(bool condition, const char* name) {
  if (!condition) std::cerr << "FAILED: " << name << '\n';
  return condition;
}

bool json_tests() {
  std::string error;
  const auto document = mactype::JsonDocument::parse(
      R"({"name":"line\nquote\"slash\\","number":12.5,"flag":true,"nested":{"name":"inner","value":7},"values":[8,10,12]})",
      error);
  if (!check(document.has_value(), "json valid document")) return false;
  if (!check(document->root_string("name") == "line\nquote\"slash\\", "json escapes")) return false;
  if (!check(document->root_number("number") == 12.5, "json number")) return false;
  if (!check(document->root_bool("flag") == true, "json boolean")) return false;
  if (!check(document->json_number("value") == 7.0, "json nested lookup")) return false;
  const auto nested = document->object("nested");
  if (!check(nested && nested->json_string("name") == "inner", "json object access")) return false;
  const auto values = document->root_number_array("values");
  if (!check(values && *values == std::vector<double>({8.0, 10.0, 12.0}), "json array access")) {
    return false;
  }
  for (const auto& [name, text] : std::array<std::pair<const char*, std::string>, 4>{
           std::pair{"json malformed", R"({"value":truejunk})"},
           std::pair{"json unsupported unicode escape", std::string{"{\"value\":\""} + char(92) + "u0041\"}"},
           std::pair{"json trailing comma", R"({"value":1,})"},
           std::pair{"json non-object root", R"([1,2])"}}) {
    error.clear();
    if (!check(!mactype::JsonDocument::parse(text, error), name)) return false;
  }
  error.clear();
  const std::string oversized(mactype::kJsonDocumentMaxLength + 1U, ' ');
  if (!check(!mactype::JsonDocument::parse(oversized, error), "json oversized document")) {
    return false;
  }
  std::string nested_text;
  for (std::size_t depth = 0; depth < mactype::kJsonDocumentMaxDepth + 1U; ++depth) {
    nested_text += R"({"value":)";
  }
  nested_text += "0";
  nested_text.append(mactype::kJsonDocumentMaxDepth + 1U, '}');
  error.clear();
  return check(!mactype::JsonDocument::parse(nested_text, error), "json nesting limit");
}

bool png_tests() {
  const HRESULT initialized = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
  std::array<std::uint8_t, 24> pixels{
      0, 0, 255, 255, 0, 255, 0, 255, 0xCC, 0xCC, 0xCC, 0xCC,
      255, 0, 0, 255, 255, 255, 255, 255, 0xDD, 0xDD, 0xDD, 0xDD};
  std::string error;
  const auto png = mactype::encode_png(2, 2, 12, pixels.data(), error);
  if (SUCCEEDED(initialized)) CoUninitialize();
  constexpr std::array<std::uint8_t, 8> signature{0x89, 0x50, 0x4E, 0x47,
                                                  0x0D, 0x0A, 0x1A, 0x0A};
  if (!check(png.size() >= 24U && std::equal(signature.begin(), signature.end(), png.begin()),
             "png signature")) {
    return false;
  }
  const auto big_endian = [&](std::size_t offset) {
    return (static_cast<std::uint32_t>(png[offset]) << 24U) |
           (static_cast<std::uint32_t>(png[offset + 1U]) << 16U) |
           (static_cast<std::uint32_t>(png[offset + 2U]) << 8U) |
           static_cast<std::uint32_t>(png[offset + 3U]);
  };
  return check(big_endian(16U) == 2U && big_endian(20U) == 2U, "png IHDR dimensions");
}

bool installation_tests() {
  const auto directory = std::filesystem::temp_directory_path() /
                         (L"mactype-preview-unit-" + std::to_wstring(GetCurrentProcessId()));
  std::error_code ignored;
  std::filesystem::remove_all(directory, ignored);
  if (!std::filesystem::create_directory(directory, ignored)) {
    return check(false, "installation temp directory creation");
  }
  const auto regular = directory / L"regular.bin";
  const auto pe = directory / L"x86.dll";
  {
    std::ofstream output(regular, std::ios::binary);
    output << "data";
  }
  {
    std::array<std::uint8_t, 256> bytes{};
    auto* dos = reinterpret_cast<IMAGE_DOS_HEADER*>(bytes.data());
    dos->e_magic = IMAGE_DOS_SIGNATURE;
    dos->e_lfanew = 128;
    auto* signature = reinterpret_cast<DWORD*>(bytes.data() + dos->e_lfanew);
    *signature = IMAGE_NT_SIGNATURE;
    auto* header = reinterpret_cast<IMAGE_FILE_HEADER*>(signature + 1);
    header->Machine = IMAGE_FILE_MACHINE_I386;
    std::ofstream output(pe, std::ios::binary);
    output.write(reinterpret_cast<const char*>(bytes.data()),
                 static_cast<std::streamsize>(bytes.size()));
  }
  const bool passed = check(mactype::regular_file(regular.wstring()), "installation regular file") &&
                      check(!mactype::regular_file(directory.wstring()), "installation directory rejected") &&
                      check(mactype::x86_image(pe.wstring()), "installation x86 image") &&
                      check(!mactype::x86_image(regular.wstring()), "installation invalid image rejected") &&
                      check(!mactype::full_path(regular.wstring()).empty(), "installation full path");
  std::filesystem::remove_all(directory, ignored);
  return passed;
}

}  // namespace

int main() {
  if (!json_tests()) return 1;
  if (!png_tests()) return 1;
  if (!installation_tests()) return 1;
  std::cout << "preview unit tests passed\n";
  return 0;
}
