#include <IAOT/IAOT.hpp>
extern "C" bool IAOT_InstallUserHooks(const iaot::HookContext& ctx);
namespace iaot { void RegisterGeneratedHooks(){ RegisterHook({OBF("UserHooks"),IAOT_InstallUserHooks}); } }
