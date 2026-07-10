#include <IAOT/IAOT.hpp>
// Put authorized app-specific hook installs here. Every native behavior must be compiled before signing.
extern "C" bool IAOT_InstallUserHooks(const iaot::HookContext& ctx){IAOT_UNUSED(ctx);return true;}
