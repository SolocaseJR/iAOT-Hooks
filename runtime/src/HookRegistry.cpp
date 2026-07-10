#include <IAOT/HookRegistry.hpp>
#include <IAOT/Logging.hpp>
#include <array>
#include <cstring>

namespace iaot {
namespace { constexpr std::size_t kMaxHooks = 128; std::array<HookEntry,kMaxHooks> g_hooks{}; std::size_t g_count{}; }
bool RegisterHook(const HookEntry& entry) {
    if (!entry.name || !entry.install || g_count >= g_hooks.size()) { Log(OBF("IAOT hook registration rejected")); return false; }
    for (std::size_t i=0;i<g_count;++i) if (std::strcmp(g_hooks[i].name, entry.name)==0) { Log(OBF("IAOT duplicate hook name rejected")); return false; }
    g_hooks[g_count++] = entry; return true;
}
bool InstallAllHooks(const HookContext& ctx) {
    bool ok = true;
    for (std::size_t i=0;i<g_count;++i) { if (!g_hooks[i].install(ctx)) { Logf("IAOT hook failed: %s", g_hooks[i].name); ok = false; } }
    return ok;
}
std::size_t GetRegisteredHookCount() { return g_count; }
namespace detail { void ResetHookRegistryForTesting(){ g_count=0; g_hooks.fill({}); } }
}
