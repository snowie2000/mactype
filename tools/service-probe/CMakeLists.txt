cmake_minimum_required(VERSION 3.24)

project(mactype_service_probe LANGUAGES CXX)

if(NOT WIN32)
  message(FATAL_ERROR "The MacType service probes only support Windows.")
endif()

if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(PROBE_ARCH_SUFFIX 64)
else()
  set(PROBE_ARCH_SUFFIX 32)
endif()

add_library(service-probe-common STATIC
  child_process.cpp
  child_process.h
  process_observation.cpp
  process_observation.h
  probe_common.cpp
  probe_common.h
  probe_io.cpp
  render_probe.cpp
  render_probe.h
  snapshot_json.cpp
  snapshot_json.h
  win32_support.cpp
  win32_support.h
)
target_compile_features(service-probe-common PUBLIC cxx_std_20)
target_compile_definitions(service-probe-common PUBLIC UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_link_libraries(service-probe-common PUBLIC advapi32 bcrypt gdi32 shell32 user32 version)

add_executable(probe-console probe_main.cpp)
target_link_libraries(probe-console PRIVATE service-probe-common)
set_target_properties(probe-console PROPERTIES OUTPUT_NAME "probe-console${PROBE_ARCH_SUFFIX}")

add_executable(probe-window WIN32 probe_main.cpp)
target_compile_definitions(probe-window PRIVATE SERVICE_PROBE_WINDOW=1)
target_link_libraries(probe-window PRIVATE service-probe-common)
set_target_properties(probe-window PROPERTIES OUTPUT_NAME "probe-window${PROBE_ARCH_SUFFIX}")

add_executable(probe-spawn-tree spawn_tree.cpp)
target_link_libraries(probe-spawn-tree PRIVATE service-probe-common)
set_target_properties(probe-spawn-tree PROPERTIES OUTPUT_NAME "probe-spawn-tree${PROBE_ARCH_SUFFIX}")

add_executable(probe-timeout-fixture EXCLUDE_FROM_ALL
  tests/fixtures/timeout_tree.cpp
)
target_compile_features(probe-timeout-fixture PRIVATE cxx_std_20)
target_compile_definitions(probe-timeout-fixture PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
set_target_properties(probe-timeout-fixture PROPERTIES OUTPUT_NAME "probe-timeout-fixture${PROBE_ARCH_SUFFIX}")

if(MSVC)
  foreach(target service-probe-common probe-console probe-window probe-spawn-tree probe-timeout-fixture)
    target_compile_options(${target} PRIVATE /W4 /WX /permissive- /utf-8 /sdl)
  endforeach()
endif()
