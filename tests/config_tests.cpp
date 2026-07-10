#include <IAOT/Config.hpp>
#include <cassert>
#include <cstring>
int main(){ iaot::RuntimeConfig cfg{}; iaot::detail::InitializeConfig(cfg); assert(iaot::ValidateConfig(&cfg)); iaot::detail::SetRuntimeConfig(&cfg); iaot::SetBool(0,true); assert(iaot::GetBool(0)); iaot::SetI32(1,-7); assert(iaot::GetI32(1)==-7); iaot::SetU32(2,42); assert(iaot::GetU32(2)==42); iaot::SetU64(4,0x1122334455667788ULL); assert(iaot::GetU64(4)==0x1122334455667788ULL); iaot::SetF32(6,1.5f); assert(iaot::GetF32(6)==1.5f); iaot::SetString(0,"demo"); assert(std::strcmp(iaot::GetString(0),"demo")==0); iaot::SetCallback(0,123); assert(iaot::GetCallback(0)==123); assert(!iaot::GetBool(999)); cfg.header.version=99; assert(!iaot::ValidateConfig(&cfg)); return 0; }
