#include <IAOT/HookRegistry.hpp>
#include <IAOT/Logging.hpp>
#include <IAOT/Runtime.hpp>
#include <IAOT/SegmentFinder.hpp>
#include <atomic>
#include <cstdio>
#include <mutex>

#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#include <mach-o/dyld.h>

namespace iaot {
namespace {
HookContext g_context{};
std::once_flag g_once;
std::atomic_bool g_initialized{false};
std::atomic_bool g_initializeResult{false};
char g_bundleId[256]{};
char g_executableName[256]{};

RuntimeConfig *RegisterConfigForImage(const mach_header_64 *header,
                                      std::intptr_t slide) {
  std::size_t size = 0;
  auto *config = static_cast<RuntimeConfig *>(
      FindSegmentData(header, OBF("__IAOTCFG"), slide, &size));
  if (config && size >= sizeof(RuntimeConfig) && ValidateConfig(config)) {
    detail::AddRuntimeConfig(config, slide);
    return config;
  }
  return nullptr;
}

void ImageAdded(const mach_header *header, std::intptr_t slide) {
  RegisterConfigForImage(reinterpret_cast<const mach_header_64 *>(header),
                         slide);
}

RuntimeConfig *FindRuntimeConfigs() {
  RuntimeConfig *primary = nullptr;
  for (std::uint32_t index = 0; index < _dyld_image_count(); ++index) {
    auto *config = RegisterConfigForImage(
        reinterpret_cast<const mach_header_64 *>(_dyld_get_image_header(index)),
        _dyld_get_image_vmaddr_slide(index));
    if (!primary && config)
      primary = config;
  }
  return primary;
}

void InitializeOnce() {
  if (_dyld_image_count() == 0) {
    Log(OBF("IAOT could not locate the main image"));
    return;
  }
  const auto *header =
      reinterpret_cast<const mach_header_64 *>(_dyld_get_image_header(0));
  const auto slide = _dyld_get_image_vmaddr_slide(0);
  if (!header) {
    Log(OBF("IAOT main image header is unavailable"));
    return;
  }
  @autoreleasepool {
    NSBundle *bundle = [NSBundle mainBundle];
    NSString *executableKey =
        [NSString stringWithUTF8String:OBF("CFBundleExecutable")];
    const char *bundleId = [[bundle bundleIdentifier] UTF8String];
    const char *executable =
        [[bundle objectForInfoDictionaryKey:executableKey] UTF8String];
    if (bundleId)
      std::snprintf(g_bundleId, sizeof(g_bundleId), "%s", bundleId);
    if (executable)
      std::snprintf(g_executableName, sizeof(g_executableName), "%s",
                    executable);
  }
  g_context = {reinterpret_cast<std::uintptr_t>(header),
               slide,
               header,
               g_bundleId,
               g_executableName,
               FindRuntimeConfigs()};
  _dyld_register_func_for_add_image(ImageAdded);
  if (!g_context.config)
    Log(OBF("IAOT config segment absent or invalid; static bridges will pass "
            "through safely"));
  RegisterGeneratedHooks();
  const bool ok = InstallAllHooks(g_context);
  g_initializeResult.store(ok, std::memory_order_release);
  g_initialized.store(true, std::memory_order_release);
  Log(ok ? OBF("IAOT initialized")
         : OBF("IAOT initialized with isolated hook failures"));
}
} // namespace

bool Initialize() {
  std::call_once(g_once, InitializeOnce);
  return g_initialized.load(std::memory_order_acquire) &&
         g_initializeResult.load(std::memory_order_acquire);
}

bool IsInitialized() { return g_initialized.load(std::memory_order_acquire); }
const HookContext &GetContext() { return g_context; }
std::uintptr_t GetMainImageBase() { return g_context.mainImageBase; }
std::intptr_t GetMainImageSlide() { return g_context.mainImageSlide; }
const mach_header_64 *GetMainMachHeader() { return g_context.mainHeader; }
const char *GetBundleId() {
  return g_context.bundleId ? g_context.bundleId : OBF("");
}
const char *GetExecutableName() {
  return g_context.executableName ? g_context.executableName : OBF("");
}
} // namespace iaot

__attribute__((constructor)) static void IAOT_Constructor() {
  iaot::Initialize();
}
#endif
