#include "installation_check.h"

#include <Windows.h>

#include <fstream>

namespace mactype {

std::wstring full_path(const std::wstring& path) {
  const DWORD required = GetFullPathNameW(path.c_str(), 0, nullptr, nullptr);
  if (required == 0) return {};
  std::wstring result(required, L'\0');
  const DWORD written = GetFullPathNameW(path.c_str(), required, result.data(), nullptr);
  if (written == 0 || written >= required) return {};
  result.resize(written);
  return result;
}

bool regular_file(const std::wstring& path) {
  const DWORD attributes = GetFileAttributesW(path.c_str());
  return attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

bool x86_image(const std::wstring& path) {
  std::ifstream input(path, std::ios::binary);
  IMAGE_DOS_HEADER dos{};
  input.read(reinterpret_cast<char*>(&dos), sizeof(dos));
  if (!input || dos.e_magic != IMAGE_DOS_SIGNATURE || dos.e_lfanew <= 0) return false;
  input.seekg(dos.e_lfanew, std::ios::beg);
  DWORD signature{};
  IMAGE_FILE_HEADER header{};
  input.read(reinterpret_cast<char*>(&signature), sizeof(signature));
  input.read(reinterpret_cast<char*>(&header), sizeof(header));
  return input && signature == IMAGE_NT_SIGNATURE && header.Machine == IMAGE_FILE_MACHINE_I386;
}

}  // namespace mactype
