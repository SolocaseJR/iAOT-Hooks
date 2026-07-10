#include <IAOT/IAOT.hpp>
#include "IAOTGeneratedConfig.hpp"
#include <string>
using DemoReturningSomeStringFn = std::string (*)();
static DemoReturningSomeStringFn g_original = nullptr;
extern "C" std::string IAOT_DemoCallback(){
    if(IAOTGetFlag(IAOTFlag::override_return_string)) return std::string(IAOTGetString(IAOTString::return_string_value));
    if(g_original) return g_original();
    return std::string(OBF("fallback demo string"));
}
extern "C" bool IAOT_InstallDemoStringHook(const iaot::HookContext& ctx){
    IAOT_UNUSED(ctx);
    // The offline patcher redirects owned demo code to this already-compiled callback before signing.
    IAOTSetCallback(IAOTCallback::demo_callback,reinterpret_cast<std::uintptr_t>(&IAOT_DemoCallback));
    iaot::Log(OBF("Demo string callback registered"));
    return true;
}
