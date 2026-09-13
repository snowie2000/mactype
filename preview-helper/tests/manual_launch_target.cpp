#include <Windows.h>

#include <fstream>

int wmain(int argc, wchar_t** argv) {
  if (argc != 2) return 2;
  std::ofstream marker(argv[1], std::ios::binary | std::ios::trunc);
  if (!marker) return 3;
  marker << "mactype-manual-launch-ready\n";
  marker.flush();
  return marker ? 0 : 4;
}
