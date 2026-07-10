#include <IAOT/IAOT.hpp>
#include <IAOT/AppFunction.hpp>
#include "IAOTGeneratedFunctions.hpp"
#include <string>
extern "C" bool IAOT_InstallDemoFunctionCaller(const iaot::HookContext& ctx){IAOT_UNUSED(ctx);auto stringFn=iaot::generated::ReturningSomeString();std::string value=stringFn.Call();iaot::Log(OBF("Called ReturningSomeString from owned demo"));iaot::Log(value.c_str());return stringFn.IsResolved();}
