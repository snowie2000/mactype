#include "preview_runtime.h"

#include "generated_settings.h"
#include "generated_native_preview.h"
#include "installation_check.h"
#include "json_document.h"
#include "png_encoder.h"

#include <CommCtrl.h>
#include <CommDlg.h>
#include <Shlwapi.h>
#include <Windowsx.h>
#include <Uxtheme.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <limits>
#include <optional>
#include <set>
#include <sstream>
#include <vector>

namespace mactype {
namespace {

constexpr wchar_t kWindowClass[] = L"MacTypePreview32Window";
constexpr UINT_PTR kStatusTimer = 1;
constexpr UINT kSaveComplete = WM_APP + 1;
constexpr int kFaceCombo = 1001;
constexpr int kSizeCombo = 1002;
constexpr int kEditControl = 1003;

enum Action {
  kNoAction,
  kBold,
  kItalic,
  kModeSample,
  kModeLadder,
  kModeCompare,
  kModeListing,
  kInvert,
  kLoupe,
  kZoom,
  kTopmost,
  kEditText,
  kSavePng,
  kCopy,
};

constexpr PreviewRuntime::Palette palette_from(const GeneratedNativePalette& value) {
  return {value.canvas, value.surface, value.hover, value.border, value.text, value.muted,
          value.accent, value.on_accent};
}

constexpr PreviewRuntime::NativeChrome chrome_from(const GeneratedNativeChrome& value) {
  return {PreviewRuntime::Skin::classic, palette_from(kNativeLightPalette), value.radius,
          value.control_height, value.toolbar_height, value.status_height, value.canvas_radius,
          value.canvas_inset, value.mono_status};
}

constexpr PreviewRuntime::Palette kLightPalette = palette_from(kNativeLightPalette);
constexpr PreviewRuntime::Palette kDarkPalette = palette_from(kNativeDarkPalette);

int scaled(int logical, UINT dpi) { return MulDiv(logical, static_cast<int>(dpi), 96); }

std::wstring utf8_to_wide(const std::string& value) {
  if (value.empty()) return {};
  const int required = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(),
                                           static_cast<int>(value.size()), nullptr, 0);
  if (required <= 0) return {};
  std::wstring result(required, L'\0');
  if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(), static_cast<int>(value.size()),
                          result.data(), required) != required) {
    return {};
  }
  return result;
}

std::string wide_to_utf8(const std::wstring& value) {
  if (value.empty()) return {};
  const int required = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value.data(),
                                           static_cast<int>(value.size()), nullptr, 0, nullptr, nullptr);
  if (required <= 0) return {};
  std::string result(static_cast<std::size_t>(required), '\0');
  if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, value.data(),
                          static_cast<int>(value.size()), result.data(), required, nullptr,
                          nullptr) != required) {
    return {};
  }
  return result;
}

std::string json_escape_string(const std::string& value) {
  constexpr char hex[] = "0123456789ABCDEF";
  std::string escaped;
  escaped.reserve(value.size());
  for (const unsigned char character : value) {
    switch (character) {
      case '"': escaped += "\\\""; break;
      case '\\': escaped += "\\\\"; break;
      case '\b': escaped += "\\b"; break;
      case '\f': escaped += "\\f"; break;
      case '\n': escaped += "\\n"; break;
      case '\r': escaped += "\\r"; break;
      case '\t': escaped += "\\t"; break;
      default:
        if (character < 0x20U) {
          escaped += "\\u00";
          escaped.push_back(hex[character >> 4U]);
          escaped.push_back(hex[character & 0x0FU]);
        } else {
          escaped.push_back(static_cast<char>(character));
        }
        break;
    }
  }
  return escaped;
}

COLORREF parse_color(const std::string& value, COLORREF fallback) {
  if (value.size() != 7 || value[0] != '#') return fallback;
  unsigned int color{};
  std::istringstream input(value.substr(1));
  input >> std::hex >> color;
  if (!input || !input.eof()) return fallback;
  return RGB((color >> 16U) & 0xFFU, (color >> 8U) & 0xFFU, color & 0xFFU);
}

std::optional<COLORREF> parse_color(const std::string& value) {
  if (value.size() != 7 || value[0] != '#') return std::nullopt;
  unsigned int color{};
  std::istringstream input(value.substr(1));
  input >> std::hex >> color;
  if (!input || !input.eof()) return std::nullopt;
  return RGB((color >> 16U) & 0xFFU, (color >> 8U) & 0xFFU, color & 0xFFU);
}

std::string color_to_hex(COLORREF color) {
  char buffer[8]{};
  std::snprintf(buffer, sizeof(buffer), "#%02X%02X%02X", GetRValue(color), GetGValue(color),
                GetBValue(color));
  return std::string{buffer};
}

mtpc::Frame error_frame(std::uint64_t request_id, const char* code, const std::string& message) {
  mtpc::Frame response;
  response.kind = mtpc::MessageKind::error;
  response.request_id = request_id;
  std::string safe = message;
  std::replace(safe.begin(), safe.end(), '"', '\'');
  response.json = std::string{"{\"code\":\""} + code + "\",\"message\":\"" + safe +
                  "\",\"recoverable\":true}";
  return response;
}

int point_size_px(float point_size, std::uint32_t dpi) {
  return MulDiv(static_cast<int>(std::lround(point_size * 100.0F)), static_cast<int>(dpi), 7200);
}

HFONT create_sample_font(const std::wstring& face, float point_size, UINT dpi, bool bold,
                         bool italic) {
  return CreateFontW(-point_size_px(point_size, dpi), 0, 0, 0, bold ? FW_BOLD : FW_NORMAL,
                     italic ? TRUE : FALSE, FALSE, FALSE, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
                     CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
                     face.c_str());
}

void fill_solid(HDC dc, const RECT& area, COLORREF color) {
  HBRUSH brush = CreateSolidBrush(color);
  FillRect(dc, &area, brush);
  DeleteObject(brush);
}

void draw_sample(HDC dc, const RECT& area, const PreviewRuntime::SampleState& sample,
                 std::uint32_t dpi, COLORREF foreground, COLORREF background) {
  fill_solid(dc, area, background);
  HFONT font = create_sample_font(sample.font_face, sample.font_size_pt, dpi, sample.bold,
                                  sample.italic);
  HGDIOBJ previous_font = SelectObject(dc, font);
  SetTextColor(dc, foreground);
  SetBkMode(dc, TRANSPARENT);
  int y = area.top + std::max(8, point_size_px(sample.font_size_pt, dpi) / 2);
  std::size_t start = 0;
  while (start <= sample.text.size()) {
    const std::size_t end = sample.text.find(L'\n', start);
    const std::size_t length = (end == std::wstring::npos ? sample.text.size() : end) - start;
    ExtTextOutW(dc, area.left + 18, y, ETO_CLIPPED, &area, sample.text.data() + start,
                static_cast<UINT>(length), nullptr);
    y += std::max(22, point_size_px(sample.font_size_pt, dpi) * 3 / 2);
    if (end == std::wstring::npos) break;
    start = end + 1;
  }
  SelectObject(dc, previous_font);
  DeleteObject(font);
}

constexpr float kListingSmallPt = 9.0F;
constexpr float kListingLargePt = 14.0F;
constexpr COLORREF kListingColorsOnLight[] = {RGB(0x00, 0x00, 0x00), RGB(0xC8, 0x00, 0x00),
                                              RGB(0x00, 0x8A, 0x00), RGB(0x00, 0x00, 0xC8)};
constexpr COLORREF kListingColorsOnDark[] = {RGB(0xF1, 0xF3, 0xF5), RGB(0xFF, 0x6B, 0x6B),
                                             RGB(0x51, 0xCF, 0x66), RGB(0x74, 0xC0, 0xFC)};

bool is_dark(COLORREF color) {
  const int luma = (299 * GetRValue(color) + 587 * GetGValue(color) + 114 * GetBValue(color)) / 1000;
  return luma < 128;
}

int listing_line_advance(float point_size, std::uint32_t dpi) {
  return std::max(12, point_size_px(point_size, dpi) * 3 / 2);
}

int listing_content_height(std::uint32_t dpi) {
  const int margin = scaled(12, dpi);
  const int size_gap = scaled(6, dpi);
  const int group_gap = scaled(14, dpi);
  const int group = 4 * listing_line_advance(kListingSmallPt, dpi) + size_gap +
                    4 * listing_line_advance(kListingLargePt, dpi);
  return 2 * margin + 2 * group + group_gap;
}

void draw_listing(HDC dc, const RECT& area, const std::wstring& text,
                  const PreviewRuntime::SampleState& sample, std::uint32_t dpi,
                  COLORREF background) {
  fill_solid(dc, area, background);
  SetBkMode(dc, TRANSPARENT);
  const COLORREF* colors = is_dark(background) ? kListingColorsOnDark : kListingColorsOnLight;
  const int size_gap = scaled(6, dpi);
  const int group_gap = scaled(14, dpi);
  int y = area.top + scaled(12, dpi);
  for (const int weight : {FW_NORMAL, FW_BOLD}) {
    for (const float point_size : {kListingSmallPt, kListingLargePt}) {
      HFONT font = create_sample_font(sample.font_face, point_size, dpi, weight == FW_BOLD, false);
      HGDIOBJ previous_font = SelectObject(dc, font);
      for (int index = 0; index < 4; ++index) {
        SetTextColor(dc, colors[index]);
        ExtTextOutW(dc, area.left + 18, y, ETO_CLIPPED, &area, text.c_str(),
                    static_cast<UINT>(text.size()), nullptr);
        y += listing_line_advance(point_size, dpi);
      }
      SelectObject(dc, previous_font);
      DeleteObject(font);
      y += size_gap;
    }
    y += group_gap - size_gap;
  }
}

bool cjk_break_character(wchar_t character) {
  const unsigned value = static_cast<unsigned>(character);
  return (value >= 0x1100 && value <= 0x11FF) || (value >= 0x2E80 && value <= 0x303F) ||
         (value >= 0x3040 && value <= 0x30FF) || (value >= 0x3400 && value <= 0x4DBF) ||
         (value >= 0x4E00 && value <= 0x9FFF) || (value >= 0xA960 && value <= 0xA97F) ||
         (value >= 0xAC00 && value <= 0xD7FF) || (value >= 0xF900 && value <= 0xFAFF) ||
         (value >= 0xFF00 && value <= 0xFF60);
}

int wrapped_text_height(HDC dc, const RECT& area, const std::wstring& text, int line_height,
                        COLORREF color, bool draw = true) {
  SetTextColor(dc, color);
  SetBkMode(dc, TRANSPARENT);
  int y = area.top;
  std::size_t line_start = 0;
  while (line_start < text.size()) {
    if (text[line_start] == L'\n') {
      y += line_height;
      ++line_start;
      continue;
    }
    std::size_t best = line_start;
    std::size_t candidate = line_start;
    for (std::size_t index = line_start; index < text.size() && text[index] != L'\n'; ++index) {
      const bool break_here = text[index] == L' ' || cjk_break_character(text[index]);
      SIZE extent{};
      GetTextExtentPoint32W(dc, text.data() + line_start,
                            static_cast<int>(index - line_start + 1), &extent);
      if (extent.cx <= area.right - area.left) {
        best = index + 1;
        if (break_here) candidate = best;
      } else {
        break;
      }
    }
    if (best == line_start) best = std::min(line_start + 1, text.size());
    std::size_t end = candidate > line_start && best < text.size() ? candidate : best;
    while (end > line_start && text[end - 1] == L' ') --end;
    if (draw) {
      ExtTextOutW(dc, area.left, y, ETO_CLIPPED, &area, text.data() + line_start,
                  static_cast<UINT>(end - line_start), nullptr);
    }
    y += line_height;
    line_start = candidate > line_start && best < text.size() ? candidate : best;
    while (line_start < text.size() && text[line_start] == L' ') ++line_start;
    if (line_start < text.size() && text[line_start] == L'\n') ++line_start;
  }
  return y - area.top;
}

int measure_wrapped_text(HFONT font, int width, const std::wstring& text, int line_height) {
  HDC dc = CreateCompatibleDC(nullptr);
  if (!dc) return line_height;
  HGDIOBJ previous = SelectObject(dc, font);
  const RECT area{0, 0, std::max(1, width), std::numeric_limits<LONG>::max() / 2};
  const int height = wrapped_text_height(dc, area, text, line_height, RGB(0, 0, 0), false);
  SelectObject(dc, previous);
  DeleteDC(dc);
  return height;
}

std::wstring format_core_version(std::uint32_t version) {
  const std::wstring raw = std::to_wstring(version);
  if (raw.size() == 8 && raw.substr(0, 2) == L"20") {
    const int year = std::stoi(raw.substr(0, 4));
    const int month = std::stoi(raw.substr(4, 2));
    const int day = std::stoi(raw.substr(6, 2));
    if (month >= 1 && month <= 12 && day >= 1 && day <= 31) {
      return std::to_wstring(year) + L"." + std::to_wstring(month) + L"." + std::to_wstring(day);
    }
  }
  return raw;
}

COLORREF blend_color(COLORREF foreground, COLORREF background, int foreground_percent) {
  const int background_percent = 100 - foreground_percent;
  return RGB((GetRValue(foreground) * foreground_percent + GetRValue(background) * background_percent) / 100,
             (GetGValue(foreground) * foreground_percent + GetGValue(background) * background_percent) / 100,
             (GetBValue(foreground) * foreground_percent + GetBValue(background) * background_percent) / 100);
}

}  // namespace

struct PreviewRuntime::CanvasBitmap {
  HDC dc{};
  HBITMAP bitmap{};
  HGDIOBJ previous{};
  void* bits{};
  int width{};
  int height{};

  CanvasBitmap(HWND owner, int requested_width, int requested_height)
      : width(std::max(1, requested_width)), height(std::max(1, requested_height)) {
    HDC screen = GetDC(owner);
    dc = CreateCompatibleDC(screen);
    BITMAPINFO info{};
    info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &bits, nullptr, 0);
    ReleaseDC(owner, screen);
    if (dc && bitmap) previous = SelectObject(dc, bitmap);
  }

  ~CanvasBitmap() {
    if (dc && previous) SelectObject(dc, previous);
    if (bitmap) DeleteObject(bitmap);
    if (dc) DeleteDC(dc);
  }

  bool valid() const { return dc && bitmap && bits; }
};

PreviewRuntime::PreviewRuntime(std::wstring install_root, Engine engine)
    : engine_(engine),
      install_root_(engine == Engine::mactype ? full_path(install_root) : std::move(install_root)),
      dll_path_(install_root_ + LR"(\MacType.dll)"),
      ladder_sizes_(kNativeLadderSizes.begin(), kNativeLadderSizes.end()) {
  for (std::size_t index = 0; index < kNativeLabelBindings.size(); ++index) {
    labels_.*(kNativeLabelBindings[index].member) = kNativeLabelDefaults[index];
  }
}

PreviewRuntime::~PreviewRuntime() {
  if (save_thread_.joinable()) {
    if (save_in_progress_) save_thread_.detach();
    else save_thread_.join();
  }
  if (control_center_) {
    control_center_->DestroyMessageWnd();
    control_center_->Release();
  }
  if (native_window_) DestroyWindow(native_window_);
  if (hidden_window_) DestroyWindow(hidden_window_);
  if (ui_font_) DeleteObject(ui_font_);
  if (mono_font_) DeleteObject(mono_font_);
  if (surface_brush_) DeleteObject(surface_brush_);
  if (edit_brush_) DeleteObject(edit_brush_);
  // MacType installs process-wide hooks, so its module must remain mapped until process exit.
  if (com_initialized_) CoUninitialize();
}

bool PreviewRuntime::initialize(std::string& error) {
  if (engine_ == Engine::plain) {
    if (FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) {
      error = "COM initialization failed";
      return false;
    }
    com_initialized_ = true;
    return create_windows(error);
  }
  if (install_root_.empty() || !regular_file(dll_path_)) {
    error = "MacType.dll was not found in the selected installation root";
    return false;
  }
  if (!regular_file(install_root_ + L"\\MacType.ini")) {
    error = "MacType.ini is missing from the selected installation root";
    return false;
  }
  if (!x86_image(dll_path_)) {
    error = "MacType.dll is not an x86 PE image";
    return false;
  }
  if (FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) {
    error = "COM initialization failed";
    return false;
  }
  com_initialized_ = true;
  SetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", L"1");
  module_ = LoadLibraryExW(dll_path_.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH);
  if (!module_) {
    error = "LoadLibraryW failed for MacType.dll";
    return false;
  }
  const auto create = reinterpret_cast<CreateControlCenter>(GetProcAddress(module_, "CreateControlCenter"));
  const auto version = reinterpret_cast<DllGetVersion>(GetProcAddress(module_, "DllGetVersion"));
  if (!create) {
    error = "MacType.dll does not export CreateControlCenter";
    return false;
  }
  has_dll_get_version_ = version != nullptr;
  if (version) {
    DLLVERSIONINFO version_info{sizeof(DLLVERSIONINFO)};
    if (FAILED(version(&version_info))) {
      error = "DllGetVersion export returned an error";
      return false;
    }
  }
  create(&control_center_);
  if (!control_center_) {
    error = "CreateControlCenter returned no interface";
    return false;
  }
  core_version_ = control_center_->GetVersion();
  control_center_->EnableCache(FALSE);
  control_center_->EnableRender(TRUE);
  control_center_->CreateMessageWnd();
  return create_windows(error);
}

bool PreviewRuntime::create_windows(std::string& error) {
  WNDCLASSW window_class{};
  window_class.lpfnWndProc = window_proc;
  window_class.hInstance = GetModuleHandleW(nullptr);
  window_class.hCursor = LoadCursorW(nullptr, IDC_ARROW);
  window_class.lpszClassName = kWindowClass;
  RegisterClassW(&window_class);
  hidden_window_ = CreateWindowExW(0, kWindowClass, L"", WS_OVERLAPPED, 0, 0, 1, 1, nullptr,
                                   nullptr, window_class.hInstance, this);
  native_window_ = CreateWindowExW(0, kWindowClass, labels_.title.c_str(),
                                   WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN, CW_USEDEFAULT,
                                   CW_USEDEFAULT, 960, 560, nullptr, nullptr,
                                   window_class.hInstance, this);
  if (!hidden_window_ || !native_window_) {
    error = "failed to create preview windows";
    return false;
  }
  native_dpi_ = GetDpiForWindow(native_window_);
  RECT initial{0, 0, scaled(960, native_dpi_), scaled(560, native_dpi_)};
  AdjustWindowRectExForDpi(&initial, WS_OVERLAPPEDWINDOW, FALSE, 0, native_dpi_);
  SetWindowPos(native_window_, nullptr, 0, 0, initial.right - initial.left,
               initial.bottom - initial.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
  recreate_ui_font();
  recreate_palette_brushes();
  face_combo_ = CreateWindowExW(0, WC_COMBOBOXW, L"",
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST |
                                    CBS_OWNERDRAWFIXED | CBS_HASSTRINGS | WS_VSCROLL,
                                0, 0, 1, 1, native_window_, reinterpret_cast<HMENU>(kFaceCombo),
                                window_class.hInstance, nullptr);
  size_combo_ = CreateWindowExW(0, WC_COMBOBOXW, L"",
                                WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST |
                                    CBS_OWNERDRAWFIXED | CBS_HASSTRINGS | WS_VSCROLL,
                                0, 0, 1, 1, native_window_, reinterpret_cast<HMENU>(kSizeCombo),
                                window_class.hInstance, nullptr);
  edit_control_ = CreateWindowExW(0, L"EDIT", native_sample_.text.c_str(),
                                  WS_CHILD | WS_TABSTOP | ES_MULTILINE | ES_AUTOVSCROLL |
                                      WS_VSCROLL,
                                  0, 0, 1, 1, native_window_, reinterpret_cast<HMENU>(kEditControl),
                                  window_class.hInstance, nullptr);
  if (!face_combo_ || !size_combo_ || !edit_control_) {
    error = "failed to create preview controls";
    return false;
  }
  SendMessageW(edit_control_, EM_SETLIMITTEXT, 4096, 0);
  apply_combo_theme();
  recreate_ui_font();
  edit_original_proc_ = reinterpret_cast<WNDPROC>(SetWindowLongPtrW(
      edit_control_, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(edit_proc)));
  SetWindowLongPtrW(edit_control_, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
  enumerate_fonts();
  constexpr int sizes[] = {8, 9, 10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 36};
  for (int size : sizes) {
    const std::wstring text = std::to_wstring(size);
    SendMessageW(size_combo_, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(text.c_str()));
  }
  sync_controls();
  relayout_controls();
  return true;
}

std::string PreviewRuntime::hello_json() const {
  if (engine_ == Engine::plain) {
    return R"({"protocolVersion":1,"renderer":"gdi-plain","loadsMacType":false,"coreVersion":0,"dllGetVersion":false})";
  }
  return std::string{"{\"protocolVersion\":1,\"renderer\":\"mactype-gdi\",\"loadsMacType\":true,\"coreVersion\":"} +
         std::to_string(core_version_) + ",\"dllGetVersion\":" +
         (has_dll_get_version_ ? "true" : "false") + "}";
}

bool PreviewRuntime::apply_request(const JsonDocument& document, std::string& error) {
  if (engine_ == Engine::mactype) {
    if (!control_center_) {
      error = "MacType control center is unavailable";
      return false;
    }
    if (const auto profile = document.json_string("profilePath"); profile && !profile->empty()) {
      const std::wstring profile_path = full_path(utf8_to_wide(*profile));
      if (profile_path.empty() || !regular_file(profile_path) ||
          _wcsicmp(PathFindExtensionW(profile_path.c_str()), L".ini") != 0) {
        error = "profilePath is not an existing INI file";
        return false;
      }
      control_center_->LoadSetting(profile_path.c_str());
    }
    for (const auto& setting : kSettings) {
      const auto value = document.json_number(setting.id);
      if (!value) continue;
      const BOOL applied = setting.is_float
                               ? control_center_->SetFloatAttribute(setting.ordinal,
                                                                    static_cast<float>(*value))
                               : control_center_->SetIntAttribute(setting.ordinal,
                                                                  static_cast<int>(*value));
      if (!applied) {
        error = std::string{"MacType rejected setting "} + setting.id;
        return false;
      }
    }
    control_center_->RefreshSetting();
  }
  return true;
}

std::vector<std::uint8_t> PreviewRuntime::render_png(const JsonDocument& document,
                                                     std::uint32_t& width,
                                                     std::uint32_t& height, std::uint32_t& dpi,
                                                     std::string& error) {
  SampleState sample{L"MacType preview 123 ABC\nThe quick brown fox jumps over the lazy dog.",
                     L"Segoe UI", 14.0F, false, false};
  COLORREF foreground = RGB(24, 29, 35);
  COLORREF background = RGB(238, 241, 244);
  if (const auto value = document.json_string("text")) sample.text = utf8_to_wide(*value);
  if (const auto value = document.json_string("fontFace")) sample.font_face = utf8_to_wide(*value);
  if (const auto value = document.json_number("fontSizePt")) sample.font_size_pt = static_cast<float>(*value);
  if (const auto value = document.json_string("foreground")) foreground = parse_color(*value, foreground);
  if (const auto value = document.json_string("background")) background = parse_color(*value, background);
  if (const auto value = document.json_bool("bold")) sample.bold = *value;
  if (const auto value = document.json_bool("italic")) sample.italic = *value;
  width = static_cast<std::uint32_t>(document.json_number("widthPx").value_or(1000));
  height = static_cast<std::uint32_t>(document.json_number("heightPx").value_or(280));
  dpi = static_cast<std::uint32_t>(document.json_number("dpi").value_or(96.0));
  if (width < 64 || width > 4096 || height < 64 || height > 2048 || dpi < 72 || dpi > 768) {
    error = "preview dimensions or DPI are outside the supported range";
    return {};
  }
  CanvasBitmap bitmap(hidden_window_, static_cast<int>(width), static_cast<int>(height));
  if (!bitmap.valid()) {
    error = "CreateDIBSection failed";
    return {};
  }
  const RECT area{0, 0, static_cast<LONG>(width), static_cast<LONG>(height)};
  draw_sample(bitmap.dc, area, sample, dpi, foreground, background);
  auto* pixels = static_cast<std::uint8_t*>(bitmap.bits);
  for (std::size_t index = 3; index < static_cast<std::size_t>(width) * height * 4U; index += 4) {
    pixels[index] = 0xFF;
  }
  return encode_png(width, height, width * 4U, pixels, error);
}

mtpc::Frame PreviewRuntime::render(const mtpc::Frame& request) {
  const auto started = std::chrono::steady_clock::now();
  std::string error;
  const auto parsed = JsonDocument::parse(request.json, error);
  if (!parsed) return error_frame(request.request_id, "invalid_request", error);
  if (!apply_request(*parsed, error)) return error_frame(request.request_id, "invalid_request", error);
  std::uint32_t width{};
  std::uint32_t height{};
  std::uint32_t dpi{};
  auto png = render_png(*parsed, width, height, dpi, error);
  if (png.empty()) return error_frame(request.request_id, "render_failed", error);
  const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
      std::chrono::steady_clock::now() - started);
  mtpc::Frame response;
  response.kind = mtpc::MessageKind::preview_rendered;
  response.request_id = request.request_id;
  response.binary = std::move(png);
  response.json = std::string{"{\"width\":"} + std::to_string(width) + ",\"height\":" +
                  std::to_string(height) + ",\"dpi\":" + std::to_string(dpi) +
                  ",\"elapsedMs\":" + std::to_string(elapsed.count()) +
                  ",\"coreVersion\":" + std::to_string(core_version_) + ",\"engine\":\"" +
                  (engine_ == Engine::mactype ? "mactype" : "plain") + "\"}";
  return response;
}

bool PreviewRuntime::apply_native_request(const std::string& json, std::string& error) {
  const auto parsed = JsonDocument::parse(json, error);
  if (!parsed) return false;
  const JsonDocument& document = *parsed;
  const auto chrome_object = document.object("chrome");
  const auto labels_object = document.object("labels");
  struct PendingNativeState {
    DisplayMode display_mode;
    SampleState sample;
    std::wstring listing_text;
    bool topmost;
    bool dark_theme;
    int zoom;
    std::vector<float> ladder_sizes;
    COLORREF native_foreground;
    COLORREF native_background;
    bool inverted;
    NativePreviewLabels labels;
    std::optional<NativeChrome> chrome;
  } pending{display_mode_, native_sample_, listing_text_, topmost_, dark_theme_, zoom_,
            ladder_sizes_, native_foreground_, native_background_, inverted_, labels_,
            std::nullopt};

  if (const auto mode = document.root_string("displayMode")) {
    if (*mode == "sample" || *mode == "default") pending.display_mode = DisplayMode::sample;
    else if (*mode == "ladder") pending.display_mode = DisplayMode::ladder;
    else if (*mode == "compare") pending.display_mode = DisplayMode::compare;
    else if (*mode == "listing") pending.display_mode = DisplayMode::listing;
    else {
      error = "displayMode is unsupported";
      return false;
    }
  }
  if (const auto value = document.root_string("text")) {
    pending.sample.text = utf8_to_wide(*value);
    if (pending.sample.text.size() > 4096) {
      error = "text exceeds 4096 UTF-16 units";
      return false;
    }
  }
  if (const auto value = document.root_string("listingText")) {
    pending.listing_text = utf8_to_wide(*value);
    if (pending.listing_text.size() > 4096) {
      error = "listingText exceeds 4096 UTF-16 units";
      return false;
    }
  }
  if (const auto value = document.root_string("fontFace")) pending.sample.font_face = utf8_to_wide(*value);
  const auto font_size_start = document.contains_root("fontSizePt");
  const auto font_size = document.root_number("fontSizePt");
  if (font_size_start && !font_size) {
    error = "fontSizePt must be a number";
    return false;
  }
  if (font_size) {
    if (*font_size < 4.0 || *font_size > 96.0) {
      error = "fontSizePt is outside the supported range";
      return false;
    }
    pending.sample.font_size_pt = static_cast<float>(*font_size);
  }
  const auto apply_boolean = [&](const char* key, bool& destination) {
    const auto start = document.contains_root(key);
    const auto value = document.root_bool(key);
    if (start && !value) {
      error = std::string{key} + " must be a boolean";
      return false;
    }
    if (value) destination = *value;
    return true;
  };
  if (!apply_boolean("bold", pending.sample.bold) ||
      !apply_boolean("italic", pending.sample.italic) ||
      !apply_boolean("topmost", pending.topmost)) {
    return false;
  }
  if (const auto value = document.root_string("theme")) {
    if (*value != "light" && *value != "dark") {
      error = "theme is unsupported";
      return false;
    }
    pending.dark_theme = *value == "dark";
  }
  const auto zoom_start = document.contains_root("zoom");
  const auto zoom = document.root_number("zoom");
  if (zoom_start && !zoom) {
    error = "zoom must be an integer";
    return false;
  }
  if (zoom) {
    const int requested = static_cast<int>(*zoom);
    if (*zoom != static_cast<double>(requested) ||
        (requested != 1 && requested != 2 && requested != 4)) {
      error = "zoom must be 1, 2, or 4";
      return false;
    }
    pending.zoom = requested;
  }
  const auto sizes_start = document.contains_root("sizes");
  const auto sizes_value = document.root_number_array("sizes");
  if (sizes_start && !sizes_value) {
    error = "sizes must be an array of integers";
    return false;
  }
  if (sizes_value) {
    if (sizes_value->empty() || sizes_value->size() > 16) {
      error = "sizes must contain between 1 and 16 values";
      return false;
    }
    std::vector<float> sizes;
    for (double size : *sizes_value) {
      const int integer_size = static_cast<int>(size);
      if (size != static_cast<double>(integer_size) || integer_size < 4 || integer_size > 96) {
        error = "sizes contains an unsupported value";
        return false;
      }
      sizes.push_back(static_cast<float>(integer_size));
    }
    pending.ladder_sizes = std::move(sizes);
  }

  if (chrome_object) {
    const auto skin = chrome_object->json_string("skin");
    if (!skin) {
      error = "chrome.skin is required";
      return false;
    }
    Skin parsed_skin{};
    if (*skin == "classic") parsed_skin = Skin::classic;
    else {
      error = "chrome.skin is unsupported";
      return false;
    }
    NativeChrome chrome = chrome_from(kNativeChromeDefault);
    chrome.skin = parsed_skin;
    struct ColorBinding {
      const char* key;
      COLORREF Palette::*member;
    };
    constexpr ColorBinding color_bindings[] = {
        {"canvas", &Palette::canvas}, {"surface", &Palette::surface},
        {"surfaceSubtle", &Palette::hover}, {"border", &Palette::border},
        {"text", &Palette::text}, {"muted", &Palette::muted},
        {"accent", &Palette::accent}, {"onAccent", &Palette::on_accent},
    };
    for (const auto& binding : color_bindings) {
      const auto value = chrome_object->json_string(binding.key);
      if (!value) {
        error = std::string{"chrome."} + binding.key + " is required";
        return false;
      }
      const auto color = parse_color(*value);
      if (!color) {
        error = std::string{"chrome."} + binding.key + " must be a #RRGGBB colour";
        return false;
      }
      chrome.palette.*(binding.member) = *color;
    }
    struct MetricBinding {
      const char* key;
      int NativeChrome::*member;
      int minimum;
      int maximum;
    };
    constexpr MetricBinding metric_bindings[] = {
        {"radius", &NativeChrome::radius, 0, 32},
        {"controlHeight", &NativeChrome::control_height, 20, 64},
        {"toolbarHeight", &NativeChrome::toolbar_height, 28, 96},
        {"statusHeight", &NativeChrome::status_height, 18, 64},
        {"canvasRadius", &NativeChrome::canvas_radius, 0, 32},
        {"canvasInset", &NativeChrome::canvas_inset, 0, 64},
    };
    for (const auto& binding : metric_bindings) {
      const auto value = chrome_object->json_number(binding.key);
      if (!value || *value != static_cast<double>(static_cast<int>(*value)) ||
          *value < binding.minimum || *value > binding.maximum) {
        error = std::string{"chrome."} + binding.key + " is outside the supported range";
        return false;
      }
      chrome.*(binding.member) = static_cast<int>(*value);
    }
    const auto mono = chrome_object->json_bool("monoStatus");
    if (!mono) {
      error = "chrome.monoStatus must be a boolean";
      return false;
    }
    chrome.mono_status = *mono;
    pending.chrome = chrome;
  } else {
    pending.chrome.reset();
  }

  const auto foreground = document.root_string("foreground");
  const auto background = document.root_string("background");
  const bool colors_supplied = foreground.has_value() || background.has_value();
  const auto inverted_start = document.contains_root("inverted");
  const auto inverted = document.root_bool("inverted");
  if (inverted_start && !inverted) {
    error = "inverted must be a boolean";
    return false;
  }
  const bool desired_inverted = inverted.value_or(pending.inverted);
  if (colors_supplied && pending.inverted) {
    std::swap(pending.native_foreground, pending.native_background);
    pending.inverted = false;
  }
  if (foreground) {
    pending.native_foreground = parse_color(*foreground, pending.native_foreground);
  }
  if (background) {
    pending.native_background = parse_color(*background, pending.native_background);
  }
  if (desired_inverted != pending.inverted) {
    std::swap(pending.native_foreground, pending.native_background);
    pending.inverted = desired_inverted;
  }

  for (const auto& binding : kNativeLabelBindings) {
    const auto value = labels_object ? labels_object->json_string(binding.key) : std::nullopt;
    if (!value) continue;
    if (value->size() > 256) {
      error = std::string{"label is too long: "} + binding.key;
      return false;
    }
    pending.labels.*(binding.member) = utf8_to_wide(*value);
  }
  const bool theme_changed = pending.dark_theme != dark_theme_;
  const bool chrome_changed = pending.chrome != chrome_;
  const bool labels_changed = pending.labels != labels_;
  const bool title_changed = pending.labels.title != labels_.title;
  const bool controls_changed = pending.sample != native_sample_;
  const bool layout_changed = theme_changed || chrome_changed || labels_changed ||
                              pending.zoom != zoom_;
  display_mode_ = pending.display_mode;
  native_sample_ = std::move(pending.sample);
  listing_text_ = std::move(pending.listing_text);
  topmost_ = pending.topmost;
  dark_theme_ = pending.dark_theme;
  zoom_ = pending.zoom;
  ladder_sizes_ = std::move(pending.ladder_sizes);
  native_foreground_ = pending.native_foreground;
  native_background_ = pending.native_background;
  inverted_ = pending.inverted;
  labels_ = std::move(pending.labels);
  chrome_ = std::move(pending.chrome);

  if (theme_changed || chrome_changed) {
    recreate_palette_brushes();
    apply_combo_theme();
  }
  if (title_changed) {
    SetWindowTextW(native_window_, labels_.title.c_str());
    ++retitle_count_;
  }
  if (controls_changed) sync_controls();
  if (layout_changed) relayout_controls();
  else InvalidateRect(native_window_, nullptr, FALSE);
  return true;
}

mtpc::Frame PreviewRuntime::show_native_preview(const mtpc::Frame& request, bool visible) {
  if (visible) {
    std::string error;
    if (!apply_native_request(request.json, error)) {
      return error_frame(request.request_id, "invalid_native_preview", error);
    }
    show_native_window();
  } else {
    hide_native_window();
  }
  mtpc::Frame response;
  response.kind = mtpc::MessageKind::native_preview_state;
  response.request_id = request.request_id;
  response.json = native_state_json(visible);
  return response;
}

std::string PreviewRuntime::native_state_json(bool visible) const {
  const char* mode = "sample";
  if (display_mode_ == DisplayMode::ladder) mode = "ladder";
  else if (display_mode_ == DisplayMode::compare) mode = "compare";
  else if (display_mode_ == DisplayMode::listing) mode = "listing";
  std::ostringstream size;
  size << native_sample_.font_size_pt;
  const std::string face = json_escape_string(wide_to_utf8(native_sample_.font_face));
  return std::string{"{\"visible\":"} + (visible ? "true" : "false") +
         ",\"displayMode\":\"" + mode + "\",\"background\":\"" +
         color_to_hex(native_background_) + "\",\"foreground\":\"" +
         color_to_hex(native_foreground_) + "\",\"inverted\":" +
         (inverted_ ? "true" : "false") + ",\"zoom\":" + std::to_string(zoom_) +
         ",\"fontFace\":\"" + face + "\",\"fontSizePt\":" + size.str() +
         ",\"bold\":" + (native_sample_.bold ? "true" : "false") + ",\"italic\":" +
         (native_sample_.italic ? "true" : "false") + ",\"topmost\":" +
         (topmost_ ? "true" : "false") +
         (chrome_ ? std::string{",\"skin\":\""} +
                        "classic" + "\"}"
                  : "}");
}

void PreviewRuntime::show_native_window() {
  if (IsWindowVisible(native_window_) && !IsIconic(native_window_)) {
    /* An open window takes new options in place: the z-order follows the
       topmost flag and the canvas repaints, without activating the window
       again and pulling focus away from the editor that sent the change. */
    SetWindowPos(native_window_, topmost_ ? HWND_TOPMOST : HWND_NOTOPMOST, 0, 0, 0, 0,
                 SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    InvalidateRect(native_window_, nullptr, FALSE);
    return;
  }
  if (has_placement_) {
    SetWindowPlacement(native_window_, &placement_);
  } else {
    /* First show: widen the window until every toolbar label fits, within
       the monitor's work area; a reader who later resizes it keeps that. */
    RECT client{};
    RECT window_rect{};
    if (GetClientRect(native_window_, &client) && GetWindowRect(native_window_, &window_rect) &&
        client.right < full_labels_client_width_) {
      MONITORINFO monitor{sizeof(MONITORINFO)};
      GetMonitorInfoW(MonitorFromWindow(native_window_, MONITOR_DEFAULTTONEAREST), &monitor);
      const int frame = (window_rect.right - window_rect.left) - client.right;
      const int wanted = full_labels_client_width_ + frame;
      const int limit = monitor.rcWork.right - monitor.rcWork.left;
      SetWindowPos(native_window_, nullptr, 0, 0, std::min(wanted, limit),
                   window_rect.bottom - window_rect.top,
                   SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
  }
  ++reshow_count_;
  SetWindowPos(native_window_, topmost_ ? HWND_TOPMOST : HWND_NOTOPMOST, 0, 0, 0, 0,
               SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
  ShowWindow(native_window_, SW_SHOWNORMAL);
  InvalidateRect(native_window_, nullptr, FALSE);
  UpdateWindow(native_window_);
}

void PreviewRuntime::hide_native_window(bool notify) {
  placement_.length = sizeof(placement_);
  has_placement_ = GetWindowPlacement(native_window_, &placement_) != FALSE;
  ShowWindow(native_window_, SW_HIDE);
  if (notify) emit_native_state(false);
}

void PreviewRuntime::set_state_sink(std::function<void(const mtpc::Frame&)> sink) {
  state_sink_ = std::move(sink);
}

void PreviewRuntime::emit_native_state(bool visible) {
  if (!state_sink_) return;
  mtpc::Frame event;
  event.kind = mtpc::MessageKind::native_preview_state;
  event.request_id = 0;
  event.json = native_state_json(visible);
  state_sink_(event);
}

void PreviewRuntime::close_from_window_for_tests() {
  SendMessageW(native_window_, WM_CLOSE, 0, 0);
}

bool PreviewRuntime::save_in_progress_for_tests() const { return save_in_progress_; }

int PreviewRuntime::scroll_max_for_tests() {
  RedrawWindow(native_window_, nullptr, nullptr, RDW_INVALIDATE | RDW_UPDATENOW);
  return scroll_max_;
}

int PreviewRuntime::wheel_for_tests(int delta) {
  SendMessageW(native_window_, WM_MOUSEWHEEL,
               MAKEWPARAM(0, static_cast<WORD>(static_cast<short>(delta))), 0);
  return scroll_y_;
}

void PreviewRuntime::set_save_in_progress_for_tests(bool in_progress) {
  save_in_progress_ = in_progress;
}

void PreviewRuntime::trigger_save_for_tests() { execute_toolbar_action(kSavePng); }

std::uint32_t PreviewRuntime::save_thread_started_for_tests() const {
  return save_thread_started_;
}

std::uint32_t PreviewRuntime::relayout_count_for_tests() const { return relayout_count_; }

std::uint32_t PreviewRuntime::retitle_count_for_tests() const { return retitle_count_; }

std::uint32_t PreviewRuntime::reshow_count_for_tests() const { return reshow_count_; }

PreviewRuntime::ToolbarSnapshot PreviewRuntime::toolbar_snapshot_for_tests() const {
  RECT client{};
  GetClientRect(native_window_, &client);
  RECT edit{};
  GetWindowRect(edit_control_, &edit);
  MapWindowPoints(nullptr, native_window_, reinterpret_cast<POINT*>(&edit), 2);
  HDC dc = GetDC(native_window_);
  HGDIOBJ previous = dc ? SelectObject(dc, ui_font_) : nullptr;
  const auto measure = [&](const std::wstring& text) {
    SIZE extent{};
    if (dc) {
      GetTextExtentPoint32W(dc, text.c_str(), static_cast<int>(text.size()), &extent);
    }
    return extent;
  };
  ToolbarSnapshot snapshot{client.right, client.bottom, toolbar_layout_height_, {},
                           {face_label_rect_, labels_.font_face, measure(labels_.font_face)},
                           {size_label_rect_, labels_.font_size, measure(labels_.font_size)}, edit};
  const std::size_t count = std::min(toolbar_buttons_.size(), toolbar_button_texts_.size());
  snapshot.buttons.reserve(count);
  for (std::size_t index = 0; index < count; ++index) {
    const RECT rectangle = toolbar_buttons_[index].second;
    const POINT center{(rectangle.left + rectangle.right) / 2,
                       (rectangle.top + rectangle.bottom) / 2};
    snapshot.buttons.push_back({toolbar_buttons_[index].first, hit_test_toolbar(center), rectangle,
                                toolbar_button_texts_[index],
                                measure(toolbar_button_texts_[index])});
  }
  if (dc) {
    SelectObject(dc, previous);
    ReleaseDC(native_window_, dc);
  }
  return snapshot;
}

bool PreviewRuntime::set_dpi_for_tests(std::uint32_t dpi) {
  if (dpi < 72 || dpi > 768) return false;
  apply_dpi(dpi);
  return true;
}

std::optional<PreviewRuntime::ToolbarCapture> PreviewRuntime::capture_toolbar_for_tests() {
  RECT client{};
  if (!GetClientRect(native_window_, &client)) return std::nullopt;
  const int width = client.right;
  const int height = toolbar_layout_height_;
  constexpr int kMaximumDimension = 8192;
  constexpr std::size_t kMaximumPixels = 16U * 1024U * 1024U;
  if (width <= 0 || height <= 0 || width > kMaximumDimension ||
      height > kMaximumDimension ||
      static_cast<std::size_t>(width) * static_cast<std::size_t>(height) > kMaximumPixels) {
    return std::nullopt;
  }
  BITMAPINFO info{};
  info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
  info.bmiHeader.biWidth = width;
  info.bmiHeader.biHeight = -height;
  info.bmiHeader.biPlanes = 1;
  info.bmiHeader.biBitCount = 32;
  info.bmiHeader.biCompression = BI_RGB;
  void* pixels = nullptr;
  HDC dc = CreateCompatibleDC(nullptr);
  if (!dc) return std::nullopt;
  HBITMAP bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &pixels, nullptr, 0);
  if (!bitmap || !pixels) {
    if (bitmap) DeleteObject(bitmap);
    DeleteDC(dc);
    return std::nullopt;
  }
  HGDIOBJ previous = SelectObject(dc, bitmap);
  draw_toolbar(dc, RECT{0, 0, width, height});
  GdiFlush();
  ToolbarCapture capture{width, height, std::vector<std::uint32_t>(
                                            static_cast<std::size_t>(width) * height)};
  std::memcpy(capture.pixels.data(), pixels, capture.pixels.size() * sizeof(std::uint32_t));
  SelectObject(dc, previous);
  DeleteObject(bitmap);
  DeleteDC(dc);
  return capture;
}

const PreviewRuntime::Palette& PreviewRuntime::palette() const {
  return chrome_ ? chrome_->palette : (dark_theme_ ? kDarkPalette : kLightPalette);
}

int PreviewRuntime::chrome_metric(int NativeChrome::*member, int fallback) const {
  return scaled(chrome_ ? (*chrome_).*member : fallback, native_dpi_);
}

PreviewRuntime::CanvasCacheKey PreviewRuntime::native_canvas_key(
    int width, int minimum_height) const {
  return CanvasCacheKey{width, minimum_height, native_sample_, listing_text_, native_foreground_,
                        native_background_, native_dpi_, display_mode_, zoom_, inverted_,
                        ladder_sizes_, labels_, chrome_, dark_theme_};
}

PreviewRuntime::CanvasBitmap* PreviewRuntime::cached_native_canvas(const CanvasCacheKey& key) {
  if (!canvas_cache_ || !canvas_cache_key_ || *canvas_cache_key_ != key) {
    canvas_cache_ = render_native_canvas(key.width, key.minimum_height, key.sample,
                                         key.foreground, key.background, key.dpi,
                                         key.display_mode, key.zoom, key.inverted,
                                         key.ladder_sizes, key.listing_text, key.labels,
                                         key.chrome, key.dark_theme);
    canvas_cache_key_ = key;
  }
  return canvas_cache_.get();
}

void PreviewRuntime::apply_combo_theme() {
  const bool dark = chrome_ ? is_dark(chrome_->palette.canvas) : dark_theme_;
  for (HWND combo : {face_combo_, size_combo_}) {
    if (combo) SetWindowTheme(combo, dark ? L"DarkMode_CFD" : nullptr, nullptr);
  }
}

void PreviewRuntime::apply_dpi(std::uint32_t dpi) {
  native_dpi_ = dpi;
  recreate_ui_font();
  relayout_controls();
}

void PreviewRuntime::recreate_ui_font() {
  if (ui_font_) DeleteObject(ui_font_);
  if (mono_font_) DeleteObject(mono_font_);
  ui_font_ = CreateFontW(-MulDiv(9, static_cast<int>(native_dpi_), 72), 0, 0, 0, FW_NORMAL, FALSE,
                         FALSE, FALSE, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                         CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE, L"Segoe UI");
  const auto cascadia = std::find_if(font_names_.begin(), font_names_.end(), [](const auto& name) {
    return _wcsicmp(name.c_str(), L"Cascadia Mono") == 0;
  });
  mono_font_ = CreateFontW(-MulDiv(10, static_cast<int>(native_dpi_), 72), 0, 0, 0, FW_NORMAL, FALSE,
                           FALSE, FALSE, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                           CLEARTYPE_QUALITY, FIXED_PITCH | FF_MODERN,
                           cascadia != font_names_.end() ? L"Cascadia Mono" : L"Consolas");
  const auto update_combo = [&](HWND combo) {
    if (!combo) return;
    SendMessageW(combo, WM_SETFONT, reinterpret_cast<WPARAM>(ui_font_), TRUE);
    const LPARAM item_height = static_cast<LPARAM>(scaled(24, native_dpi_));
    SendMessageW(combo, CB_SETITEMHEIGHT, 0, item_height);
    SendMessageW(combo, CB_SETITEMHEIGHT, static_cast<WPARAM>(-1), item_height);
  };
  update_combo(face_combo_);
  update_combo(size_combo_);
  if (edit_control_) SendMessageW(edit_control_, WM_SETFONT, reinterpret_cast<WPARAM>(ui_font_), TRUE);
}

void PreviewRuntime::recreate_palette_brushes() {
  if (surface_brush_) DeleteObject(surface_brush_);
  if (edit_brush_) DeleteObject(edit_brush_);
  const Palette& colors = palette();
  surface_brush_ = CreateSolidBrush(colors.surface);
  edit_brush_ = CreateSolidBrush(colors.surface);
  if (native_window_) InvalidateRect(native_window_, nullptr, TRUE);
}

int CALLBACK enumerate_font_proc(const LOGFONTW* font, const TEXTMETRICW*, DWORD, LPARAM data) {
  if (font->lfFaceName[0] != L'@') {
    static_cast<std::set<std::wstring>*>(reinterpret_cast<void*>(data))->insert(font->lfFaceName);
  }
  return 1;
}

void PreviewRuntime::enumerate_fonts() {
  HDC dc = GetDC(native_window_);
  LOGFONTW query{};
  query.lfCharSet = DEFAULT_CHARSET;
  std::set<std::wstring> names;
  EnumFontFamiliesExW(dc, &query, enumerate_font_proc, reinterpret_cast<LPARAM>(&names), 0);
  ReleaseDC(native_window_, dc);
  font_names_.assign(names.begin(), names.end());
  for (const auto& name : font_names_) {
    SendMessageW(face_combo_, CB_ADDSTRING, 0, reinterpret_cast<LPARAM>(name.c_str()));
  }
}

void PreviewRuntime::sync_controls() {
  if (!face_combo_) return;
  LRESULT face_index = SendMessageW(face_combo_, CB_FINDSTRINGEXACT, static_cast<WPARAM>(-1),
                                    reinterpret_cast<LPARAM>(native_sample_.font_face.c_str()));
  if (face_index == CB_ERR) {
    face_index = SendMessageW(face_combo_, CB_INSERTSTRING, 0,
                              reinterpret_cast<LPARAM>(native_sample_.font_face.c_str()));
  }
  if (face_index != CB_ERR && face_index != CB_ERRSPACE &&
      SendMessageW(face_combo_, CB_GETCURSEL, 0, 0) != face_index) {
    SendMessageW(face_combo_, CB_SETCURSEL, static_cast<WPARAM>(face_index), 0);
  }
  const std::wstring size = std::to_wstring(static_cast<int>(std::lround(native_sample_.font_size_pt)));
  const LRESULT size_index = SendMessageW(size_combo_, CB_FINDSTRINGEXACT, static_cast<WPARAM>(-1),
                                          reinterpret_cast<LPARAM>(size.c_str()));
  if (size_index != CB_ERR && SendMessageW(size_combo_, CB_GETCURSEL, 0, 0) != size_index) {
    SendMessageW(size_combo_, CB_SETCURSEL, static_cast<WPARAM>(size_index), 0);
  }
  /* The edit control keeps its caret unless the sample really changed; it
     stores line breaks as CR LF, so the comparison ignores the CR. */
  const int length = GetWindowTextLengthW(edit_control_);
  std::wstring current(static_cast<std::size_t>(length) + 1, L'\0');
  GetWindowTextW(edit_control_, current.data(), length + 1);
  current.resize(static_cast<std::size_t>(length));
  std::erase(current, L'\r');
  std::wstring wanted = native_sample_.text;
  std::erase(wanted, L'\r');
  if (current != wanted) {
    updating_edit_ = true;
    SetWindowTextW(edit_control_, native_sample_.text.c_str());
    updating_edit_ = false;
  }
  if ((IsWindowVisible(edit_control_) != FALSE) != edit_visible_) {
    ShowWindow(edit_control_, edit_visible_ ? SW_SHOW : SW_HIDE);
  }
}

std::wstring PreviewRuntime::selected_face_for_tests() const {
  if (!face_combo_) return {};
  const LRESULT selected = SendMessageW(face_combo_, CB_GETCURSEL, 0, 0);
  if (selected == CB_ERR) return {};
  const LRESULT length = SendMessageW(face_combo_, CB_GETLBTEXTLEN, selected, 0);
  if (length == CB_ERR) return {};
  std::wstring value(static_cast<std::size_t>(length) + 1, L'\0');
  SendMessageW(face_combo_, CB_GETLBTEXT, selected, reinterpret_cast<LPARAM>(value.data()));
  value.resize(static_cast<std::size_t>(length));
  return value;
}

void PreviewRuntime::relayout_controls() {
  if (!native_window_ || !face_combo_) return;
  ++relayout_count_;
  RECT client{};
  GetClientRect(native_window_, &client);
  const int toolbar_height = chrome_metric(&NativeChrome::toolbar_height, 40);
  const int control_height = chrome_metric(&NativeChrome::control_height, 28);
  const int top = std::max(0, (toolbar_height - control_height) / 2);
  HDC dc = GetDC(native_window_);
  HGDIOBJ previous = SelectObject(dc, ui_font_);
  auto label_width = [&](const std::wstring& text) {
    SIZE extent{};
    GetTextExtentPoint32W(dc, text.c_str(), static_cast<int>(text.size()), &extent);
    return static_cast<int>(extent.cx) + scaled(8, native_dpi_);
  };
  const int face_label_width = label_width(labels_.font_face);
  const int size_label_width = label_width(labels_.font_size);
  SelectObject(dc, previous);
  ReleaseDC(native_window_, dc);
  const int face_width = std::clamp(static_cast<int>(client.right) / 7, scaled(110, native_dpi_),
                                    scaled(160, native_dpi_));
  const int size_width = scaled(58, native_dpi_);
  int x = scaled(8, native_dpi_);
  face_label_rect_ = {x, top, x + face_label_width, top + control_height};
  x += face_label_width;
  MoveWindow(face_combo_, x, top, face_width, scaled(300, native_dpi_), TRUE);
  x += face_width + scaled(6, native_dpi_);
  size_label_rect_ = {x, top, x + size_label_width, top + control_height};
  x += size_label_width;
  MoveWindow(size_combo_, x, top, size_width, scaled(250, native_dpi_), TRUE);
  rebuild_toolbar_layout();
  InvalidateRect(native_window_, nullptr, FALSE);
}

void PreviewRuntime::rebuild_toolbar_layout() {
  toolbar_buttons_.clear();
  toolbar_button_texts_.clear();
  toolbar_separators_.clear();
  RECT client{};
  GetClientRect(native_window_, &client);
  const int left = size_label_rect_.right + scaled(64, native_dpi_);
  const int right = static_cast<int>(client.right) - scaled(8, native_dpi_);
  const int toolbar_height = chrome_metric(&NativeChrome::toolbar_height, 40);
  const int height = chrome_metric(&NativeChrome::control_height, 28);
  int top = std::max(0, (toolbar_height - height) / 2);
  constexpr std::array<int, 13> actions{kBold, kItalic, kModeSample, kModeLadder, kModeCompare,
                                         kModeListing, kInvert, kLoupe, kZoom, kTopmost,
                                         kEditText, kSavePng, kCopy};
  auto text_for = [&](int action) {
    switch (action) {
      case kBold: return labels_.bold;
      case kItalic: return labels_.italic;
      case kModeSample: return labels_.mode_sample;
      case kModeLadder: return labels_.mode_ladder;
      case kModeCompare: return labels_.mode_compare;
      case kModeListing: return labels_.mode_listing;
      case kInvert: return labels_.invert;
      case kLoupe: return labels_.loupe;
      case kZoom: return labels_.zoom + L" " + std::to_wstring(zoom_) + L"x";
      case kTopmost: return labels_.topmost;
      case kEditText: return labels_.edit_text;
      case kSavePng: return labels_.save_png;
      case kCopy: return labels_.copy;
      default: return std::wstring{};
    }
  };
  for (int action : actions) toolbar_button_texts_.push_back(text_for(action));
  const int gap = scaled(2, native_dpi_);
  HDC dc = GetDC(native_window_);
  HGDIOBJ previous = SelectObject(dc, ui_font_);
  const auto widths = [&] {
    std::vector<int> result;
    for (const auto& text : toolbar_button_texts_) {
      SIZE extent{};
      GetTextExtentPoint32W(dc, text.c_str(), static_cast<int>(text.size()), &extent);
      result.push_back(std::max(scaled(40, native_dpi_), static_cast<int>(extent.cx) + scaled(20, native_dpi_)));
    }
    return result;
  }();
  auto total_width = [&] {
    int total = gap * static_cast<int>(actions.size() - 1);
    for (int width : widths) total += width;
    return total;
  };
  full_labels_client_width_ = left + total_width() + scaled(16, native_dpi_);
  SelectObject(dc, previous);
  ReleaseDC(native_window_, dc);
  int x = left;
  int row = 0;
  const int row_left = scaled(8, native_dpi_);
  for (std::size_t index = 0; index < actions.size(); ++index) {
    if (x + widths[index] > right && x > row_left) {
      x = row_left;
      top += toolbar_height;
      ++row;
    }
    toolbar_buttons_.push_back({actions[index], RECT{x, top, x + widths[index], top + height}});
    x += widths[index] + gap;
    if ((index == 1 || index == 5 || index == 8) && index + 1 < actions.size() &&
        x + widths[index + 1] <= right) {
      toolbar_separators_.push_back(RECT{x - gap / 2, top + scaled(5, native_dpi_),
                                       x - gap / 2, top + height - scaled(5, native_dpi_)});
    }
  }
  toolbar_layout_height_ = (row + 1) * toolbar_height;
  minimum_client_width_ = std::max(scaled(720, native_dpi_),
      std::max(left, *std::max_element(widths.begin(), widths.end()) + row_left) +
          scaled(8, native_dpi_));
  MoveWindow(edit_control_, row_left, toolbar_layout_height_ + scaled(4, native_dpi_),
             std::max(1, static_cast<int>(client.right) - scaled(16, native_dpi_)),
             scaled(64, native_dpi_), TRUE);
}

void PreviewRuntime::draw_toolbar(HDC dc, const RECT& area) {
  const Palette& palette = this->palette();
  fill_solid(dc, area, palette.surface);
  HGDIOBJ previous_font = SelectObject(dc, ui_font_);
  SetBkMode(dc, TRANSPARENT);
  SetTextColor(dc, palette.muted);
  DrawTextW(dc, labels_.font_face.c_str(), -1, &face_label_rect_, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
  DrawTextW(dc, labels_.font_size.c_str(), -1, &size_label_rect_, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
  for (const RECT& separator : toolbar_separators_) {
    HPEN pen = CreatePen(PS_SOLID, 1, palette.border);
    HGDIOBJ previous_pen = SelectObject(dc, pen);
    MoveToEx(dc, separator.left, separator.top, nullptr);
    LineTo(dc, separator.right, separator.bottom);
    SelectObject(dc, previous_pen);
    DeleteObject(pen);
  }
  std::size_t button_index = 0;
  for (const auto& [action, rectangle] : toolbar_buttons_) {
    bool active = false;
    switch (action) {
      case kBold: active = native_sample_.bold; break;
      case kItalic: active = native_sample_.italic; break;
      case kModeSample: active = display_mode_ == DisplayMode::sample; break;
      case kModeLadder: active = display_mode_ == DisplayMode::ladder; break;
      case kModeCompare: active = display_mode_ == DisplayMode::compare; break;
      case kModeListing: active = display_mode_ == DisplayMode::listing; break;
      case kInvert: active = inverted_; break;
      case kLoupe: active = loupe_; break;
      case kTopmost: active = topmost_; break;
      case kEditText: active = edit_visible_; break;
      default: break;
    }
    COLORREF fill = active || pressed_action_ == action
                        ? palette.accent
                        : (hover_action_ == action ? palette.hover : palette.surface);
    HBRUSH brush = CreateSolidBrush(fill);
    COLORREF outline = active ? palette.accent : palette.border;
    HPEN pen = CreatePen(PS_SOLID, 1, outline);
    HGDIOBJ previous_brush = SelectObject(dc, brush);
    HGDIOBJ previous_pen = SelectObject(dc, pen);
    const int radius = chrome_metric(&NativeChrome::radius, 4);
    RoundRect(dc, rectangle.left, rectangle.top, rectangle.right, rectangle.bottom, radius, radius);
    SelectObject(dc, previous_brush);
    SelectObject(dc, previous_pen);
    DeleteObject(brush);
    DeleteObject(pen);
    const std::wstring& text = toolbar_button_texts_[button_index++];
    RECT text_rect = rectangle;
    SetTextColor(dc, (active || pressed_action_ == action)
                         ? palette.on_accent
                         : palette.text);
    DrawTextW(dc, text.c_str(), -1, &text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
  }
  SelectObject(dc, previous_font);
}

void PreviewRuntime::draw_combo_item(const DRAWITEMSTRUCT& item) {
  if (item.itemID == static_cast<UINT>(-1)) return;
  const Palette& palette = this->palette();
  const bool selected = (item.itemState & ODS_SELECTED) != 0;
  fill_solid(item.hDC, item.rcItem, selected ? palette.accent : palette.surface);
  wchar_t text[LF_FACESIZE]{};
  SendMessageW(item.hwndItem, CB_GETLBTEXT, item.itemID, reinterpret_cast<LPARAM>(text));
  RECT text_rect = item.rcItem;
  text_rect.left += scaled(6, native_dpi_);
  SetBkMode(item.hDC, TRANSPARENT);
  SetTextColor(item.hDC, selected ? palette.on_accent : palette.text);
  HGDIOBJ previous = SelectObject(item.hDC, ui_font_);
  DrawTextW(item.hDC, text, -1, &text_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
  SelectObject(item.hDC, previous);
}

std::unique_ptr<PreviewRuntime::CanvasBitmap> PreviewRuntime::render_native_canvas(
    int width, int minimum_height, const SampleState& sample, COLORREF foreground,
    COLORREF background, std::uint32_t dpi, DisplayMode mode, int zoom, bool inverted,
    const std::vector<float>& ladder_sizes, const std::wstring& listing_text,
    const NativePreviewLabels& labels, const std::optional<NativeChrome>& chrome,
    bool dark_theme) {
  (void)zoom;
  (void)inverted;
  const Palette& canvas_palette = chrome ? chrome->palette : (dark_theme ? kDarkPalette : kLightPalette);
  const int content_width = std::max(1, width);
  int height = minimum_height;
  if (mode == DisplayMode::listing) height = std::max(height, listing_content_height(dpi));
  if (mode == DisplayMode::ladder) {
    int ladder_height = scaled(20, dpi);
    for (float size : ladder_sizes) ladder_height += std::max(scaled(24, dpi), point_size_px(size, dpi) * 3 / 2);
    height = std::max(height, ladder_height);
  }
  const int sample_line_height =
      std::max(scaled(20, dpi), point_size_px(sample.font_size_pt, dpi) * 3 / 2);
  const int sample_margin = scaled(18, dpi);
  const int compare_gap = scaled(1, dpi);
  const int compare_header = scaled(30, dpi);
  const int compare_half = (content_width - compare_gap) / 2;
  if (mode == DisplayMode::sample || mode == DisplayMode::compare) {
    /* The canvas grows to hold every wrapped line, so a zoomed view scrolls
       through the sample instead of clipping it at the window's height. */
    HFONT measure_font = create_sample_font(sample.font_face, sample.font_size_pt, dpi, sample.bold,
                                            sample.italic);
    const int column_width = mode == DisplayMode::sample
                                 ? content_width - 2 * sample_margin
                                 : compare_half - 2 * scaled(10, dpi);
    const int text_height =
        measure_wrapped_text(measure_font, column_width, sample.text, sample_line_height);
    DeleteObject(measure_font);
    height = std::max(height, mode == DisplayMode::sample
                                  ? text_height + 2 * sample_margin
                                  : compare_header + text_height + scaled(10, dpi));
  }
  auto bitmap = std::make_unique<CanvasBitmap>(native_window_, content_width, height);
  if (!bitmap->valid()) return nullptr;
  const RECT area{0, 0, content_width, height};
  fill_solid(bitmap->dc, area, background);
  SetBkMode(bitmap->dc, TRANSPARENT);
  if (mode == DisplayMode::listing) {
    draw_listing(bitmap->dc, area, listing_text, sample, dpi, background);
  } else if (mode == DisplayMode::ladder) {
    int y = scaled(10, dpi);
    const int gutter = scaled(50, dpi);
    const HFONT gutter_font = chrome && chrome->mono_status ? mono_font_ : ui_font_;
    HGDIOBJ ui_previous = SelectObject(bitmap->dc, gutter_font);
    SetTextColor(bitmap->dc, canvas_palette.muted);
    for (float size : ladder_sizes) {
      const int line_height = std::max(scaled(24, dpi), point_size_px(size, dpi) * 3 / 2);
      const std::wstring label = std::to_wstring(static_cast<int>(std::lround(size))) + L" pt";
      SelectObject(bitmap->dc, ui_previous);
      HFONT sample_font = create_sample_font(sample.font_face, size, dpi, sample.bold, sample.italic);
      HGDIOBJ previous_font = SelectObject(bitmap->dc, sample_font);
      TEXTMETRICW metrics{};
      GetTextMetricsW(bitmap->dc, &metrics);
      SelectObject(bitmap->dc, gutter_font);
      /* The label sits on the sample's baseline; its box starts above the
         line so a gutter font taller than a small sample is not clipped. */
      RECT label_area{scaled(6, dpi), y - line_height, gutter - scaled(4, dpi),
                      y + metrics.tmAscent};
      DrawTextW(bitmap->dc, label.c_str(), -1, &label_area, DT_RIGHT | DT_BOTTOM | DT_SINGLELINE);
      SelectObject(bitmap->dc, sample_font);
      SetTextColor(bitmap->dc, foreground);
      RECT line_area{gutter, y, content_width - scaled(8, dpi), y + line_height};
      const std::size_t newline = sample.text.find(L'\n');
      const std::size_t length = newline == std::wstring::npos ? sample.text.size() : newline;
      ExtTextOutW(bitmap->dc, line_area.left, y, ETO_CLIPPED, &line_area, sample.text.c_str(),
                  static_cast<UINT>(length), nullptr);
      SelectObject(bitmap->dc, previous_font);
      DeleteObject(sample_font);
      ui_previous = SelectObject(bitmap->dc, gutter_font);
      y += line_height;
    }
    SelectObject(bitmap->dc, ui_previous);
  } else if (mode == DisplayMode::compare) {
    HGDIOBJ previous_font = SelectObject(bitmap->dc, ui_font_);
    SetTextColor(bitmap->dc, canvas_palette.muted);
    RECT left_header{scaled(10, dpi), 0, compare_half, compare_header};
    RECT right_header{compare_half + compare_gap + scaled(10, dpi), 0, content_width,
                      compare_header};
    DrawTextW(bitmap->dc, labels.compare_mactype.c_str(), -1, &left_header,
              DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    DrawTextW(bitmap->dc, labels.compare_windows.c_str(), -1, &right_header,
              DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    SelectObject(bitmap->dc, previous_font);
    RECT left{scaled(10, dpi), compare_header, compare_half - scaled(10, dpi), height};
    RECT right{compare_half + compare_gap + scaled(10, dpi), compare_header,
               content_width - scaled(10, dpi), height};
    HFONT sample_font = create_sample_font(sample.font_face, sample.font_size_pt, dpi, sample.bold, sample.italic);
    previous_font = SelectObject(bitmap->dc, sample_font);
    wrapped_text_height(bitmap->dc, left, sample.text, sample_line_height, foreground);
    const BOOL disabled = control_center_ ? control_center_->EnableRender(FALSE) : FALSE;
    struct RenderRestore {
      IControlCenter* control_center;
      BOOL disabled;
      ~RenderRestore() {
        if (control_center && disabled) control_center->EnableRender(TRUE);
      }
    } restore{control_center_, disabled};
    if (disabled) {
      wrapped_text_height(bitmap->dc, right, sample.text, sample_line_height, foreground);
    } else {
      SelectObject(bitmap->dc, previous_font);
      HGDIOBJ unavailable_previous = SelectObject(bitmap->dc, ui_font_);
      SetTextColor(bitmap->dc, canvas_palette.muted);
      DrawTextW(bitmap->dc, labels.compare_unavailable.c_str(), -1, &right,
                DT_CENTER | DT_VCENTER | DT_WORDBREAK);
      SelectObject(bitmap->dc, unavailable_previous);
      previous_font = SelectObject(bitmap->dc, sample_font);
    }
    SelectObject(bitmap->dc, previous_font);
    DeleteObject(sample_font);
  } else {
    RECT text_area{sample_margin, sample_margin, content_width - sample_margin, height - sample_margin};
    HFONT sample_font = create_sample_font(sample.font_face, sample.font_size_pt, dpi, sample.bold, sample.italic);
    HGDIOBJ previous_font = SelectObject(bitmap->dc, sample_font);
    wrapped_text_height(bitmap->dc, text_area, sample.text, sample_line_height, foreground);
    SelectObject(bitmap->dc, previous_font);
    DeleteObject(sample_font);
  }
  auto* pixels = static_cast<std::uint8_t*>(bitmap->bits);
  for (std::size_t index = 3; index < static_cast<std::size_t>(bitmap->width) * bitmap->height * 4U;
       index += 4) {
    pixels[index] = 0xFF;
  }
  return bitmap;
}

void PreviewRuntime::paint_native(HWND window) {
  PAINTSTRUCT paint{};
  HDC target = BeginPaint(window, &paint);
  RECT client{};
  GetClientRect(window, &client);
  CanvasBitmap buffer(window, client.right, client.bottom);
  if (!buffer.valid()) {
    EndPaint(window, &paint);
    return;
  }
  const Palette& palette = this->palette();
  fill_solid(buffer.dc, client, palette.canvas);
  const int edit_height = edit_visible_ ? scaled(72, native_dpi_) : 0;
  const int status_height = chrome_metric(&NativeChrome::status_height, 26);
  RECT toolbar{0, 0, client.right, toolbar_layout_height_};
  draw_toolbar(buffer.dc, toolbar);
  const int canvas_top = toolbar_layout_height_ + edit_height;
  RECT canvas_view{0, canvas_top, client.right,
                   std::max(canvas_top, static_cast<int>(client.bottom) - status_height)};
  fill_solid(buffer.dc, canvas_view, palette.canvas);
  if (edit_visible_) {
    RECT edit_rect{};
    GetWindowRect(edit_control_, &edit_rect);
    MapWindowPoints(HWND_DESKTOP, native_window_, reinterpret_cast<POINT*>(&edit_rect), 2);
    InflateRect(&edit_rect, 1, 1);
    HBRUSH border_brush = CreateSolidBrush(palette.border);
    FrameRect(buffer.dc, &edit_rect, border_brush);
    DeleteObject(border_brush);
  }
  const int padding = chrome_metric(&NativeChrome::canvas_inset, 18);
  const int available_width = std::max(1, static_cast<int>(canvas_view.right) - 2 * padding);
  const int available_height = std::max(
      1, static_cast<int>(canvas_view.bottom - canvas_view.top) - 2 * padding);
  const int source_width = std::max(1, available_width / zoom_);
  const int source_min_height = std::max(1, available_height / zoom_);
  CanvasBitmap* canvas = cached_native_canvas(native_canvas_key(source_width, source_min_height));
  if (canvas) {
    const int drawn_width = canvas->width * zoom_;
    const int drawn_height = canvas->height * zoom_;
    update_scroll_bounds(drawn_height + 2 * padding - (canvas_view.bottom - canvas_view.top));
    SetStretchBltMode(buffer.dc, COLORONCOLOR);
    StretchBlt(buffer.dc, padding, canvas_top + padding - scroll_y_, drawn_width, drawn_height,
               canvas->dc, 0, 0, canvas->width, canvas->height, SRCCOPY);
    RECT canvas_frame{padding - 1, canvas_top + padding - scroll_y_ - 1,
                      padding + drawn_width + 1, canvas_top + padding - scroll_y_ + drawn_height + 1};
    HRGN frame_region = CreateRoundRectRgn(canvas_frame.left, canvas_frame.top,
                                            canvas_frame.right + 1, canvas_frame.bottom + 1,
                                            chrome_metric(&NativeChrome::canvas_radius, 4),
                                            chrome_metric(&NativeChrome::canvas_radius, 4));
    HBRUSH frame_brush = CreateSolidBrush(palette.border);
    FrameRgn(buffer.dc, frame_region, frame_brush, 1, 1);
    DeleteObject(frame_brush);
    DeleteObject(frame_region);
    if (zoom_ == 4) {
      const COLORREF grid = blend_color(native_foreground_, native_background_, 20);
      HPEN pen = CreatePen(PS_SOLID, 1, grid);
      HGDIOBJ previous_pen = SelectObject(buffer.dc, pen);
      const int left = padding;
      const int top = canvas_top + padding - scroll_y_;
      for (int x = 0; x <= canvas->width; ++x) {
        MoveToEx(buffer.dc, left + x * 4, std::max(static_cast<int>(canvas_view.top), top), nullptr);
        LineTo(buffer.dc, left + x * 4,
               std::min(static_cast<int>(canvas_view.bottom), top + drawn_height));
      }
      for (int y = 0; y <= canvas->height; ++y) {
        const int line_y = top + y * 4;
        if (line_y >= canvas_view.top && line_y <= canvas_view.bottom) {
          MoveToEx(buffer.dc, left, line_y, nullptr);
          LineTo(buffer.dc, std::min(static_cast<int>(canvas_view.right), left + drawn_width), line_y);
        }
      }
      SelectObject(buffer.dc, previous_pen);
      DeleteObject(pen);
    }
    if (loupe_ && mouse_inside_ && PtInRect(&canvas_view, mouse_position_)) {
      const int source_x = std::clamp((static_cast<int>(mouse_position_.x) - padding) / zoom_, 0,
                                      canvas->width - 1);
      const int source_y = std::clamp(
          (static_cast<int>(mouse_position_.y) - canvas_top - padding + scroll_y_) / zoom_, 0,
          canvas->height - 1);
      const int source_side = 20;
      const int loupe_side = scaled(160, native_dpi_);
      int loupe_x = mouse_position_.x + scaled(18, native_dpi_);
      int loupe_y = mouse_position_.y + scaled(18, native_dpi_);
      if (loupe_x + loupe_side > client.right) loupe_x = mouse_position_.x - loupe_side - scaled(18, native_dpi_);
      if (loupe_y + loupe_side > client.bottom) loupe_y = mouse_position_.y - loupe_side - scaled(18, native_dpi_);
      SetStretchBltMode(buffer.dc, COLORONCOLOR);
      StretchBlt(buffer.dc, loupe_x, loupe_y, loupe_side, loupe_side, canvas->dc,
                 source_x - source_side / 2, source_y - source_side / 2, source_side, source_side,
                 SRCCOPY);
      HPEN frame = CreatePen(PS_SOLID, 1, palette.border);
      HGDIOBJ previous_pen = SelectObject(buffer.dc, frame);
      HGDIOBJ previous_brush = SelectObject(buffer.dc, GetStockObject(NULL_BRUSH));
      Rectangle(buffer.dc, loupe_x, loupe_y, loupe_x + loupe_side, loupe_y + loupe_side);
      SelectObject(buffer.dc, previous_pen);
      DeleteObject(frame);
      HPEN marker = CreatePen(PS_SOLID, 1, palette.accent);
      previous_pen = SelectObject(buffer.dc, marker);
      const int cell = loupe_side / source_side;
      Rectangle(buffer.dc, loupe_x + (source_side / 2) * cell,
                loupe_y + (source_side / 2) * cell,
                loupe_x + (source_side / 2 + 1) * cell + 1,
                loupe_y + (source_side / 2 + 1) * cell + 1);
      SelectObject(buffer.dc, previous_brush);
      SelectObject(buffer.dc, previous_pen);
      DeleteObject(marker);
    }
  }
  RECT status{0, client.bottom - status_height, client.right, client.bottom};
  fill_solid(buffer.dc, status, palette.surface);
  const bool hairlines = true;
  if (hairlines) {
    HPEN pen = CreatePen(PS_SOLID, 1, palette.border);
    HGDIOBJ previous_pen = SelectObject(buffer.dc, pen);
    MoveToEx(buffer.dc, 0, toolbar_layout_height_ - 1, nullptr);
    LineTo(buffer.dc, client.right, toolbar_layout_height_ - 1);
    MoveToEx(buffer.dc, 0, status.top, nullptr);
    LineTo(buffer.dc, client.right, status.top);
    SelectObject(buffer.dc, previous_pen);
    DeleteObject(pen);
  }
  status.left += scaled(10, native_dpi_);
  status.right -= scaled(10, native_dpi_);
  HGDIOBJ previous_font = SelectObject(buffer.dc,
                                       chrome_ && chrome_->mono_status ? mono_font_ : ui_font_);
  SetBkMode(buffer.dc, TRANSPARENT);
  SetTextColor(buffer.dc, palette.muted);
  const std::wstring text = temporary_status_.empty() ? status_text() : temporary_status_;
  DrawTextW(buffer.dc, text.c_str(), -1, &status, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
  SelectObject(buffer.dc, previous_font);
  BitBlt(target, 0, 0, client.right, client.bottom, buffer.dc, 0, 0, SRCCOPY);
  EndPaint(window, &paint);
}

void PreviewRuntime::update_scroll_bounds(int overflow) {
  scroll_max_ = std::max(0, overflow);
  scroll_y_ = std::clamp(scroll_y_, 0, scroll_max_);
}

std::wstring PreviewRuntime::mode_label() const {
  switch (display_mode_) {
    case DisplayMode::sample: return labels_.mode_sample;
    case DisplayMode::ladder: return labels_.mode_ladder;
    case DisplayMode::compare: return labels_.mode_compare;
    case DisplayMode::listing: return labels_.mode_listing;
  }
  return labels_.mode_sample;
}

std::wstring PreviewRuntime::status_text() const {
  std::wstring version = labels_.core_version;
  const std::wstring marker = L"{version}";
  const std::size_t marker_position = version.find(marker);
  if (marker_position != std::wstring::npos) {
    version.replace(marker_position, marker.size(), format_core_version(core_version_));
  }
  std::wostringstream status;
  status << native_sample_.font_face << L" · " << static_cast<int>(std::lround(native_sample_.font_size_pt)) << L" pt · "
         << native_dpi_ << L" DPI · " << version << L" · " << mode_label() << L" · "
         << labels_.engine_mactype;
  return status.str();
}

int PreviewRuntime::hit_test_toolbar(POINT point) const {
  for (const auto& [action, rectangle] : toolbar_buttons_) {
    if (PtInRect(&rectangle, point)) return action;
  }
  return kNoAction;
}

void PreviewRuntime::execute_toolbar_action(int action) {
  switch (action) {
    case kBold: native_sample_.bold = !native_sample_.bold; break;
    case kItalic: native_sample_.italic = !native_sample_.italic; break;
    case kModeSample: display_mode_ = DisplayMode::sample; scroll_y_ = 0; break;
    case kModeLadder: display_mode_ = DisplayMode::ladder; scroll_y_ = 0; break;
    case kModeCompare: display_mode_ = DisplayMode::compare; scroll_y_ = 0; break;
    case kModeListing: display_mode_ = DisplayMode::listing; scroll_y_ = 0; break;
    case kInvert:
      std::swap(native_foreground_, native_background_);
      inverted_ = !inverted_;
      break;
    case kLoupe: loupe_ = !loupe_; break;
    case kZoom:
      zoom_ = zoom_ == 1 ? 2 : (zoom_ == 2 ? 4 : 1);
      scroll_y_ = 0;
      rebuild_toolbar_layout();
      break;
    case kTopmost:
      topmost_ = !topmost_;
      SetWindowPos(native_window_, topmost_ ? HWND_TOPMOST : HWND_NOTOPMOST, 0, 0, 0, 0,
                   SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
      break;
    case kEditText:
      edit_visible_ = !edit_visible_;
      ShowWindow(edit_control_, edit_visible_ ? SW_SHOW : SW_HIDE);
      relayout_controls();
      break;
    case kSavePng: save_canvas_png(); break;
    case kCopy: copy_canvas(); break;
    default: break;
  }
  InvalidateRect(native_window_, nullptr, FALSE);
}

bool PreviewRuntime::handle_key(WPARAM key) {
  const bool edit_focused = GetFocus() == edit_control_;
  const bool control = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
  const bool shift = (GetKeyState(VK_SHIFT) & 0x8000) != 0;
  if (key == VK_ESCAPE) {
    hide_native_window(true);
    return true;
  }
  if (control && key == 'S') return save_canvas_png();
  if (control && key == 'C' && !edit_focused) return copy_canvas();
  if (edit_focused) return false;
  if (key == 'I' && shift) execute_toolbar_action(kItalic);
  else if (key == 'I') execute_toolbar_action(kInvert);
  else if (key == 'B') execute_toolbar_action(kBold);
  else if (key == VK_ADD || key == VK_OEM_PLUS) {
    if (zoom_ < 4) zoom_ *= 2;
    rebuild_toolbar_layout();
    InvalidateRect(native_window_, nullptr, FALSE);
  } else if (key == VK_SUBTRACT || key == VK_OEM_MINUS) {
    if (zoom_ > 1) zoom_ /= 2;
    rebuild_toolbar_layout();
    InvalidateRect(native_window_, nullptr, FALSE);
  } else if (key == VK_HOME || key == VK_END) {
    scroll_y_ = key == VK_HOME ? 0 : scroll_max_;
    InvalidateRect(native_window_, nullptr, FALSE);
  } else if (key == VK_DOWN || key == VK_UP || key == VK_NEXT || key == VK_PRIOR) {
    const int step = scaled(key == VK_NEXT || key == VK_PRIOR ? 240 : 48, native_dpi_);
    const int direction = key == VK_DOWN || key == VK_NEXT ? 1 : -1;
    scroll_y_ = std::clamp(scroll_y_ + direction * step, 0, scroll_max_);
    InvalidateRect(native_window_, nullptr, FALSE);
  } else {
    return false;
  }
  return true;
}

void PreviewRuntime::set_temporary_status(const std::wstring& text) {
  temporary_status_ = text;
  SetTimer(native_window_, kStatusTimer, 2000, nullptr);
  InvalidateRect(native_window_, nullptr, FALSE);
}

bool PreviewRuntime::save_canvas_png() {
  if (save_in_progress_) return false;
  if (save_thread_.joinable()) save_thread_.join();

  RECT client{};
  GetClientRect(native_window_, &client);
  const int width = std::max(1, (static_cast<int>(client.right) - 2 * scaled(18, native_dpi_)) / zoom_);
  auto canvas = render_native_canvas(width, scaled(300, native_dpi_), native_sample_,
                                     native_foreground_, native_background_, native_dpi_,
                                     display_mode_, zoom_, inverted_, ladder_sizes_, listing_text_,
                                     labels_, chrome_, dark_theme_);
  if (!canvas) return false;
  std::string error;
  auto png = encode_png(static_cast<std::uint32_t>(canvas->width),
                        static_cast<std::uint32_t>(canvas->height),
                        static_cast<std::uint32_t>(canvas->width) * 4U,
                        static_cast<const std::uint8_t*>(canvas->bits), error);
  if (png.empty()) return false;

  std::wstring filter = labels_.png_filter;
  std::replace(filter.begin(), filter.end(), L'|', L'\0');
  filter.push_back(L'\0');
  filter.push_back(L'\0');
  if (filter.size() < 3 || filter[filter.size() - 3] == L'\0') {
    filter = std::wstring(L"PNG files (*.png)\0*.png\0\0", 25);
  }
  const HWND owner = native_window_;
  save_in_progress_ = true;
  ++save_thread_started_;
  save_thread_ = std::thread([owner, filter = std::move(filter), png = std::move(png)]() mutable {
    const HRESULT com = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    wchar_t file[MAX_PATH] = L"mactype-preview.png";
    OPENFILENAMEW dialog{sizeof(dialog)};
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter.c_str();
    dialog.lpstrFile = file;
    dialog.nMaxFile = MAX_PATH;
    dialog.lpstrDefExt = L"png";
    dialog.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
    bool succeeded = false;
    if (GetSaveFileNameW(&dialog)) {
      std::ofstream output(std::filesystem::path(file), std::ios::binary | std::ios::trunc);
      output.write(reinterpret_cast<const char*>(png.data()),
                   static_cast<std::streamsize>(png.size()));
      succeeded = static_cast<bool>(output);
    }
    if (SUCCEEDED(com)) CoUninitialize();
    PostMessageW(owner, kSaveComplete, succeeded ? TRUE : FALSE, 0);
  });
  return true;
}

bool PreviewRuntime::copy_canvas() {
  RECT client{};
  GetClientRect(native_window_, &client);
  const int width = std::max(1, (static_cast<int>(client.right) - 2 * scaled(18, native_dpi_)) / zoom_);
  auto canvas = render_native_canvas(width, scaled(300, native_dpi_), native_sample_,
                                     native_foreground_, native_background_, native_dpi_,
                                     display_mode_, zoom_, inverted_, ladder_sizes_, listing_text_,
                                     labels_, chrome_, dark_theme_);
  if (!canvas) return false;
  const SIZE_T pixel_bytes = static_cast<SIZE_T>(canvas->width) * canvas->height * 4U;
  const SIZE_T total = sizeof(BITMAPINFOHEADER) + pixel_bytes;
  HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, total);
  if (!memory) return false;
  auto* destination = static_cast<std::uint8_t*>(GlobalLock(memory));
  if (!destination) {
    GlobalFree(memory);
    return false;
  }
  BITMAPINFOHEADER header{};
  header.biSize = sizeof(header);
  header.biWidth = canvas->width;
  header.biHeight = canvas->height;
  header.biPlanes = 1;
  header.biBitCount = 32;
  header.biCompression = BI_RGB;
  std::memcpy(destination, &header, sizeof(header));
  const auto* source = static_cast<const std::uint8_t*>(canvas->bits);
  const SIZE_T row_bytes = static_cast<SIZE_T>(canvas->width) * 4U;
  for (int row = 0; row < canvas->height; ++row) {
    std::memcpy(destination + sizeof(header) + static_cast<SIZE_T>(row) * row_bytes,
                source + static_cast<SIZE_T>(canvas->height - 1 - row) * row_bytes, row_bytes);
  }
  GlobalUnlock(memory);
  if (!OpenClipboard(native_window_)) {
    GlobalFree(memory);
    return false;
  }
  EmptyClipboard();
  const HANDLE placed = SetClipboardData(CF_DIB, memory);
  CloseClipboard();
  if (!placed) {
    GlobalFree(memory);
    return false;
  }
  set_temporary_status(labels_.copied);
  return true;
}

LRESULT CALLBACK PreviewRuntime::edit_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
  auto* runtime = reinterpret_cast<PreviewRuntime*>(GetWindowLongPtrW(window, GWLP_USERDATA));
  if (runtime && message == WM_KEYDOWN && runtime->handle_key(wparam)) return 0;
  return runtime && runtime->edit_original_proc_
             ? CallWindowProcW(runtime->edit_original_proc_, window, message, wparam, lparam)
             : DefWindowProcW(window, message, wparam, lparam);
}

LRESULT CALLBACK PreviewRuntime::window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam) {
  if (message == WM_NCCREATE) {
    const auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
    SetWindowLongPtrW(window, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(create->lpCreateParams));
  }
  auto* runtime = reinterpret_cast<PreviewRuntime*>(GetWindowLongPtrW(window, GWLP_USERDATA));
  if (!runtime) return DefWindowProcW(window, message, wparam, lparam);
  switch (message) {
    case WM_PAINT:
      if (window == runtime->native_window_) runtime->paint_native(window);
      else {
        PAINTSTRUCT paint{};
        BeginPaint(window, &paint);
        EndPaint(window, &paint);
      }
      return 0;
    case WM_ERASEBKGND: return 1;
    case WM_CLOSE:
      if (window == runtime->native_window_) runtime->hide_native_window(true);
      else ShowWindow(window, SW_HIDE);
      return 0;
    case kSaveComplete:
      if (window == runtime->native_window_) {
        runtime->save_in_progress_ = false;
        if (runtime->save_thread_.joinable()) runtime->save_thread_.join();
        if (wparam != FALSE) runtime->set_temporary_status(runtime->labels_.saved);
      }
      return 0;
    case WM_GETMINMAXINFO:
      if (window == runtime->native_window_) {
        auto* minimum = reinterpret_cast<MINMAXINFO*>(lparam);
        RECT desired{0, 0, std::max(scaled(720, runtime->native_dpi_), runtime->minimum_client_width_),
                     scaled(440, runtime->native_dpi_)};
        AdjustWindowRectExForDpi(&desired, WS_OVERLAPPEDWINDOW, FALSE, 0, runtime->native_dpi_);
        minimum->ptMinTrackSize.x = desired.right - desired.left;
        minimum->ptMinTrackSize.y = desired.bottom - desired.top;
      }
      return 0;
    case WM_DPICHANGED:
      if (window == runtime->native_window_) {
        const RECT* suggested = reinterpret_cast<const RECT*>(lparam);
        SetWindowPos(window, nullptr, suggested->left, suggested->top,
                     suggested->right - suggested->left, suggested->bottom - suggested->top,
                     SWP_NOZORDER | SWP_NOACTIVATE);
        runtime->apply_dpi(HIWORD(wparam));
      }
      return 0;
    case WM_SIZE:
      if (window == runtime->native_window_) runtime->relayout_controls();
      return 0;
    case WM_KEYDOWN:
      if (runtime->handle_key(wparam)) return 0;
      break;
    case WM_MOUSEMOVE:
      if (window == runtime->native_window_) {
        runtime->mouse_position_ = {GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        if (!runtime->mouse_inside_) {
          TRACKMOUSEEVENT tracking{sizeof(tracking), TME_LEAVE, window, 0};
          TrackMouseEvent(&tracking);
          runtime->mouse_inside_ = true;
        }
        const int hover = runtime->hit_test_toolbar(runtime->mouse_position_);
        if (hover != runtime->hover_action_ || runtime->loupe_) {
          runtime->hover_action_ = hover;
          InvalidateRect(window, nullptr, FALSE);
        }
      }
      return 0;
    case WM_MOUSELEAVE:
      runtime->mouse_inside_ = false;
      runtime->hover_action_ = kNoAction;
      InvalidateRect(window, nullptr, FALSE);
      return 0;
    case WM_LBUTTONDOWN:
      if (window == runtime->native_window_) {
        POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        runtime->pressed_action_ = runtime->hit_test_toolbar(point);
        if (runtime->pressed_action_) SetCapture(window);
        InvalidateRect(window, nullptr, FALSE);
      }
      return 0;
    case WM_LBUTTONUP:
      if (window == runtime->native_window_) {
        POINT point{GET_X_LPARAM(lparam), GET_Y_LPARAM(lparam)};
        const int action = runtime->hit_test_toolbar(point);
        const int pressed = runtime->pressed_action_;
        runtime->pressed_action_ = kNoAction;
        if (GetCapture() == window) ReleaseCapture();
        if (action && action == pressed) runtime->execute_toolbar_action(action);
        InvalidateRect(window, nullptr, FALSE);
      }
      return 0;
    case WM_MOUSEWHEEL:
      if (window == runtime->native_window_) {
        const int delta = GET_WHEEL_DELTA_WPARAM(wparam);
        runtime->scroll_y_ = std::clamp(
            runtime->scroll_y_ - MulDiv(delta, scaled(48, runtime->native_dpi_), WHEEL_DELTA), 0,
            runtime->scroll_max_);
        InvalidateRect(window, nullptr, FALSE);
      }
      return 0;
    case WM_COMMAND:
      if (LOWORD(wparam) == kFaceCombo && HIWORD(wparam) == CBN_SELCHANGE) {
        wchar_t value[LF_FACESIZE]{};
        const LRESULT selected = SendMessageW(runtime->face_combo_, CB_GETCURSEL, 0, 0);
        if (selected != CB_ERR) {
          SendMessageW(runtime->face_combo_, CB_GETLBTEXT, selected, reinterpret_cast<LPARAM>(value));
          runtime->native_sample_.font_face = value;
          InvalidateRect(window, nullptr, FALSE);
        }
        return 0;
      }
      if (LOWORD(wparam) == kSizeCombo && HIWORD(wparam) == CBN_SELCHANGE) {
        wchar_t value[16]{};
        const LRESULT selected = SendMessageW(runtime->size_combo_, CB_GETCURSEL, 0, 0);
        if (selected != CB_ERR) {
          SendMessageW(runtime->size_combo_, CB_GETLBTEXT, selected, reinterpret_cast<LPARAM>(value));
          runtime->native_sample_.font_size_pt = static_cast<float>(_wtoi(value));
          InvalidateRect(window, nullptr, FALSE);
        }
        return 0;
      }
      if (LOWORD(wparam) == kEditControl && HIWORD(wparam) == EN_CHANGE && !runtime->updating_edit_) {
        const int length = GetWindowTextLengthW(runtime->edit_control_);
        std::wstring value(static_cast<std::size_t>(length) + 1, L'\0');
        GetWindowTextW(runtime->edit_control_, value.data(), length + 1);
        value.resize(static_cast<std::size_t>(length));
        runtime->native_sample_.text = std::move(value);
        InvalidateRect(window, nullptr, FALSE);
        return 0;
      }
      break;
    case WM_DRAWITEM:
      runtime->draw_combo_item(*reinterpret_cast<DRAWITEMSTRUCT*>(lparam));
      return TRUE;
    case WM_MEASUREITEM: {
      auto* item = reinterpret_cast<MEASUREITEMSTRUCT*>(lparam);
      item->itemHeight = static_cast<UINT>(scaled(24, runtime->native_dpi_));
      return TRUE;
    }
    case WM_CTLCOLORLISTBOX:
    case WM_CTLCOLOREDIT: {
      const Palette& palette = runtime->palette();
      HDC dc = reinterpret_cast<HDC>(wparam);
      SetTextColor(dc, palette.text);
      SetBkColor(dc, palette.surface);
      /* The frame around the edit control belongs to the parent's own paint;
         invalidating the parent from here would repaint under the edit, which
         repaints the edit, which sends this message again without end. */
      return reinterpret_cast<LRESULT>(message == WM_CTLCOLOREDIT ? runtime->edit_brush_
                                                                  : runtime->surface_brush_);
    }
    case WM_TIMER:
      if (wparam == kStatusTimer) {
        KillTimer(window, kStatusTimer);
        runtime->temporary_status_.clear();
        InvalidateRect(window, nullptr, FALSE);
        return 0;
      }
      break;
    default: break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}

void PreviewRuntime::pump_messages() {
  MSG message{};
  while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
    TranslateMessage(&message);
    DispatchMessageW(&message);
  }
}

}  // namespace mactype
