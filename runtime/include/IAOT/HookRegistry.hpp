#pragma once
#include <IAOT/Runtime.hpp>
#include <cstddef>
namespace iaot {
using HookInstallFn = bool (*)(const HookContext &ctx);
struct HookEntry {
  const char *name;
  HookInstallFn install;
};
enum class HookStatus : std::uint32_t {
  Registered = 0,
  Installing = 1,
  Ready = 2,
  Disabled = 3,
  Failed = 4
};
struct HookDiagnostic {
  const char *name;
  HookStatus status;
};
IAOT_EXPORT bool RegisterHook(const HookEntry &entry);
IAOT_EXPORT bool InstallAllHooks(const HookContext &ctx);
IAOT_EXPORT std::size_t GetRegisteredHookCount();
IAOT_EXPORT HookDiagnostic GetHookDiagnostic(std::size_t index);
IAOT_EXPORT void RegisterGeneratedHooks();
namespace detail {
void ResetHookRegistryForTesting();
}
} // namespace iaot
#define IAOT_DECLARE_HOOK(name)                                                \
  extern "C" bool name(const iaot::HookContext &ctx)
#define IAOT_REGISTER_HOOK(name, fn)                                           \
  namespace {                                                                  \
  struct IAOT_AutoRegister_##fn {                                              \
    IAOT_AutoRegister_##fn() { iaot::RegisterHook({OBF(name), fn}); }          \
  } IAOT_AutoRegister_Instance_##fn;                                           \
  }
