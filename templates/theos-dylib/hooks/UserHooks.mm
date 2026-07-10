#include <IAOT/IAOT.hpp>
// Put authorized app-specific hook installs here. Compile them before signing.
extern "C" bool IAOT_InstallUserHooks(const iaot::HookContext& ctx){ IAOT_UNUSED(ctx); iaot::Log(OBF("User hook module loaded")); return true; }
