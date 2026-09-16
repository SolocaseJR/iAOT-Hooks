#include <IAOT/HookRegistry.hpp>
#include <IAOT/Logging.hpp>
#include <array>
#include <cstring>
#include <mutex>

namespace iaot {
namespace {
constexpr std::size_t kMaxHooks = 128;
std::array<HookEntry, kMaxHooks> g_hooks{};
std::array<HookStatus, kMaxHooks> g_status{};
std::size_t g_count{};
std::mutex g_registryMutex;
} // namespace
bool RegisterHook(const HookEntry &entry) {
  std::lock_guard<std::mutex> lock(g_registryMutex);
  if (!entry.name || !entry.install || g_count >= g_hooks.size()) {
    Log(OBF("IAOT hook registration rejected"));
    return false;
  }
  for (std::size_t i = 0; i < g_count; ++i)
    if (std::strcmp(g_hooks[i].name, entry.name) == 0) {
      Log(OBF("IAOT duplicate hook name rejected"));
      return false;
    }
  g_hooks[g_count] = entry;
  g_status[g_count] = HookStatus::Registered;
  ++g_count;
  return true;
}
bool InstallAllHooks(const HookContext &ctx) {
  bool ok = true;
  std::size_t count = 0;
  {
    std::lock_guard<std::mutex> lock(g_registryMutex);
    count = g_count;
  }
  for (std::size_t i = 0; i < count; ++i) {
    HookEntry entry{};
    {
      std::lock_guard<std::mutex> lock(g_registryMutex);
      g_status[i] = HookStatus::Installing;
      entry = g_hooks[i];
    }
    const bool installed = entry.install(ctx);
    {
      std::lock_guard<std::mutex> lock(g_registryMutex);
      g_status[i] = installed ? HookStatus::Ready : HookStatus::Failed;
    }
    if (!installed) {
      Logf("IAOT hook failed and was isolated: %s", entry.name);
      ok = false;
    }
  }
  return ok;
}
bool AreRegisteredHooksReady() {
  std::lock_guard<std::mutex> lock(g_registryMutex);
  for (std::size_t i = 0; i < g_count; ++i)
    if (g_status[i] != HookStatus::Ready && g_status[i] != HookStatus::Disabled)
      return false;
  return true;
}
bool RetryFailedHooks(const HookContext &ctx) {
  bool ok = true;
  std::size_t count = 0;
  {
    std::lock_guard<std::mutex> lock(g_registryMutex);
    count = g_count;
  }
  for (std::size_t i = 0; i < count; ++i) {
    HookEntry entry{};
    {
      std::lock_guard<std::mutex> lock(g_registryMutex);
      // Only Failed entries are retried. Skipping Ready ones is what makes this
      // safe to call repeatedly: a hook that captured the original IMP would
      // otherwise capture its own replacement and recurse.
      if (g_status[i] != HookStatus::Failed)
        continue;
      g_status[i] = HookStatus::Installing;
      entry = g_hooks[i];
    }
    const bool installed = entry.install(ctx);
    {
      std::lock_guard<std::mutex> lock(g_registryMutex);
      g_status[i] = installed ? HookStatus::Ready : HookStatus::Failed;
    }
    if (installed) {
      Logf("IAOT hook installed on retry: %s", entry.name);
    } else {
      Logf("IAOT hook still failing after retry: %s", entry.name);
      ok = false;
    }
  }
  return ok;
}
std::size_t GetRegisteredHookCount() {
  std::lock_guard<std::mutex> lock(g_registryMutex);
  return g_count;
}
HookDiagnostic GetHookDiagnostic(std::size_t index) {
  std::lock_guard<std::mutex> lock(g_registryMutex);
  return index < g_count ? HookDiagnostic{g_hooks[index].name, g_status[index]}
                         : HookDiagnostic{nullptr, HookStatus::Disabled};
}
namespace detail {
void ResetHookRegistryForTesting() {
  std::lock_guard<std::mutex> lock(g_registryMutex);
  g_count = 0;
  g_hooks.fill({});
  g_status.fill(HookStatus::Registered);
}
} // namespace detail
} // namespace iaot
