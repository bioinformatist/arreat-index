cmake_minimum_required(VERSION 3.28)

project(ArreatIndexD2RLoaderPlugin VERSION 0.1.0 LANGUAGES NONE)

if(NOT CMAKE_SYSTEM_NAME STREQUAL "Windows")
    message(FATAL_ERROR "The Arreat Index D2RLoader plugin targets Windows x64 only.")
endif()

enable_language(CXX)
enable_language(RC)

if(NOT CMAKE_CXX_COMPILER_ID STREQUAL "MSVC" AND
   NOT (CMAKE_CXX_COMPILER_ID MATCHES "Clang" AND
        CMAKE_CXX_COMPILER_FRONTEND_VARIANT STREQUAL "MSVC"))
    message(FATAL_ERROR "Use MSVC or clang-cl to build the D2RLoader plugin.")
endif()

include(CheckCXXSourceCompiles)
check_cxx_source_compiles([=[
#if !defined(_M_X64)
#error The target is not Windows x64.
#endif
int main() { return 0; }
]=] ARREAT_INDEX_TARGET_IS_WINDOWS_X64)
if(NOT ARREAT_INDEX_TARGET_IS_WINDOWS_X64)
    message(FATAL_ERROR "The Arreat Index D2RLoader plugin requires an x64 target.")
endif()

include(FetchContent)
set(D2RLPLUGIN_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(D2RLPLUGIN_BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(
    D2RLPluginSDK
    GIT_REPOSITORY "https://github.com/D2RLoader/PluginSDK.git"
    GIT_TAG 4933e2c42cb2592958cd0df3b6dc5003102252d1
    GIT_SUBMODULES ""
    EXCLUDE_FROM_ALL
)
FetchContent_MakeAvailable(D2RLPluginSDK)

add_library(ArreatIndexD2RL SHARED
    src/observation.cpp
    src/plugin.cpp
    src/plugin.rc
)
target_include_directories(ArreatIndexD2RL PRIVATE src)
target_link_libraries(ArreatIndexD2RL PRIVATE D2RLPlugin::D2RLPlugin)
set_target_properties(ArreatIndexD2RL PROPERTIES
    CXX_STANDARD 20
    CXX_STANDARD_REQUIRED TRUE
    CXX_EXTENSIONS FALSE
    OUTPUT_NAME "d2rl-arreat-index"
)

add_executable(ArreatIndexObservationTests
    src/observation.cpp
    tests/observation_test.cpp
)
target_include_directories(ArreatIndexObservationTests PRIVATE src)
set_target_properties(ArreatIndexObservationTests PROPERTIES
    CXX_STANDARD 20
    CXX_STANDARD_REQUIRED TRUE
    CXX_EXTENSIONS FALSE
)

    target_compile_options(ArreatIndexD2RL PRIVATE
        "$<$<COMPILE_LANGUAGE:CXX>:/W4>"
        "$<$<COMPILE_LANGUAGE:CXX>:/WX>"
        "$<$<COMPILE_LANGUAGE:CXX>:/permissive->"
    )
    target_compile_options(ArreatIndexObservationTests PRIVATE /W4 /WX /permissive-)

enable_testing()
add_test(NAME observation COMMAND ArreatIndexObservationTests)

install(
    FILES "$<TARGET_FILE:ArreatIndexD2RL>"
    DESTINATION d2rloader/plugins
)
install(
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/LICENSE.D2RLoader-PluginSDK.txt"
    DESTINATION licenses
)
