#pragma once
#include <IAOT/IAOT.hpp>
#include <string>
namespace iaot::generated {
inline auto ReturningSomeString(){return iaot::AppFunction<std::string()>::FromSymbol(OBF("DemoReturningSomeString"));}
inline auto AddProfileValue(){return iaot::AppFunction<void(void*, int)>::FromVmAddr(0x100ABCDEFULL);}
inline auto GetProfileValue(){return iaot::AppFunction<int(void*)>::FromRva(0x123456ULL);}
}
