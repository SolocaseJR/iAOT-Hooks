#if !defined(__APPLE__)
#include <IAOT/Runtime.hpp>
#include <IAOT/SymbolResolver.hpp>
namespace iaot {
namespace {
HookContext context{};
}
bool Initialize() { return false; }
bool IsInitialized() { return false; }
const HookContext &GetContext() { return context; }
std::uintptr_t GetMainImageBase() { return context.mainImageBase; }
std::intptr_t GetMainImageSlide() { return context.mainImageSlide; }
const mach_header_64 *GetMainMachHeader() { return nullptr; }
const char *GetBundleId() { return OBF(""); }
const char *GetExecutableName() { return OBF(""); }
std::uintptr_t ResolveExportedSymbol(const char *) { return 0; }
std::uintptr_t ResolveImageSymbol(const mach_header_64 *, const char *) {
  return 0;
}
std::uintptr_t FindImageBaseByName(const char *) { return 0; }
} // namespace iaot
#endif
