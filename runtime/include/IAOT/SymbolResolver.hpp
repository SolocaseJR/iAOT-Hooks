#pragma once
#include <cstdint>
#if !defined(__APPLE__)
struct mach_header_64;
#endif
namespace iaot {
std::uintptr_t ResolveExportedSymbol(const char* symbolName);
std::uintptr_t ResolveImageSymbol(const mach_header_64* header, const char* symbolName);
std::uintptr_t FindImageBaseByName(const char* imageName);
}
