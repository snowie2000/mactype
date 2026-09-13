cmake_minimum_required(VERSION 3.24)
project(mactype_preview32 LANGUAGES CXX)

if(NOT WIN32)
  message(FATAL_ERROR "mactype-preview32 is a Windows-only program")
endif()

if(NOT CMAKE_SIZEOF_VOID_P EQUAL 4)
  message(FATAL_ERROR "mactype-preview32 must be configured with -A Win32")
endif()

add_executable(mactype-preview32
  main.cpp
  preview_runtime.cpp
  protocol.cpp
  include/generated_settings.h
  include/legacy_control_center.h
  include/preview_runtime.h
  include/protocol.h
)
target_compile_features(mactype-preview32 PRIVATE cxx_std_20)
target_include_directories(mactype-preview32 PRIVATE include)
target_compile_definitions(mactype-preview32 PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_link_libraries(mactype-preview32 PRIVATE windowscodecs ole32 shlwapi comctl32 comdlg32 uxtheme)
if(MSVC)
  target_compile_options(mactype-preview32 PRIVATE /W4 /WX /permissive- /utf-8 /guard:cf)
  # HIGHENTROPYVA is only valid for 64-bit images. This helper is deliberately
  # Win32 so it can host the existing 32-bit MacType preview path.
  target_link_options(mactype-preview32 PRIVATE /DYNAMICBASE /NXCOMPAT)
endif()

include(CTest)
if(BUILD_TESTING)
  add_executable(protocol-tests tests/protocol_tests.cpp protocol.cpp include/protocol.h)
  target_compile_features(protocol-tests PRIVATE cxx_std_20)
  target_include_directories(protocol-tests PRIVATE include)
  if(MSVC)
    target_compile_options(protocol-tests PRIVATE /W4 /WX /permissive- /utf-8)
  endif()
  add_test(NAME protocol-tests COMMAND protocol-tests)

  add_library(fake-mactype SHARED tests/fake_mactype.cpp tests/fake_mactype.def)
  target_include_directories(fake-mactype PRIVATE include)
  target_compile_features(fake-mactype PRIVATE cxx_std_20)
  target_compile_definitions(fake-mactype PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
  set_target_properties(fake-mactype PROPERTIES OUTPUT_NAME MacType PREFIX "")

  add_library(fake-easyhook SHARED tests/fake_easyhook.cpp)
  target_compile_features(fake-easyhook PRIVATE cxx_std_20)
  target_compile_definitions(fake-easyhook PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
  set_target_properties(fake-easyhook PROPERTIES OUTPUT_NAME EasyHK32 PREFIX "")

  file(GENERATE OUTPUT "$<TARGET_FILE_DIR:fake-mactype>/MacType.ini"
       CONTENT "[FreeType]\r\nNormalWeight=0\r\nGammaValue=1.0\r\n")

  add_executable(runtime-tests tests/runtime_tests.cpp preview_runtime.cpp protocol.cpp)
  target_include_directories(runtime-tests PRIVATE include)
  target_compile_features(runtime-tests PRIVATE cxx_std_20)
  target_compile_definitions(runtime-tests PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
  target_link_libraries(runtime-tests PRIVATE windowscodecs ole32 shlwapi comctl32 comdlg32 uxtheme)
  add_dependencies(runtime-tests fake-mactype fake-easyhook)
  if(MSVC)
    target_compile_options(fake-mactype PRIVATE /W4 /WX /permissive- /utf-8)
    target_compile_options(fake-easyhook PRIVATE /W4 /WX /permissive- /utf-8)
    target_compile_options(runtime-tests PRIVATE /W4 /WX /permissive- /utf-8)
  endif()
  add_test(NAME runtime-tests COMMAND runtime-tests "$<TARGET_FILE_DIR:fake-mactype>")

  add_executable(manual-launch-target tests/manual_launch_target.cpp)
  target_compile_features(manual-launch-target PRIVATE cxx_std_20)
  target_compile_definitions(manual-launch-target PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
  if(MSVC)
    target_compile_options(manual-launch-target PRIVATE /W4 /WX /permissive- /utf-8)
  endif()
endif()
