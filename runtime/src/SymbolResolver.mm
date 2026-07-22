#include <IAOT/SymbolResolver.hpp>
#if defined(__APPLE__)
#include <cstring>
#include <dlfcn.h>
#include <mach-o/dyld.h>
namespace iaot {
std::uintptr_t ResolveExportedSymbol(const char *n) {
  return n ? reinterpret_cast<std::uintptr_t>(dlsym(RTLD_DEFAULT, n)) : 0;
}
std::uintptr_t ResolveImageSymbol(const mach_header_64 *, const char *n) {
  return ResolveExportedSymbol(n);
}
std::uintptr_t FindImageBaseByName(const char *n) {
  if (!n)
    return 0;
  for (std::uint32_t i = 0; i < _dyld_image_count(); ++i) {
    auto *p = _dyld_get_image_name(i);
    if (p && std::strstr(p, n))
      return reinterpret_cast<std::uintptr_t>(_dyld_get_image_header(i));
  }
  return 0;
}
} // namespace iaot
#else
namespace iaot {
std::uintptr_t ResolveExportedSymbol(const char *) { return 0; }
std::uintptr_t ResolveImageSymbol(const mach_header_64 *, const char *) {
  return 0;
}
std::uintptr_t FindImageBaseByName(const char *) { return 0; }
} // namespace iaot
#endif
