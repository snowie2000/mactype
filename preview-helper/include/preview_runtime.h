#pragma once

#include "legacy_control_center.h"
#include "protocol.h"

#include <Windows.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace mactype {

enum class Engine { mactype, plain };

class PreviewRuntime {
  friend struct PreviewRuntimeTestAccess;
 public:
  explicit PreviewRuntime(std::wstring install_root, Engine engine = Engine::mactype);
  ~PreviewRuntime();
  PreviewRuntime(const PreviewRuntime&) = delete;
  PreviewRuntime& operator=(const PreviewRuntime&) = delete;

  bool initialize(std::string& error);
  mtpc::Frame render(const mtpc::Frame& request);
  mtpc::Frame load_profile(const mtpc::Frame& request);
  mtpc::Frame show_native_preview(const mtpc::Frame& request, bool visible);
  std::string hello_json() const;
  void pump_messages();
  void set_state_sink(std::function<void(const mtpc::Frame&)> sink);
  std::wstring selected_face_for_tests() const;
  void close_from_window_for_tests();
  bool save_in_progress_for_tests() const;
  void set_save_in_progress_for_tests(bool in_progress);
  void trigger_save_for_tests();
  std::uint32_t save_thread_started_for_tests() const;

  enum class DisplayMode { sample, ladder, compare, listing };
  enum class Skin { classic };

  struct Palette {
    COLORREF canvas;
    COLORREF surface;
    COLORREF hover;
    COLORREF border;
    COLORREF text;
    COLORREF muted;
    COLORREF accent;
    COLORREF on_accent;
  };

  struct NativeChrome {
    Skin skin;
    Palette palette;
    int radius;
    int control_height;
    int toolbar_height;
    int status_height;
    int canvas_radius;
    int canvas_inset;
    bool mono_status;
  };

 private:
  struct NativeLabels {
    std::wstring title{L"MacType Preview"};
    std::wstring font_face{L"Font"};
    std::wstring font_size{L"Size"};
    std::wstring bold{L"B"};
    std::wstring italic{L"I"};
    std::wstring mode_sample{L"Sample"};
    std::wstring mode_ladder{L"Ladder"};
    std::wstring mode_compare{L"Compare"};
    std::wstring mode_listing{L"Listing"};
    std::wstring invert{L"Invert"};
    std::wstring loupe{L"Loupe"};
    std::wstring zoom{L"Zoom"};
    std::wstring topmost{L"Top"};
    std::wstring edit_text{L"Edit"};
    std::wstring save_png{L"Save"};
    std::wstring copy{L"Copy"};
    std::wstring compare_mactype{L"MacType"};
    std::wstring compare_windows{L"Windows"};
    std::wstring compare_unavailable{L"Comparison unavailable"};
    std::wstring engine_mactype{L"MacType"};
    std::wstring core_version{L"Core {version}"};
    std::wstring png_filter{L"PNG files (*.png)|*.png"};
    std::wstring saved{L"Saved"};
    std::wstring copied{L"Copied"};
  };

  struct CanvasBitmap;

  static LRESULT CALLBACK window_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam);
  static LRESULT CALLBACK edit_proc(HWND window, UINT message, WPARAM wparam, LPARAM lparam);
  bool create_windows(std::string& error);
  bool apply_request(const std::string& json, std::string& error);
  bool apply_native_request(const std::string& json, std::string& error);
  std::vector<std::uint8_t> render_png(const std::string& json, std::uint32_t& width,
                                      std::uint32_t& height, std::uint32_t& dpi, std::string& error);
  void paint_native(HWND window);
  void recreate_ui_font();
  void recreate_palette_brushes();
  void apply_combo_theme();
  const Palette& palette() const;
  int chrome_metric(int NativeChrome::*member, int fallback) const;
  void invalidate_canvas_cache();
  CanvasBitmap* cached_native_canvas(int width, int minimum_height);
  void enumerate_fonts();
  void sync_controls();
  void relayout_controls();
  void rebuild_toolbar_layout();
  void draw_toolbar(HDC dc, const RECT& area);
  void draw_combo_item(const DRAWITEMSTRUCT& item);
  std::unique_ptr<CanvasBitmap> render_native_canvas(int width, int minimum_height);
  void execute_toolbar_action(int action);
  bool handle_key(WPARAM key);
  int hit_test_toolbar(POINT point) const;
  void hide_native_window(bool notify = false);
  void show_native_window();
  void emit_native_state(bool visible);
  void update_scroll_bounds(int canvas_height);
  void set_temporary_status(const std::wstring& text);
  bool save_canvas_png();
  bool copy_canvas();
  std::wstring status_text() const;
  std::wstring mode_label() const;
  std::string native_state_json(bool visible) const;

  Engine engine_;
  std::wstring install_root_;
  std::wstring dll_path_;
  HMODULE module_{};
  IControlCenter* control_center_{};
  HWND hidden_window_{};
  HWND native_window_{};
  HWND face_combo_{};
  HWND size_combo_{};
  HWND edit_control_{};
  WNDPROC edit_original_proc_{};
  HFONT ui_font_{};
  HFONT mono_font_{};
  HBRUSH surface_brush_{};
  HBRUSH edit_brush_{};
  std::wstring sample_text_{L"MacType preview 123 ABC\nThe quick brown fox jumps over the lazy dog."};
  std::wstring font_face_{L"Segoe UI"};
  float font_size_pt_{14.0F};
  bool sample_bold_{false};
  bool sample_italic_{false};
  DisplayMode display_mode_{DisplayMode::sample};
  std::wstring listing_text_{L"The quick brown fox jumps over the lazy dog."};
  COLORREF foreground_{RGB(24, 29, 35)};
  COLORREF background_{RGB(238, 241, 244)};
  COLORREF native_foreground_{RGB(24, 29, 35)};
  COLORREF native_background_{RGB(238, 241, 244)};
  bool inverted_{false};
  bool dark_theme_{false};
  std::optional<NativeChrome> chrome_;
  bool loupe_{false};
  bool mouse_inside_{false};
  bool topmost_{false};
  bool edit_visible_{false};
  bool updating_edit_{false};
  int zoom_{1};
  int scroll_y_{};
  int scroll_max_{};
  int hover_action_{};
  int pressed_action_{};
  int minimum_client_width_{};
  int toolbar_layout_height_{};
  /// Client width at which every toolbar label fits unabbreviated; the first
  /// show grows the window to it so the labels are the words, not initials.
  int full_labels_client_width_{};
  POINT mouse_position_{};
  std::vector<float> ladder_sizes_{8, 9, 10, 11, 12, 14, 16, 18, 20, 24};
  NativeLabels labels_{};
  std::vector<std::wstring> font_names_;
  std::vector<std::pair<int, RECT>> toolbar_buttons_;
  std::vector<std::wstring> toolbar_button_texts_;
  RECT face_label_rect_{};
  RECT size_label_rect_{};
  std::vector<RECT> toolbar_separators_;
  std::wstring temporary_status_;
  WINDOWPLACEMENT placement_{sizeof(WINDOWPLACEMENT)};
  bool has_placement_{false};
  std::uint32_t dpi_{96};
  std::uint32_t native_dpi_{96};
  std::uint32_t core_version_{};
  bool has_dll_get_version_{};
  bool com_initialized_{};
  bool canvas_cache_dirty_{true};
  int canvas_cache_width_{};
  int canvas_cache_minimum_height_{};
  std::unique_ptr<CanvasBitmap> canvas_cache_;
  std::function<void(const mtpc::Frame&)> state_sink_;
  std::thread save_thread_;
  bool save_in_progress_{};
  std::uint32_t save_thread_started_{};
};

}  // namespace mactype
