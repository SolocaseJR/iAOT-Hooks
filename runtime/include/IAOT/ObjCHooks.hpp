#pragma once
#if defined(__OBJC__)
#include <objc/runtime.h>
#else
using IMP = void (*)();
#endif
namespace iaot {
bool ReplaceInstanceMethod(const char *className, const char *selectorName,
                           IMP replacement, IMP *originalOut);
bool ReplaceClassMethod(const char *className, const char *selectorName,
                        IMP replacement, IMP *originalOut);
} // namespace iaot
