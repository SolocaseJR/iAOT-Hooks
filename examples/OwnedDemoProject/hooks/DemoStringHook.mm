#include "IAOTGeneratedConfig.hpp"
#include "IAOTGeneratedOriginals.hpp"
#include <IAOT/IAOT.hpp>
#include <string>
extern "C" std::string IAOT_DemoCallback() {
  if (IAOTGetFlag(IAOTFlag::override_return_string))
    return std::string(IAOTGetString(IAOTString::return_string_value));
  auto original = iaot::generated::Original_DemoReturnStringPatch();
  if (original.IsResolved())
    return original.Call();
  return std::string(OBF("fallback demo string"));
}
extern "C" bool IAOT_InstallDemoStringHook(const iaot::HookContext &ctx) {
  IAOT_UNUSED(ctx);
  // Generated registration connects demo_callback to IAOT_DemoCallback.
  iaot::Log(OBF("Demo string callback available"));
  return true;
}
