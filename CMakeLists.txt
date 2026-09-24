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
  runtime/src/OverlayStub.cpp
  runtime/src/IL2CPPPortable.cpp
  runtime/src/PersistencePortable.cpp
)
target_include_directories(iaot_portable PUBLIC runtime/include)
add_executable(runtime_layout_tests tests/runtime_layout_tests.cpp)
add_executable(config_tests tests/config_tests.cpp)
add_executable(app_function_tests tests/app_function_tests.cpp)
add_executable(hook_registry_tests tests/hook_registry_tests.cpp)
add_executable(generated_api_tests tests/generated_api_tests.cpp)
add_executable(overlay_tests tests/overlay_tests.cpp)
add_executable(il2cpp_tests tests/il2cpp_tests.cpp)
add_executable(persistence_tests tests/persistence_tests.cpp)
add_executable(obf_lifetime_tests tests/obf_lifetime_tests.cpp)
target_compile_definitions(obf_lifetime_tests PRIVATE IAOT_ENABLE_OBF=1)
target_include_directories(generated_api_tests PRIVATE examples/OwnedDemoProject/generated)
foreach(t runtime_layout_tests config_tests app_function_tests hook_registry_tests generated_api_tests overlay_tests il2cpp_tests persistence_tests obf_lifetime_tests)
  target_link_libraries(${t} PRIVATE iaot_portable)
endforeach()
enable_testing()
add_test(NAME runtime_layout COMMAND runtime_layout_tests)
add_test(NAME config COMMAND config_tests)
add_test(NAME app_function COMMAND app_function_tests)
add_test(NAME hook_registry COMMAND hook_registry_tests)
add_test(NAME generated_api COMMAND generated_api_tests)
add_test(NAME overlay COMMAND overlay_tests)
add_test(NAME il2cpp COMMAND il2cpp_tests)
add_test(NAME persistence COMMAND persistence_tests)
add_test(NAME obf_lifetime COMMAND obf_lifetime_tests)
