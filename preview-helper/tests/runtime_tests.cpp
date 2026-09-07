#include "preview_runtime.h"
#include "toolbar_layout_tests.h"

#include <array>
#include <filesystem>
#include <iostream>
#include <string>

int wmain(int argc, wchar_t** argv) {
  if (argc != 2) return 1;
  mactype::PreviewRuntime runtime(std::filesystem::path(argv[1]).wstring());
  std::string error;
  if (!runtime.initialize(error)) {
    std::cerr << error << '\n';
    return 2;
  }
  if (runtime.hello_json().find("\"loadsMacType\":true") == std::string::npos) return 3;

  mtpc::Frame request;
  request.kind = mtpc::MessageKind::render_preview;
  request.request_id = 42;
  request.json = R"({"overrides":{"normal_weight":4,"gamma_value":1.2},"sample":{"text":"MacType preview 123 ABC","fontFace":"Segoe UI","fontSizePt":14,"widthPx":640,"heightPx":180,"dpi":96,"foreground":"#181D23","background":"#EEF1F4"}})";
  const mtpc::Frame response = runtime.render(request);
  if (response.kind != mtpc::MessageKind::preview_rendered || response.request_id != 42) return 4;
  constexpr std::array<std::uint8_t, 8> signature{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
  if (response.binary.size() <= signature.size() ||
      !std::equal(signature.begin(), signature.end(), response.binary.begin())) {
    return 5;
  }

  mtpc::Frame styled;
  styled.kind = mtpc::MessageKind::render_preview;
  styled.request_id = 43;
  styled.json = R"({"overrides":{},"sample":{"text":"MacType preview 123 ABC","fontFace":"Segoe UI","fontSizePt":14,"widthPx":640,"heightPx":96,"dpi":96,"foreground":"#181D23","background":"#EEF1F4","bold":true,"italic":true}})";
  const mtpc::Frame styledResponse = runtime.render(styled);
  if (styledResponse.kind != mtpc::MessageKind::preview_rendered || styledResponse.request_id != 43) return 8;
  if (styledResponse.binary.size() <= signature.size() ||
      !std::equal(signature.begin(), signature.end(), styledResponse.binary.begin())) {
    return 9;
  }

  if (runtime.show_native_preview(request, true).kind != mtpc::MessageKind::native_preview_state) return 6;
  runtime.pump_messages();
  if (runtime.show_native_preview(request, false).kind != mtpc::MessageKind::native_preview_state) return 7;

  mtpc::Frame native;
  native.kind = mtpc::MessageKind::show_native_preview;
  native.request_id = 44;
  native.json = R"({"displayMode":"sample","text":"Native preview 한글 ABC","listingText":"Listing text","fontFace":"Segoe UI","fontSizePt":16,"bold":true,"italic":true,"foreground":"#112233","background":"#EEDDCC","theme":"dark","inverted":true,"zoom":4,"sizes":[8,10,12,16,24],"labels":{"title":"Preview Tool","fontFace":"Face","fontSize":"Size","bold":"Bold","italic":"Italic","modeSample":"Sample","modeLadder":"Ladder","modeCompare":"Compare","modeListing":"Listing","invert":"Invert","loupe":"Loupe","zoom":"Zoom","topmost":"Topmost","editText":"Edit","savePng":"Save PNG","copy":"Copy","compareMacType":"MacType","compareWindows":"Windows","compareUnavailable":"Unavailable","engineMacType":"MacType","coreVersion":"Core {version}","pngFilter":"PNG files (*.png)|*.png","saved":"Saved","copied":"Copied"}})";
  const mtpc::Frame native_response = runtime.show_native_preview(native, true);
  if (native_response.kind != mtpc::MessageKind::native_preview_state) return 10;
  if (native_response.json.find("\"visible\":true") == std::string::npos ||
      native_response.json.find("\"displayMode\":\"sample\"") == std::string::npos ||
      native_response.json.find("\"background\":\"#112233\"") == std::string::npos ||
      native_response.json.find("\"foreground\":\"#EEDDCC\"") == std::string::npos ||
      native_response.json.find("\"inverted\":true") == std::string::npos ||
      native_response.json.find("\"zoom\":4") == std::string::npos ||
      native_response.json.find("\"fontFace\":\"Segoe UI\"") == std::string::npos ||
      native_response.json.find("\"fontSizePt\":16") == std::string::npos ||
      native_response.json.find("\"bold\":true") == std::string::npos ||
      native_response.json.find("\"italic\":true") == std::string::npos ||
      native_response.json.find("\"topmost\":false") == std::string::npos) {
    return 11;
  }
  runtime.pump_messages();
  if (runtime.selected_face_for_tests() != L"Segoe UI") return 28;

  for (const char* skin : {"classic"}) {
    mtpc::Frame skin_request;
    skin_request.kind = mtpc::MessageKind::show_native_preview;
    skin_request.request_id = 49;
    skin_request.json = std::string{R"({"chrome":{"skin":")"} + skin +
        R"(","canvas":"#E4E8EC","surface":"#F4F6F8","surfaceSubtle":"#FFFFFF","border":"#D2D8DF","text":"#1B2129","muted":"#5A6673","accent":"#0B8E9F","onAccent":"#FFFFFF","radius":4,"controlHeight":26,"toolbarHeight":36,"statusHeight":24,"canvasRadius":4,"canvasInset":10,"monoStatus":true}})";
    const mtpc::Frame skin_response = runtime.show_native_preview(skin_request, true);
    if (skin_response.kind != mtpc::MessageKind::native_preview_state ||
        skin_response.json.find(std::string{"\"skin\":\""} + skin + "\"") == std::string::npos) {
      return 29;
    }
    runtime.pump_messages();
  }

  mtpc::Frame no_chrome;
  no_chrome.kind = mtpc::MessageKind::show_native_preview;
  no_chrome.request_id = 50;
  no_chrome.json = R"({"theme":"light"})";
  const mtpc::Frame fallback_response = runtime.show_native_preview(no_chrome, true);
  if (fallback_response.json.find("\"skin\"") != std::string::npos) return 30;
  runtime.pump_messages();

  for (const char* mode : {"ladder", "compare", "listing", "sample"}) {
    mtpc::Frame mode_request;
    mode_request.kind = mtpc::MessageKind::show_native_preview;
    mode_request.request_id = 45;
    mode_request.json = std::string{"{\"displayMode\":\""} + mode + "\"}";
    const mtpc::Frame mode_response = runtime.show_native_preview(mode_request, true);
    if (mode_response.json.find(std::string{"\"displayMode\":\""} + mode + "\"") ==
        std::string::npos) {
      return 12;
    }
    runtime.pump_messages();
  }

  mtpc::Frame omitted;
  omitted.kind = mtpc::MessageKind::show_native_preview;
  omitted.request_id = 46;
  omitted.json = "{}";
  const mtpc::Frame preserved = runtime.show_native_preview(omitted, true);
  if (preserved.json.find("\"background\":\"#112233\"") == std::string::npos ||
      preserved.json.find("\"foreground\":\"#EEDDCC\"") == std::string::npos ||
      preserved.json.find("\"zoom\":4") == std::string::npos ||
      preserved.json.find("\"fontSizePt\":16") == std::string::npos) {
    return 13;
  }
  runtime.pump_messages();

  mtpc::Frame inverted_colors;
  inverted_colors.kind = mtpc::MessageKind::show_native_preview;
  inverted_colors.request_id = 51;
  inverted_colors.json = R"({"foreground":"#111111","background":"#EEEEEE","inverted":true})";
  const mtpc::Frame inverted_colors_response = runtime.show_native_preview(inverted_colors, true);
  if (inverted_colors_response.json.find("\"background\":\"#111111\"") == std::string::npos ||
      inverted_colors_response.json.find("\"foreground\":\"#EEEEEE\"") == std::string::npos ||
      inverted_colors_response.json.find("\"inverted\":true") == std::string::npos) {
    return 31;
  }
  mtpc::Frame restore_colors;
  restore_colors.kind = mtpc::MessageKind::show_native_preview;
  restore_colors.request_id = 52;
  restore_colors.json = R"({"inverted":false})";
  const mtpc::Frame restore_colors_response = runtime.show_native_preview(restore_colors, true);
  if (restore_colors_response.json.find("\"background\":\"#EEEEEE\"") == std::string::npos ||
      restore_colors_response.json.find("\"foreground\":\"#111111\"") == std::string::npos ||
      restore_colors_response.json.find("\"inverted\":false") == std::string::npos) {
    return 32;
  }

  mtpc::Frame escaped_face;
  escaped_face.kind = mtpc::MessageKind::show_native_preview;
  escaped_face.request_id = 47;
  escaped_face.json = R"({"fontFace":"Path\\Face"})";
  const mtpc::Frame escaped_face_response = runtime.show_native_preview(escaped_face, true);
  if (escaped_face_response.kind != mtpc::MessageKind::native_preview_state ||
      escaped_face_response.json.find(R"("fontFace":"Path\\Face")") == std::string::npos) {
    return 25;
  }
  if (runtime.selected_face_for_tests() != L"Path\\Face") return 33;

  mtpc::Frame oversized_text;
  oversized_text.kind = mtpc::MessageKind::show_native_preview;
  oversized_text.request_id = 53;
  oversized_text.json = std::string{"{\"text\":\""} + std::string(4097, 'a') + "\"}";
  if (runtime.show_native_preview(oversized_text, true).kind != mtpc::MessageKind::error) return 34;

  mtpc::Frame oversized_listing;
  oversized_listing.kind = mtpc::MessageKind::show_native_preview;
  oversized_listing.request_id = 54;
  oversized_listing.json =
      std::string{"{\"listingText\":\""} + std::string(4097, 'a') + "\"}";
  if (runtime.show_native_preview(oversized_listing, true).kind != mtpc::MessageKind::error) return 35;

  mtpc::Frame transactional_invalid;
  transactional_invalid.kind = mtpc::MessageKind::show_native_preview;
  transactional_invalid.request_id = 48;
  transactional_invalid.json = R"({"displayMode":"ladder","zoom":3})";
  if (runtime.show_native_preview(transactional_invalid, true).kind != mtpc::MessageKind::error) {
    return 26;
  }
  const mtpc::Frame after_transactional_error = runtime.show_native_preview(omitted, true);
  if (after_transactional_error.kind != mtpc::MessageKind::native_preview_state ||
      after_transactional_error.json != escaped_face_response.json) {
    return 27;
  }
  runtime.pump_messages();
  if (runtime.show_native_preview(omitted, false).kind != mtpc::MessageKind::native_preview_state) return 14;

  std::vector<mtpc::Frame> native_events;
  runtime.set_state_sink([&](const mtpc::Frame& event) { native_events.push_back(event); });
  runtime.show_native_preview(native, true);
  runtime.pump_messages();
  runtime.close_from_window_for_tests();
  runtime.pump_messages();
  if (native_events.size() != 1 ||
      native_events.back().kind != mtpc::MessageKind::native_preview_state ||
      native_events.back().request_id != 0 ||
      native_events.back().json.find("\"visible\":false") == std::string::npos) {
    return 37;
  }
  runtime.show_native_preview(omitted, false);
  if (native_events.size() != 1) return 39;

  runtime.set_save_in_progress_for_tests(true);
  const std::uint32_t save_threads = runtime.save_thread_started_for_tests();
  runtime.trigger_save_for_tests();
  if (!runtime.save_in_progress_for_tests() ||
      runtime.save_thread_started_for_tests() != save_threads) {
    return 38;
  }
  runtime.set_save_in_progress_for_tests(false);

  for (const char* invalid_json : {"{\"zoom\":4.5}", "{\"zoom\":4junk}",
                                   "{\"sizes\":[8,10.5]}", "{\"bold\":truejunk}"}) {
    mtpc::Frame invalid;
    invalid.kind = mtpc::MessageKind::show_native_preview;
    invalid.request_id = 47;
    invalid.json = invalid_json;
    if (runtime.show_native_preview(invalid, true).kind != mtpc::MessageKind::error) return 15;
  }

  mactype::PreviewRuntime plain_runtime(LR"(Z:\missing\arbitrary-root)", mactype::Engine::plain);
  error.clear();
  if (!plain_runtime.initialize(error)) return 19;
  if (plain_runtime.hello_json() !=
      R"({"protocolVersion":1,"renderer":"gdi-plain","loadsMacType":false,"coreVersion":0,"dllGetVersion":false})") {
    return 20;
  }
  mtpc::Frame plain_render;
  plain_render.kind = mtpc::MessageKind::render_preview;
  plain_render.request_id = 48;
  plain_render.json = R"({"profilePath":"Z:\\missing\\arbitrary.ini","overrides":{"normal_weight":999,"gamma_value":99.0},"sample":{"text":"Plain GDI preview","fontFace":"Segoe UI","fontSizePt":13,"widthPx":512,"heightPx":128,"dpi":120,"foreground":"#112233","background":"#F0F0F0","bold":true,"italic":true}})";
  const mtpc::Frame plain_response = plain_runtime.render(plain_render);
  if (plain_response.kind != mtpc::MessageKind::preview_rendered || plain_response.request_id != 48) {
    return 21;
  }
  if (plain_response.binary.size() <= signature.size() ||
      !std::equal(signature.begin(), signature.end(), plain_response.binary.begin())) {
    return 22;
  }
  if (plain_response.json.find("\"engine\":\"plain\"") == std::string::npos ||
      plain_response.json.find("\"coreVersion\":0") == std::string::npos) {
    return 23;
  }
  const mtpc::Frame load_response = plain_runtime.load_profile(plain_render);
  if (load_response.kind != mtpc::MessageKind::ack ||
      load_response.json != R"({"loaded":false,"engine":"plain"})") {
    return 24;
  }
  for (const char* skin : {"classic"}) {
    if (!mactype::PreviewRuntimeTestAccess::toolbar_labels(plain_runtime, skin)) return 80;
  }
  return 0;
}
