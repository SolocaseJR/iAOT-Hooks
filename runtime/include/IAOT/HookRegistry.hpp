#pragma once
#include <IAOT/Runtime.hpp>
#include <cstddef>
namespace iaot {
using HookInstallFn = bool (*)(const HookContext& ctx);
struct HookEntry { const char* name; HookInstallFn install; };
IAOT_EXPORT bool RegisterHook(const HookEntry& entry);
IAOT_EXPORT bool InstallAllHooks(const HookContext& ctx);
IAOT_EXPORT std::size_t GetRegisteredHookCount();
IAOT_EXPORT void RegisterGeneratedHooks();
namespace detail { void ResetHookRegistryForTesting(); }
}
#define IAOT_DECLARE_HOOK(name) extern "C" bool name(const iaot::HookContext& ctx)
#define IAOT_REGISTER_HOOK(name, fn) namespace { struct IAOT_AutoRegister_##fn { IAOT_AutoRegister_##fn(){ iaot::RegisterHook({OBF(name), fn}); } } IAOT_AutoRegister_Instance_##fn; }
