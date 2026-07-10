#include <IAOT/Config.hpp>
#include <cassert>
#include <cstddef>
int main(){ static_assert(sizeof(iaot::RuntimeConfigHeader)==72); assert(offsetof(iaot::RuntimeConfig,flags)==72); assert(offsetof(iaot::RuntimeConfig,callbacks)==1096); return 0; }
