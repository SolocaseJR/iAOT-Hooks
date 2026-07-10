#include "DemoTarget.hpp"
#include <IAOT/Types.hpp>
extern "C" std::string DemoReturningSomeString(){return std::string(OBF("original owned-demo string"));}
extern "C" void DemoAddProfileValue(void* p,int v){if(p)*static_cast<int*>(p)+=v;}
extern "C" int DemoGetProfileValue(void* p){return p?*static_cast<int*>(p):0;}
