#include "IAOTGeneratedConfig.hpp"
#include "IAOTGeneratedFunctions.hpp"
#include <IAOT/AppFunction.hpp>
#include <string>
#include <type_traits>
int main(){auto generated=iaot::generated::ReturningSomeString();static_assert(std::is_same_v<decltype(generated),iaot::AppFunction<std::string()>>);auto manual=iaot::AppFunction<int(void*)>::FromVmAddr(0x100123456);IAOT_UNUSED(manual);return 0;}
