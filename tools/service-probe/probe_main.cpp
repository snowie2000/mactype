#include "probe_common.h"

#include <shellapi.h>

#include <iostream>
#include <string>

namespace {

void PrintUsage() {
  std::wcerr << L"Usage: probe-{console|window}{32|64}.exe --out <result.json> "
                L"[--wait-ms <milliseconds>] [--role <name>]\n";
}

#if defined(SERVICE_PROBE_WINDOW)
LRESULT CALLBACK ProbeWindowProcedure(const HWND window, const UINT message,
                                      const WPARAM wparam, const LPARAM lparam) {
  switch (message) {
    case WM_PAINT: {
      PAINTSTRUCT paint{};
      const HDC dc = BeginPaint(window, &paint);
      const wchar_t text[] = L"MacType service characterization window probe";
      TextOutW(dc, 20, 24, text, static_cast<int>(std::size(text) - 1));
      EndPaint(window, &paint);
      return 0;
    }
    case WM_CLOSE:
      DestroyWindow(window);
      return 0;
    default:
      return DefWindowProcW(window, message, wparam, lparam);
  }
}

int RunWindowProbe(const HINSTANCE instance, const int argc, wchar_t** argv) {
  mactype::service_probe::CommonArguments arguments;
  arguments.options.probe_kind = L"window";
  std::wstring error;
  if (!mactype::service_probe::ParseCommonArguments(argc, argv, arguments, error)) {
    std::wcerr << error << L'\n';
    PrintUsage();
    return 64;
  }
  if (arguments.show_help) {
    PrintUsage();
    return 0;
  }

  constexpr wchar_t window_class[] = L"MacTypeServiceProbeWindow";
  WNDCLASSW registration{};
  registration.hInstance = instance;
  registration.lpfnWndProc = ProbeWindowProcedure;
  registration.lpszClassName = window_class;
  registration.hCursor = LoadCursorW(nullptr, IDC_ARROW);
  registration.hbrBackground = static_cast<HBRUSH>(GetStockObject(WHITE_BRUSH));
  if (RegisterClassW(&registration) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS) {
    return 65;
  }
  const HWND window = CreateWindowExW(
      0, window_class, L"MacType Service Window Probe", WS_OVERLAPPEDWINDOW,
      CW_USEDEFAULT, CW_USEDEFAULT, 560, 160, nullptr, nullptr, instance, nullptr);
  if (window == nullptr) {
    return 66;
  }
  ShowWindow(window, SW_SHOWNORMAL);
  UpdateWindow(window);
  const int result = mactype::service_probe::ObserveAndWrite(
      arguments.options, true, error);
  if (IsWindow(window) != FALSE) {
    DestroyWindow(window);
  }
  if (result != 0) {
    std::wcerr << error << L'\n';
  }
  return result;
}
#endif

}  // namespace

#if defined(SERVICE_PROBE_WINDOW)
int WINAPI wWinMain(const HINSTANCE instance, HINSTANCE, wchar_t*, int) {
  int argc = 0;
  wchar_t** argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  if (argv == nullptr) {
    return 63;
  }
  const int result = RunWindowProbe(instance, argc, argv);
  LocalFree(argv);
  return result;
}
#else
int wmain(const int argc, wchar_t** argv) {
  mactype::service_probe::CommonArguments arguments;
  arguments.options.probe_kind = L"console";
  std::wstring error;
  if (!mactype::service_probe::ParseCommonArguments(argc, argv, arguments, error)) {
    std::wcerr << error << L'\n';
    PrintUsage();
    return 64;
  }
  if (arguments.show_help) {
    PrintUsage();
    return 0;
  }
  const int result =
      mactype::service_probe::ObserveAndWrite(arguments.options, false, error);
  if (result != 0) {
    std::wcerr << error << L'\n';
  }
  return result;
}
#endif

