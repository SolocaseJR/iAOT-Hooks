cmake_minimum_required(VERSION 3.16)
project(IAOTPortableTests LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
add_library(iaot_portable
  runtime/src/Config.cpp
  runtime/src/HookRegistry.cpp
  runtime/src/AppFunction.cpp
  runtime/src/FunctionHooks.cpp
  runtime/src/LoggingPortable.cpp
  runtime/src/PortableStubs.cpp
)
target_include_directories(iaot_portable PUBLIC runtime/include)
add_executable(runtime_layout_tests tests/runtime_layout_tests.cpp)
add_executable(config_tests tests/config_tests.cpp)
add_executable(app_function_tests tests/app_function_tests.cpp)
add_executable(hook_registry_tests tests/hook_registry_tests.cpp)
add_executable(generated_api_tests tests/generated_api_tests.cpp)
target_include_directories(generated_api_tests PRIVATE examples/OwnedDemoProject/generated)
foreach(t runtime_layout_tests config_tests app_function_tests hook_registry_tests generated_api_tests)
  target_link_libraries(${t} PRIVATE iaot_portable)
endforeach()
enable_testing()
add_test(NAME runtime_layout COMMAND runtime_layout_tests)
add_test(NAME config COMMAND config_tests)
add_test(NAME app_function COMMAND app_function_tests)
add_test(NAME hook_registry COMMAND hook_registry_tests)
add_test(NAME generated_api COMMAND generated_api_tests)
