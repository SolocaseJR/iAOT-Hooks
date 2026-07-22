#include "IAOTGeneratedOriginals.hpp"
#include "UserFeatures.hpp"
#include <IAOT/IAOT.hpp>

// Replace this exact ABI with the ABI of the function you are authorized to
// instrument.
extern "C" int IAOT_ExampleHook(void *instance, int value) {
  auto original = iaot::generated::Original_ExampleHook();
  if (!Features::ExampleHookEnabled())
    return original.Call(instance, value);
  return 123;
}

extern "C" bool IAOT_InstallUserMenu(const iaot::HookContext &context);
extern "C" bool IAOT_InstallUserHooks(const iaot::HookContext &context) {
  return IAOT_InstallUserMenu(context);
}
