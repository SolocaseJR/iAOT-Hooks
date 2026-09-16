#include <IAOT/HookRegistry.hpp>
#include <IAOT/Logging.hpp>
#include <IAOT/Runtime.hpp>
#include <IAOT/SegmentFinder.hpp>
#include <atomic>
#include <cstdio>
#include <mutex>
#if __has_include("IAOTGeneratedRuntime.hpp")
#include "IAOTGeneratedRuntime.hpp"
#endif
#ifndef IAOT_CONFIG_SEGMENT_NAME
#define IAOT_CONFIG_SEGMENT_NAME "__IAOTCFG"
#endif

#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#include <dlfcn.h>
#include <dispatch/dispatch.h>
#include <mach-o/dyld.h>

namespace iaot {
namespace {
HookContext g_context{};
std::once_flag g_once;
std::atomic_bool g_initialized{false};
char g_bundleId[256]{};
char g_executableName[256]{};

RuntimeConfig *RegisterConfigForImage(const mach_header_64 *header,
                                      std::intptr_t slide) {
  std::size_t size = 0;
  // Generated from the same setting the patcher uses to write this segment.
  auto *config = static_cast<RuntimeConfig *>(
      FindSegmentData(header, OBF(IAOT_CONFIG_SEGMENT_NAME), slide, &size));
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
  g_initialized.store(true, std::memory_order_release);
  Log(ok ? OBF("IAOT initialized")
         : OBF("IAOT initialized with isolated hook failures"));
}
} // namespace

bool Initialize() {
  // Do not consume call_once when the loader has not supplied a main image.
  if (_dyld_image_count() == 0 || !_dyld_get_image_header(0))
    return false;
  std::call_once(g_once, InitializeOnce);
  return g_initialized.load(std::memory_order_acquire) &&
         AreRegisteredHooksReady();
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

namespace {
std::atomic_bool g_failedHookRetryScheduled{false};

void RetryHooksForAddedImage(const mach_header *, std::intptr_t) {
  if (!iaot::IsInitialized())
    return;
  if (g_failedHookRetryScheduled.exchange(true, std::memory_order_acq_rel))
    return;
  dispatch_async(dispatch_get_main_queue(), ^{
    g_failedHookRetryScheduled.store(false, std::memory_order_release);
    iaot::RetryFailedHooks(iaot::GetContext());
  });
}
} // namespace

#ifndef IAOT_RUNTIME_AUTOSTART
#define IAOT_RUNTIME_AUTOSTART 1
#endif

#if IAOT_RUNTIME_AUTOSTART
__attribute__((constructor)) static void IAOT_Constructor() {
  // Hooks install immediately at image load. Static bridges and Objective-C
  // method replacement do not require the application lifecycle, and delaying
  // them can let early callers cache state before instrumentation is active.
  // Keep the preflight and callback publication in one constructor.  Separate
  // constructors in different object files have no portable relative-order
  // guarantee, which used to leave the identity guard's position ambiguous.
  using EarlyPreflight = void (*)();
  const auto earlyPreflight = reinterpret_cast<EarlyPreflight>(
      dlsym(RTLD_DEFAULT, OBF("IAOT_RunEarlyPreflight")));
  if (earlyPreflight)
    earlyPreflight();
  iaot::Initialize();

  // ObjC classes supplied by optional frameworks may not exist during this
  // dylib's constructor. dyld invokes this for every subsequently loaded image;
  // coalescing onto the main queue gives failed class hooks another chance
  // without ever reinstalling hooks that are already Ready.
  _dyld_register_func_for_add_image(RetryHooksForAddedImage);

  // The overlay is the only part that needs a live UIScene, and it handles that
  // itself -- StartOverlay falls back to UISceneDidActivate/DidBecomeActive
  // observers plus its own short retry when no foreground scene exists yet, so
  // the menu simply appears once the scene is up.
  //
  // One retry pass remains necessary: an ObjC hook whose class lives in a
  // framework dyld has not registered yet cannot install at constructor time.
  // Queue it immediately (rather than holding a fixed startup window); only
  // Failed entries are retried, so Ready hooks are never touched.
  RetryHooksForAddedImage(nullptr, 0);
}
#endif
#endif
