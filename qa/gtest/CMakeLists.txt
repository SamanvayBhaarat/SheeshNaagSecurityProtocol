cmake_minimum_required(VERSION 3.20)
project(sheshnag_qa LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

include(FetchContent)
FetchContent_Declare(
  googletest
  URL https://github.com/google/googletest/archive/refs/tags/v1.15.2.tar.gz
  URL_HASH SHA256=7b42b4d6ed48810c5362c265a17faebe90dc2373c885e5216439d37927f02926)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)

add_executable(sheshnag_tests
  test_support.cc
  baseline_test.cc
  detection_agents_test.cc
  detection_system_test.cc
  actions_test.cc
  cli_contract_test.cc)

target_compile_options(sheshnag_tests PRIVATE -Wall -Wextra -Werror)
target_compile_definitions(sheshnag_tests PRIVATE
  SHESHNAG_SCRIPT_PATH="${CMAKE_CURRENT_SOURCE_DIR}/../../Src/sheshnag.sh")
target_link_libraries(sheshnag_tests PRIVATE GTest::gtest_main GTest::gmock)

enable_testing()
include(GoogleTest)
gtest_discover_tests(sheshnag_tests DISCOVERY_TIMEOUT 30 PROPERTIES TIMEOUT 30 LABELS small)
