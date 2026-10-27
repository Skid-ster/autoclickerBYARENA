cmake_minimum_required(VERSION 3.24)

project(AutoClickerEngineA LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# x64 is the only supported architecture for this suite.
if(CMAKE_SIZEOF_VOID_P EQUAL 4)
    message(FATAL_ERROR "Engine A requires a 64-bit (x64) toolchain.")
endif()

# windows.h hygiene, following Microsoft's own "Using the Windows Headers"
# guidance and the convention used by the DirectX/Chromium/LLVM codebases.
# NOMINMAX prevents windows.h from defining min/max as preprocessor macros,
# which otherwise corrupt std::min/std::max/std::numeric_limits and trigger
# MSVC C2589/C4003. WIN32_LEAN_AND_MEAN excludes unused legacy header surface
# (Shell, OLE, Winsock, cryptography...) for faster, collision-resistant builds.
# Directory-scope definitions apply to every target including imgui_static.
if(WIN32)
    add_compile_definitions(NOMINMAX WIN32_LEAN_AND_MEAN)
endif()

if(MSVC)
    # C4324 only reports trailing padding from the deliberate cache-line
    # alignas() annotations in telemetry.h (prevents CPU false sharing).
    add_compile_options(/wd4324)
endif()

include(FetchContent)
FetchContent_Declare(
    imgui
    GIT_REPOSITORY https://github.com/ocornut/imgui.git
    GIT_TAG v1.90.4
    GIT_SHALLOW TRUE
)
FetchContent_MakeAvailable(imgui)

add_library(imgui_static STATIC
    ${imgui_SOURCE_DIR}/imgui.cpp
    ${imgui_SOURCE_DIR}/imgui_draw.cpp
    ${imgui_SOURCE_DIR}/imgui_tables.cpp
    ${imgui_SOURCE_DIR}/imgui_widgets.cpp
    ${imgui_SOURCE_DIR}/backends/imgui_impl_win32.cpp
    ${imgui_SOURCE_DIR}/backends/imgui_impl_dx11.cpp
)
target_include_directories(imgui_static PUBLIC
    ${imgui_SOURCE_DIR}
    ${imgui_SOURCE_DIR}/backends
)
if(MSVC)
    target_compile_options(imgui_static PRIVATE /W3 /EHsc /std:c++20 /utf-8)
endif()

set(ENGINE_A_SOURCES
    src/main.cpp
    src/app.cpp
    src/click_engine.cpp
    src/humanize.cpp
    src/input_dispatcher.cpp
    src/telemetry.cpp
    src/theme.cpp
    src/timing.cpp
    src/window_target.cpp
)

add_executable(AutoClickerEngineA WIN32 ${ENGINE_A_SOURCES})
target_compile_definitions(AutoClickerEngineA PRIVATE UNICODE _UNICODE)
target_include_directories(AutoClickerEngineA PRIVATE src)
target_link_libraries(AutoClickerEngineA PRIVATE imgui_static)
target_link_libraries(AutoClickerEngineA PRIVATE
    d3d11
    dxgi
    dwmapi
    winmm
    user32
    gdi32
)

if(MSVC)
    target_compile_options(AutoClickerEngineA PRIVATE
        /W4
        /permissive-
        /EHsc
        /std:c++20
        /utf-8
        /Zc:__cplusplus
    )
    target_link_options(AutoClickerEngineA PRIVATE /SUBSYSTEM:WINDOWS)
else()
    target_compile_options(AutoClickerEngineA PRIVATE
        -Wall -Wextra -Wpedantic
    )
endif()

# Unit tests exercise only portable translation units so they can also run on
# non-Windows CI runners; the Win32 modules are guarded by _WIN32.
enable_testing()

add_executable(test_humanize tests/test_humanize.cpp src/humanize.cpp)
target_include_directories(test_humanize PRIVATE src)
if(MSVC)
    target_compile_options(test_humanize PRIVATE /W4 /permissive- /EHsc
        /std:c++20 /utf-8 /Zc:__cplusplus)
else()
    target_compile_options(test_humanize PRIVATE -Wall -Wextra -Wpedantic)
endif()
add_test(NAME test_humanize COMMAND test_humanize)

add_executable(test_ring_buffer tests/test_ring_buffer.cpp src/telemetry.cpp)
target_include_directories(test_ring_buffer PRIVATE src)
if(MSVC)
    target_compile_options(test_ring_buffer PRIVATE /W4 /permissive- /EHsc
        /std:c++20 /utf-8 /Zc:__cplusplus)
else()
    target_compile_options(test_ring_buffer PRIVATE -Wall -Wextra -Wpedantic)
endif()
add_test(NAME test_ring_buffer COMMAND test_ring_buffer)

add_executable(test_config tests/test_config.cpp)
target_include_directories(test_config PRIVATE src)
if(MSVC)
    target_compile_options(test_config PRIVATE /W4 /permissive- /EHsc
        /std:c++20 /utf-8 /Zc:__cplusplus)
else()
    target_compile_options(test_config PRIVATE -Wall -Wextra -Wpedantic)
endif()
add_test(NAME test_config COMMAND test_config)
