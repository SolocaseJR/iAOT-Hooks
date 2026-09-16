#pragma once
#include <IAOT/Config.hpp>
#include <cstdint>
#if defined(__APPLE__)
#include <mach-o/loader.h>
#else
struct mach_header_64;
#endif
namespace iaot {
struct HookContext {
  std::uintptr_t mainImageBase{};
  std::intptr_t mainImageSlide{};
  const mach_header_64 *mainHeader{};
  const char *bundleId{};
  const char *executableName{};
  RuntimeConfig *config{};
};
// Initial setup runs once; the return value reflects current registered
// installer health, including subsequent retries. It does not prove execution.
IAOT_EXPORT bool Initialize();
IAOT_EXPORT bool IsInitialized();
IAOT_EXPORT const HookContext &GetContext();
IAOT_EXPORT std::uintptr_t GetMainImageBase();
IAOT_EXPORT std::intptr_t GetMainImageSlide();
IAOT_EXPORT const mach_header_64 *GetMainMachHeader();
IAOT_EXPORT const char *GetBundleId();
IAOT_EXPORT const char *GetExecutableName();
} // namespace iaot
