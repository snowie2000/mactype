#pragma once

#include "preview_runtime.h"

#include <algorithm>
#include <array>
#include <filesystem>
#include <fstream>
#include <iostream>

namespace mactype {

struct PreviewRuntimeTestAccess {
  static bool capture_toolbar(PreviewRuntime& runtime, const std::filesystem::path& path) {
    RECT client{};
    GetClientRect(runtime.native_window_, &client);
    const int width = client.right;
    const int height = runtime.toolbar_layout_height_;
    BITMAPINFO info{};
    info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    void* pixels = nullptr;
    HDC dc = CreateCompatibleDC(nullptr);
    HBITMAP bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &pixels, nullptr, 0);
    if (!bitmap) { DeleteDC(dc); return false; }
    HGDIOBJ previous = SelectObject(dc, bitmap);
    runtime.draw_toolbar(dc, RECT{0, 0, width, height});
    GdiFlush();
    BITMAPFILEHEADER file{};
    file.bfType = 0x4D42;
    file.bfOffBits = sizeof(file) + sizeof(info.bmiHeader);
    const DWORD pixel_bytes = static_cast<DWORD>(width * height * 4);
    file.bfSize = file.bfOffBits + pixel_bytes;
    std::ofstream out(path, std::ios::binary);
    out.write(reinterpret_cast<const char*>(&file), sizeof(file));
    out.write(reinterpret_cast<const char*>(&info.bmiHeader), sizeof(info.bmiHeader));
    out.write(static_cast<const char*>(pixels), pixel_bytes);
    const bool saved = out.good();
    SelectObject(dc, previous);
    DeleteObject(bitmap);
    DeleteDC(dc);
    return saved;
  }

  static bool toolbar_labels(PreviewRuntime& runtime, const std::string& skin) {
    mtpc::Frame request;
    request.kind = mtpc::MessageKind::show_native_preview;
    const int row_height = skin == "classic" ? 44 : skin == "fluent" ? 48 : skin == "console" ? 36 : 40;
    const int control_height = skin == "classic" || skin == "fluent" ? 32 : 26;
    request.json = std::string{R"({"chrome":{"skin":")"} + skin +
        R"(","canvas":"#E4E8EC","surface":"#F4F6F8","surfaceSubtle":"#FFFFFF","border":"#D2D8DF","text":"#1B2129","muted":"#5A6673","accent":"#0B8E9F","onAccent":"#FFFFFF","radius":4,"controlHeight":)" +
        std::to_string(control_height) + R"(,"toolbarHeight":)" + std::to_string(row_height) +
        R"(,"statusHeight":24,"canvasRadius":4,"canvasInset":10,"monoStatus":false}})";
    if (runtime.show_native_preview(request, true).kind != mtpc::MessageKind::native_preview_state) return false;
    std::array<wchar_t, 32768> evidence_path{};
    GetEnvironmentVariableW(L"MACTYPE_TOOLBAR_EVIDENCE_DIR", evidence_path.data(),
                            static_cast<DWORD>(evidence_path.size()));
    int cases = 0;
    for (bool korean : {false, true}) {
      runtime.labels_.font_face = korean ? L"미리보기 글꼴" : L"Preview font";
      runtime.labels_.font_size = korean ? L"글꼴 크기" : L"Font size";
      runtime.labels_.bold = korean ? L"굵게" : L"Bold";
      runtime.labels_.italic = korean ? L"기울임" : L"Italic";
      runtime.labels_.mode_sample = korean ? L"견본" : L"Sample";
      runtime.labels_.mode_ladder = korean ? L"크기 사다리" : L"Size ladder";
      runtime.labels_.mode_compare = korean ? L"윈도우" : L"Compare with Windows";
      runtime.labels_.mode_listing = korean ? L"나열 표시" : L"Listing";
      runtime.labels_.invert = korean ? L"색 반전" : L"Invert colours";
      runtime.labels_.loupe = korean ? L"확대경" : L"Loupe";
      runtime.labels_.zoom = korean ? L"확대" : L"Zoom";
      runtime.labels_.topmost = korean ? L"항상 위" : L"Always on top";
      runtime.labels_.edit_text = korean ? L"예시 문장 편집" : L"Edit sample text";
      runtime.labels_.save_png = korean ? L"PNG로 저장" : L"Save PNG";
      runtime.labels_.copy = korean ? L"복사" : L"Copy";
      const std::vector<std::wstring> expected{
          runtime.labels_.bold, runtime.labels_.italic, runtime.labels_.mode_sample,
          runtime.labels_.mode_ladder, runtime.labels_.mode_compare, runtime.labels_.mode_listing,
          runtime.labels_.invert, runtime.labels_.loupe, runtime.labels_.zoom + L" 1x",
          runtime.labels_.topmost, runtime.labels_.edit_text, runtime.labels_.save_png, runtime.labels_.copy};
      for (std::uint32_t dpi : {96U, 144U, 192U}) {
        runtime.native_dpi_ = dpi;
        runtime.recreate_ui_font();
        runtime.relayout_controls();
        const std::array<int, 3> widths = dpi == 96
            ? std::array<int, 3>{720, 1000, 1800} : std::array<int, 3>{720, 800, 900};
        for (int width : widths) {
          RECT window{};
          RECT before{};
          GetWindowRect(runtime.native_window_, &window);
          GetClientRect(runtime.native_window_, &before);
          const int wanted_width = MulDiv(width, static_cast<int>(dpi), 96);
          MINMAXINFO minimum{};
          SendMessageW(runtime.native_window_, WM_GETMINMAXINFO, 0, reinterpret_cast<LPARAM>(&minimum));
          // Injected DPI changes control metrics, not the test monitor's non-client frame.
          const int frame_width = window.right - window.left - before.right;
          const int expected_width = std::max(
              std::min(wanted_width, GetSystemMetrics(SM_CXMAXTRACK) - frame_width),
              static_cast<int>(minimum.ptMinTrackSize.x) - frame_width);
          SetWindowPos(runtime.native_window_, nullptr, 0, 0,
                       wanted_width + window.right - window.left - before.right,
                       MulDiv(600, static_cast<int>(dpi), 96) + window.bottom - window.top - before.bottom,
                       SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
          runtime.relayout_controls();
          if (runtime.toolbar_button_texts_ != expected) {
            std::cerr << "Toolbar labels were abbreviated\n"; return false;
          }
          RECT client{};
          GetClientRect(runtime.native_window_, &client);
          HDC dc = GetDC(runtime.native_window_);
          HGDIOBJ previous = SelectObject(dc, runtime.ui_font_);
          bool fits = client.right == expected_width;
          for (std::size_t i = 0; i < expected.size(); ++i) {
            const auto& [action, rect] = runtime.toolbar_buttons_[i];
            SIZE extent{};
            GetTextExtentPoint32W(dc, expected[i].c_str(), static_cast<int>(expected[i].size()), &extent);
            fits = fits && rect.left >= 0 && rect.right <= client.right &&
                rect.top >= 0 && rect.bottom <= runtime.toolbar_layout_height_ &&
                rect.right - rect.left >= extent.cx + MulDiv(20, static_cast<int>(dpi), 96) &&
                rect.bottom - rect.top >= extent.cy &&
                runtime.hit_test_toolbar(POINT{(rect.left + rect.right) / 2,
                                               (rect.top + rect.bottom) / 2}) == action;
            for (std::size_t j = 0; j < i; ++j) {
              RECT overlap{};
              fits = fits && !IntersectRect(&overlap, &rect, &runtime.toolbar_buttons_[j].second);
            }
          }
          for (const auto& entry : {std::pair{runtime.labels_.font_face, runtime.face_label_rect_},
                                   std::pair{runtime.labels_.font_size, runtime.size_label_rect_}}) {
            SIZE extent{};
            GetTextExtentPoint32W(dc, entry.first.c_str(), static_cast<int>(entry.first.size()), &extent);
            fits = fits && entry.second.right - entry.second.left >= extent.cx;
          }
          SelectObject(dc, previous);
          ReleaseDC(runtime.native_window_, dc);
          RECT edit{};
          GetWindowRect(runtime.edit_control_, &edit);
          MapWindowPoints(nullptr, runtime.native_window_, reinterpret_cast<POINT*>(&edit), 2);
          fits = fits && edit.top >= runtime.toolbar_layout_height_;
          const RECT& invert = runtime.toolbar_buttons_[6].second;
          const LPARAM point = MAKELPARAM((invert.left + invert.right) / 2,
                                         (invert.top + invert.bottom) / 2);
          const bool was_inverted = runtime.inverted_;
          SendMessageW(runtime.native_window_, WM_LBUTTONDOWN, MK_LBUTTON, point);
          SendMessageW(runtime.native_window_, WM_LBUTTONUP, 0, point);
          fits = fits && runtime.inverted_ != was_inverted;
          SendMessageW(runtime.native_window_, WM_LBUTTONDOWN, MK_LBUTTON, point);
          SendMessageW(runtime.native_window_, WM_LBUTTONUP, 0, point);
          fits = fits && runtime.inverted_ == was_inverted;
          if (!fits) { std::cerr << "Toolbar layout failed: " << korean << " " << dpi << " " << width << '\n'; return false; }
          if (evidence_path[0] && dpi == 96 && korean) {
            const auto path = std::filesystem::path(evidence_path.data()) /
                (std::string{"toolbar-"} + skin + "-ko-" + std::to_string(width) + ".bmp");
            if (!capture_toolbar(runtime, path)) return false;
          }
          ++cases;
        }
      }
    }
    runtime.show_native_preview(request, false);
    std::cout << skin << ": full toolbar labels, bounds, hit targets and editor offset: " << cases << " cases passed\n";
    return true;
  }
};

}
