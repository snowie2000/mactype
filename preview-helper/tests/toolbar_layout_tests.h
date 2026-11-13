#pragma once

#include "preview_runtime.h"

#include <algorithm>
#include <array>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string_view>

namespace mactype {
namespace {

bool toolbar_failure(std::string_view name, bool korean, std::uint32_t dpi, int width) {
  std::cerr << "FAILED: toolbar." << name << " locale=" << (korean ? "ko" : "en")
            << " dpi=" << dpi << " width=" << width << '\n';
  return false;
}

HWND visible_thread_window() {
  HWND result{};
  EnumThreadWindows(
      GetCurrentThreadId(),
      [](HWND window, LPARAM data) {
        if (IsWindowVisible(window)) {
          *reinterpret_cast<HWND*>(data) = window;
          return FALSE;
        }
        return TRUE;
      },
      reinterpret_cast<LPARAM>(&result));
  return result;
}

bool write_toolbar_capture(const PreviewRuntime::ToolbarCapture& capture,
                           const std::filesystem::path& path) {
  BITMAPINFOHEADER info{};
  info.biSize = sizeof(info);
  info.biWidth = capture.width;
  info.biHeight = -capture.height;
  info.biPlanes = 1;
  info.biBitCount = 32;
  info.biCompression = BI_RGB;
  BITMAPFILEHEADER file{};
  file.bfType = 0x4D42;
  file.bfOffBits = sizeof(file) + sizeof(info);
  const DWORD pixel_bytes = static_cast<DWORD>(capture.pixels.size() * sizeof(std::uint32_t));
  file.bfSize = file.bfOffBits + pixel_bytes;
  std::ofstream out(path, std::ios::binary);
  out.write(reinterpret_cast<const char*>(&file), sizeof(file));
  out.write(reinterpret_cast<const char*>(&info), sizeof(info));
  out.write(reinterpret_cast<const char*>(capture.pixels.data()), pixel_bytes);
  return out.good();
}

bool toolbar_labels(PreviewRuntime& runtime) {
  struct LabelCase {
    bool korean;
    const char* json;
    std::array<std::wstring_view, 13> expected;
  };
  const std::array<LabelCase, 2> label_cases{{
      {false,
       R"({"fontFace":"Preview font","fontSize":"Font size","bold":"Bold","italic":"Italic","modeSample":"Sample","modeLadder":"Size ladder","modeCompare":"Compare with Windows","modeListing":"Listing","invert":"Invert colours","loupe":"Loupe","zoom":"Zoom","topmost":"Always on top","editText":"Edit sample text","savePng":"Save PNG","copy":"Copy"})",
       {L"Bold", L"Italic", L"Sample", L"Size ladder", L"Compare with Windows", L"Listing",
        L"Invert colours", L"Loupe", L"Zoom 1x", L"Always on top", L"Edit sample text",
        L"Save PNG", L"Copy"}},
      {true,
       R"({"fontFace":"미리보기 글꼴","fontSize":"글꼴 크기","bold":"굵게","italic":"기울임","modeSample":"견본","modeLadder":"크기 사다리","modeCompare":"윈도우","modeListing":"나열 표시","invert":"색 반전","loupe":"확대경","zoom":"확대","topmost":"항상 위","editText":"예시 문장 편집","savePng":"PNG로 저장","copy":"복사"})",
       {L"굵게", L"기울임", L"견본", L"크기 사다리", L"윈도우", L"나열 표시", L"색 반전",
        L"확대경", L"확대 1x", L"항상 위", L"예시 문장 편집", L"PNG로 저장", L"복사"}},
  }};
  constexpr std::string_view chrome =
      R"({"skin":"classic","canvas":"#E4E8EC","surface":"#F4F6F8","surfaceSubtle":"#FFFFFF","border":"#D2D8DF","text":"#1B2129","muted":"#5A6673","accent":"#0B8E9F","onAccent":"#FFFFFF","radius":4,"controlHeight":32,"toolbarHeight":44,"statusHeight":24,"canvasRadius":4,"canvasInset":10,"monoStatus":false})";
  std::array<wchar_t, 32768> evidence_path{};
  GetEnvironmentVariableW(L"MACTYPE_TOOLBAR_EVIDENCE_DIR", evidence_path.data(),
                          static_cast<DWORD>(evidence_path.size()));
  int cases = 0;
  for (const auto& label_case : label_cases) {
    mtpc::Frame request;
    request.kind = mtpc::MessageKind::show_native_preview;
    request.json = std::string{"{\"chrome\":"} + std::string{chrome} +
                   ",\"labels\":" + label_case.json + "}";
    if (runtime.show_native_preview(request, true).kind !=
        mtpc::MessageKind::native_preview_state) {
      return toolbar_failure("request", label_case.korean, 0, 0);
    }
    for (std::uint32_t dpi : {96U, 144U, 192U}) {
      if (!runtime.set_dpi_for_tests(dpi)) {
        return toolbar_failure("dpi", label_case.korean, dpi, 0);
      }
      const std::array<int, 3> widths = dpi == 96
          ? std::array<int, 3>{720, 1000, 1800}
          : std::array<int, 3>{720, 800, 900};
      for (int width : widths) {
        const HWND window = visible_thread_window();
        RECT window_rectangle{};
        RECT before{};
        if (!window || !GetWindowRect(window, &window_rectangle) ||
            !GetClientRect(window, &before)) {
          return toolbar_failure("window", label_case.korean, dpi, width);
        }
        const int wanted_width = MulDiv(width, static_cast<int>(dpi), 96);
        const int wanted_height = MulDiv(600, static_cast<int>(dpi), 96);
        MINMAXINFO minimum{};
        SendMessageW(window, WM_GETMINMAXINFO, 0, reinterpret_cast<LPARAM>(&minimum));
        // Synthetic DPI changes control metrics, not the test monitor's non-client frame.
        const int frame_width = window_rectangle.right - window_rectangle.left - before.right;
        const int expected_width = std::max(
            std::min(wanted_width, GetSystemMetrics(SM_CXMAXTRACK) - frame_width),
            static_cast<int>(minimum.ptMinTrackSize.x) - frame_width);
        if (!SetWindowPos(window, nullptr, 0, 0, wanted_width + frame_width,
                          wanted_height + window_rectangle.bottom - window_rectangle.top -
                              before.bottom,
                          SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE)) {
          return toolbar_failure("resize", label_case.korean, dpi, width);
        }
        const auto snapshot = runtime.toolbar_snapshot_for_tests();
        if (snapshot.client_width != expected_width) {
          return toolbar_failure("client-width", label_case.korean, dpi, width);
        }
        if (snapshot.buttons.size() != label_case.expected.size()) {
          return toolbar_failure("button-count", label_case.korean, dpi, width);
        }
        bool fits = true;
        for (std::size_t index = 0; index < snapshot.buttons.size(); ++index) {
          const auto& button = snapshot.buttons[index];
          const RECT& rectangle = button.rectangle;
          fits = fits && button.text == label_case.expected[index] && rectangle.left >= 0 &&
                 rectangle.right <= snapshot.client_width && rectangle.top >= 0 &&
                 rectangle.bottom <= snapshot.layout_height &&
                 rectangle.right - rectangle.left >=
                     button.text_extent.cx + MulDiv(20, static_cast<int>(dpi), 96) &&
                 rectangle.bottom - rectangle.top >= button.text_extent.cy &&
                 button.center_hit_id == button.id;
          for (std::size_t previous = 0; previous < index; ++previous) {
            RECT overlap{};
            fits = fits && !IntersectRect(&overlap, &rectangle,
                                          &snapshot.buttons[previous].rectangle);
          }
        }
        for (const auto& label : {snapshot.face_label, snapshot.size_label}) {
          fits = fits && label.rectangle.right - label.rectangle.left >= label.text_extent.cx;
        }
        fits = fits && snapshot.edit_rectangle.top >= snapshot.layout_height;
        if (!fits) {
          return toolbar_failure("layout-or-hit-target", label_case.korean, dpi, width);
        }
        const RECT invert = snapshot.buttons[6].rectangle;
        const LPARAM invert_point = MAKELPARAM((invert.left + invert.right) / 2,
                                               (invert.top + invert.bottom) / 2);
        SendMessageW(window, WM_LBUTTONDOWN, MK_LBUTTON, invert_point);
        SendMessageW(window, WM_LBUTTONUP, 0, invert_point);
        const mtpc::Frame inverted = runtime.show_native_preview(request, true);
        if (inverted.json.find("\"inverted\":true") == std::string::npos) {
          return toolbar_failure("invert-on", label_case.korean, dpi, width);
        }
        SendMessageW(window, WM_LBUTTONDOWN, MK_LBUTTON, invert_point);
        SendMessageW(window, WM_LBUTTONUP, 0, invert_point);
        const mtpc::Frame restored = runtime.show_native_preview(request, true);
        if (restored.json.find("\"inverted\":false") == std::string::npos) {
          return toolbar_failure("invert-off", label_case.korean, dpi, width);
        }
        const auto capture = runtime.capture_toolbar_for_tests();
        if (!capture || capture->width != snapshot.client_width ||
            capture->height != snapshot.layout_height ||
            capture->pixels.size() !=
                static_cast<std::size_t>(capture->width) * capture->height) {
          return toolbar_failure("capture-bounds", label_case.korean, dpi, width);
        }
        if (evidence_path[0] && dpi == 96 && label_case.korean) {
          const auto path = std::filesystem::path(evidence_path.data()) /
                            (std::string{"toolbar-classic-ko-"} +
                             std::to_string(width) + ".bmp");
          if (!write_toolbar_capture(*capture, path)) {
            return toolbar_failure("capture-write", label_case.korean, dpi, width);
          }
        }
        ++cases;
      }
    }
  }
  mtpc::Frame hide;
  hide.kind = mtpc::MessageKind::show_native_preview;
  hide.json = "{}";
  if (runtime.show_native_preview(hide, false).kind !=
      mtpc::MessageKind::native_preview_state) {
    return toolbar_failure("hide", false, 0, 0);
  }
  std::cout << "classic: full toolbar labels, bounds, hit targets and editor offset: " << cases
            << " cases passed\n";
  return true;
}

}  // namespace
}  // namespace mactype
