#include <IAOT/IAOT.hpp>
#include "IAOTGeneratedConfig.hpp"
#include "IAOTGeneratedFunctions.hpp"
extern "C" bool IAOT_InstallDemoStringHook(const iaot::HookContext& ctx);
extern "C" bool IAOT_InstallDemoObjCHook(const iaot::HookContext& ctx);
extern "C" bool IAOT_InstallDemoFunctionCaller(const iaot::HookContext& ctx);
extern "C" bool IAOT_InstallUserHooks(const iaot::HookContext& ctx);
namespace iaot { void RegisterGeneratedHooks() {
    RegisterHook({ OBF("DemoStringHook"), IAOT_InstallDemoStringHook });
    RegisterHook({ OBF("DemoObjCHook"), IAOT_InstallDemoObjCHook });
    RegisterHook({ OBF("DemoFunctionCaller"), IAOT_InstallDemoFunctionCaller });
    RegisterHook({ OBF("UserHooks"), IAOT_InstallUserHooks });
} }
