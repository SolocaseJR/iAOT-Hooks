#include <IAOT/HookRegistry.hpp>
#include <cassert>
static int order = 0;
static bool One(const iaot::HookContext &) {
  order = 1;
  return true;
}
static bool Two(const iaot::HookContext &) {
  if (order == 1)
    order = 2;
  return true;
}
int main() {
  iaot::detail::ResetHookRegistryForTesting();
  assert(iaot::RegisterHook({"one", One}));
  assert(iaot::RegisterHook({"two", Two}));
  assert(!iaot::RegisterHook({"one", One}));
  assert(iaot::GetRegisteredHookCount() == 2);
  assert(iaot::InstallAllHooks({}));
  assert(order == 2);
  return 0;
}
