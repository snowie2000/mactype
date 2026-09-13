cmake_minimum_required(VERSION 3.24)

project(mactype_service_injector LANGUAGES CXX)

include(CTest)

if(NOT WIN32 OR NOT MSVC)
  message(FATAL_ERROR "The MacType service injector requires MSVC on Windows.")
endif()

if(CMAKE_SIZEOF_VOID_P EQUAL 8)
  set(_injector_target mactype-injector64)
  set(_injector_link_options /WX /guard:cf /DYNAMICBASE /HIGHENTROPYVA /NXCOMPAT)
else()
  set(_injector_target mactype-injector32)
  set(_injector_link_options /WX /guard:cf /DYNAMICBASE /NXCOMPAT)
endif()

set(_injector_sources
  src/broker_request.cpp
  src/fixed_module.cpp
  src/injector.cpp
  src/module_inventory.cpp
  src/process_lifecycle.cpp
  src/remote_injection.cpp
  src/remote_injection_verdict.cpp
  src/result.cpp
  src/safety_policy.cpp
)

add_executable(${_injector_target}
  ${_injector_sources}
  src/main.cpp
)
target_compile_features(${_injector_target} PRIVATE cxx_std_20)
target_compile_definitions(${_injector_target} PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
target_compile_options(${_injector_target} PRIVATE /W4 /WX /permissive- /utf-8 /EHsc /guard:cf)
target_link_options(${_injector_target} PRIVATE ${_injector_link_options})
target_include_directories(${_injector_target} PRIVATE include)

add_executable(injector-contract-tests
  src/broker_request.cpp
  src/module_inventory.cpp
  src/process_lifecycle.cpp
  src/remote_injection_verdict.cpp
  src/result.cpp
  src/safety_policy.cpp
  tests/contract_tests.cpp
)
target_compile_features(injector-contract-tests PRIVATE cxx_std_20)
target_compile_options(injector-contract-tests PRIVATE /W4 /WX /permissive- /utf-8 /EHsc)
target_include_directories(injector-contract-tests PRIVATE include)

if(BUILD_TESTING)
  add_test(NAME injector-contract-tests COMMAND injector-contract-tests)

  add_executable(injector-marker-target
    src/module_inventory.cpp
    tests/marker_target.cpp
  )
  target_compile_features(injector-marker-target PRIVATE cxx_std_20)
  target_compile_definitions(injector-marker-target PRIVATE UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX)
  target_compile_options(injector-marker-target PRIVATE /W4 /WX /permissive- /utf-8 /EHsc)
  target_include_directories(injector-marker-target PRIVATE include)

  add_executable(injector-inherited-handle-launcher tests/inherited_handle_launcher.cpp)
  target_compile_features(injector-inherited-handle-launcher PRIVATE cxx_std_20)
  target_compile_definitions(injector-inherited-handle-launcher PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
  )
  target_compile_options(injector-inherited-handle-launcher PRIVATE
    /W4 /WX /permissive- /utf-8 /EHsc /guard:cf
  )
  target_link_options(injector-inherited-handle-launcher PRIVATE ${_injector_link_options})

  add_library(injector-marker-dll SHARED tests/marker_dll.cpp)
  target_compile_features(injector-marker-dll PRIVATE cxx_std_20)
  target_compile_definitions(injector-marker-dll PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
  )
  target_compile_options(injector-marker-dll PRIVATE /W4 /WX /permissive- /utf-8 /EHsc)
  if(CMAKE_SIZEOF_VOID_P EQUAL 8)
    set(_marker_module_name "MacTypeInjectorMarker64")
  else()
    set(_marker_module_name "MacTypeInjectorMarker32")
  endif()
  set_target_properties(injector-marker-dll PROPERTIES
    PREFIX ""
    OUTPUT_NAME "${_marker_module_name}"
  )

  add_library(injector-slow-marker-dll SHARED tests/marker_dll.cpp)
  target_compile_features(injector-slow-marker-dll PRIVATE cxx_std_20)
  target_compile_definitions(injector-slow-marker-dll PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX MACTYPE_MARKER_DELAY_MS=250
  )
  target_compile_options(injector-slow-marker-dll PRIVATE
    /W4 /WX /permissive- /utf-8 /EHsc
  )
  set_target_properties(injector-slow-marker-dll PROPERTIES
    PREFIX ""
    OUTPUT_NAME "${_marker_module_name}Slow"
  )

  add_library(injector-decoy-marker-dll SHARED tests/marker_dll.cpp)
  target_compile_features(injector-decoy-marker-dll PRIVATE cxx_std_20)
  target_compile_definitions(injector-decoy-marker-dll PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
  )
  target_compile_options(injector-decoy-marker-dll PRIVATE
    /W4 /WX /permissive- /utf-8 /EHsc
  )
  set_target_properties(injector-decoy-marker-dll PROPERTIES
    PREFIX ""
    OUTPUT_NAME "${_marker_module_name}"
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/decoy"
  )
  foreach(_configuration DEBUG RELEASE RELWITHDEBINFO MINSIZEREL)
    set_target_properties(injector-decoy-marker-dll PROPERTIES
      RUNTIME_OUTPUT_DIRECTORY_${_configuration} "${CMAKE_CURRENT_BINARY_DIR}/decoy/${_configuration}"
    )
  endforeach()
  add_executable(injector-integration-helper
    ${_injector_sources}
    src/main.cpp
  )
  target_compile_features(injector-integration-helper PRIVATE cxx_std_20)
  target_compile_definitions(injector-integration-helper PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
    MACTYPE_FIXED_MODULE_BASENAME=L"${_marker_module_name}.dll"
    MACTYPE_FIXED_MODULE_UTF8="${_marker_module_name}.dll"
  )
  target_compile_options(injector-integration-helper PRIVATE
    /W4 /WX /permissive- /utf-8 /EHsc /guard:cf
  )
  target_link_options(injector-integration-helper PRIVATE ${_injector_link_options})
  target_include_directories(injector-integration-helper PRIVATE include)
  set_target_properties(injector-integration-helper PROPERTIES
    OUTPUT_NAME "mactype-injector-integration"
  )

  add_executable(injector-timeout-helper
    ${_injector_sources}
    src/main.cpp
  )
  target_compile_features(injector-timeout-helper PRIVATE cxx_std_20)
  target_compile_definitions(injector-timeout-helper PRIVATE
    UNICODE _UNICODE WIN32_LEAN_AND_MEAN NOMINMAX
    MACTYPE_FIXED_MODULE_BASENAME=L"${_marker_module_name}.dll"
    MACTYPE_FIXED_MODULE_UTF8="${_marker_module_name}.dll"
    MACTYPE_REMOTE_TIMEOUT_MS=100
  )
  target_compile_options(injector-timeout-helper PRIVATE
    /W4 /WX /permissive- /utf-8 /EHsc /guard:cf
  )
  target_link_options(injector-timeout-helper PRIVATE ${_injector_link_options})
  target_include_directories(injector-timeout-helper PRIVATE include)
  set_target_properties(injector-timeout-helper PROPERTIES
    OUTPUT_NAME "mactype-injector-timeout"
  )

  add_test(
    NAME injector-loads-fixed-adjacent-module
    COMMAND powershell -NoProfile -ExecutionPolicy Bypass
      -File "${CMAKE_CURRENT_SOURCE_DIR}/tests/integration_test.ps1"
      -Injector "$<TARGET_FILE:injector-integration-helper>"
      -Target "$<TARGET_FILE:injector-marker-target>"
      -Module "$<TARGET_FILE:injector-marker-dll>"
      -SlowModule "$<TARGET_FILE:injector-slow-marker-dll>"
      -DecoyModule "$<TARGET_FILE:injector-decoy-marker-dll>"
      -TimeoutInjector "$<TARGET_FILE:injector-timeout-helper>"
      -InheritedHandleLauncher "$<TARGET_FILE:injector-inherited-handle-launcher>"
      -Architecture "$<IF:$<EQUAL:${CMAKE_SIZEOF_VOID_P},8>,x64,x86>"
  )
  set_tests_properties(injector-loads-fixed-adjacent-module PROPERTIES RUN_SERIAL TRUE)
endif()
