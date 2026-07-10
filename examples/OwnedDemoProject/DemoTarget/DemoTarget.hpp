#pragma once
#include <string>
extern "C" std::string DemoReturningSomeString();
extern "C" void DemoAddProfileValue(void* profile, int value);
extern "C" int DemoGetProfileValue(void* profile);
