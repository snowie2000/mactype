#include "preview_runtime.h"
#include "protocol.h"

#include <Windows.h>
#include <fcntl.h>
#include <io.h>

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <deque>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>

namespace {

std::wstring argument_value(int argc, wchar_t** argv, const wchar_t* name) {
  for (int index = 1; index + 1 < argc; ++index) {
    if (_wcsicmp(argv[index], name) == 0) return argv[index + 1];
  }
  return {};
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
  SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

  if (_setmode(_fileno(stdin), _O_BINARY) == -1 || _setmode(_fileno(stdout), _O_BINARY) == -1) {
    std::cerr << "failed to set binary IPC mode\n";
    return 2;
  }

  const std::wstring engine_value = argument_value(argc, argv, L"--engine");
  mactype::Engine engine = mactype::Engine::mactype;
  if (!engine_value.empty()) {
    if (_wcsicmp(engine_value.c_str(), L"mactype") == 0) {
      engine = mactype::Engine::mactype;
    } else if (_wcsicmp(engine_value.c_str(), L"plain") == 0) {
      engine = mactype::Engine::plain;
    } else {
      std::cerr << "unsupported engine\n";
      return 2;
    }
  }

  mactype::PreviewRuntime runtime(argument_value(argc, argv, L"--install-root"), engine);
  std::string initialization_error;
  if (!runtime.initialize(initialization_error)) {
    std::cerr << "preview initialization failed: " << initialization_error << '\n';
    return 5;
  }

  HANDLE work_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
  if (!work_event) {
    std::cerr << "failed to create IPC event\n";
    return 6;
  }
  std::mutex queue_mutex;
  std::deque<mtpc::Frame> queue;
  std::atomic<bool> input_closed{false};
  std::atomic<bool> protocol_failed{false};
  // Window messages and requests both run on this thread, so event and response writes cannot interleave.
  runtime.set_state_sink([&](const mtpc::Frame& event) {
    if (!mtpc::write_frame(std::cout, event)) {
      std::cerr << "failed to write protocol event\n";
      protocol_failed = true;
      SetEvent(work_event);
    }
  });

  std::thread reader([&] {
    for (;;) {
      mtpc::Frame request;
      std::string error;
      if (!mtpc::read_frame(std::cin, request, error)) {
        if (!std::cin.eof()) {
          protocol_failed = true;
          std::cerr << "protocol error: " << error << '\n';
        }
        input_closed = true;
        SetEvent(work_event);
        return;
      }
      {
        std::scoped_lock lock(queue_mutex);
        if (request.kind == mtpc::MessageKind::render_preview) {
          std::erase_if(queue, [](const mtpc::Frame& pending) {
            return pending.kind == mtpc::MessageKind::render_preview;
          });
        }
        queue.push_back(std::move(request));
      }
      SetEvent(work_event);
    }
  });

  int exit_code = 0;
  bool running = true;
  while (running) {
    const DWORD wait = MsgWaitForMultipleObjects(1, &work_event, FALSE, INFINITE, QS_ALLINPUT);
    if (wait == WAIT_OBJECT_0 + 1) {
      runtime.pump_messages();
      continue;
    }
    if (wait != WAIT_OBJECT_0) {
      exit_code = 7;
      break;
    }
    for (;;) {
      mtpc::Frame request;
      {
        std::scoped_lock lock(queue_mutex);
        if (queue.empty()) break;
        request = std::move(queue.front());
        queue.pop_front();
      }
      mtpc::Frame response;
      response.request_id = request.request_id;
      switch (request.kind) {
        case mtpc::MessageKind::hello:
          response.kind = mtpc::MessageKind::hello_ack;
          response.json = runtime.hello_json();
          break;
        case mtpc::MessageKind::ping:
          response.kind = mtpc::MessageKind::pong;
          response.json = R"({"ok":true})";
          break;
        case mtpc::MessageKind::load_profile:
          response = runtime.load_profile(request);
          break;
        case mtpc::MessageKind::render_preview:
          response = runtime.render(request);
          break;
        case mtpc::MessageKind::show_native_preview:
          response = runtime.show_native_preview(request, true);
          break;
        case mtpc::MessageKind::hide_native_preview:
          response = runtime.show_native_preview(request, false);
          break;
        case mtpc::MessageKind::shutdown:
          response.kind = mtpc::MessageKind::ack;
          response.json = R"({"shutdown":true})";
          running = false;
          break;
        default:
          response.kind = mtpc::MessageKind::error;
          response.json = R"({"code":"unsupported_message","recoverable":true})";
          break;
      }
      if (!mtpc::write_frame(std::cout, response)) {
        std::cerr << "failed to write protocol response\n";
        exit_code = 4;
        running = false;
        break;
      }
    }
    if (input_closed && queue.empty()) {
      if (protocol_failed) exit_code = 3;
      break;
    }
  }

  if (reader.joinable()) {
    if (running) {
      reader.join();
    } else {
      CancelSynchronousIo(reader.native_handle());
      reader.join();
    }
  }
  CloseHandle(work_event);
  return exit_code;
}
