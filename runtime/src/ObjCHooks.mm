#include <IAOT/Logging.hpp>
#include <IAOT/ObjCHooks.hpp>
#if defined(__APPLE__)
#import <objc/runtime.h>
namespace iaot {
static bool Replace(const char *c, const char *s, IMP r, IMP *o,
                    bool classMethod) {
  if (!c || !s || !r)
    return false;
  Class cls = objc_getClass(c);
  SEL sel = sel_registerName(s);
  if (!cls || !sel) {
    Log(OBF("Objective-C class or selector lookup failed"));
    return false;
  }
  Method m = classMethod ? class_getClassMethod(cls, sel)
                         : class_getInstanceMethod(cls, sel);
  if (!m) {
    Log(OBF("Objective-C method lookup failed"));
    return false;
  }
  IMP old = method_setImplementation(m, r);
  if (o)
    *o = old;
  return old != nullptr;
}
bool ReplaceInstanceMethod(const char *c, const char *s, IMP r, IMP *o) {
  return Replace(c, s, r, o, false);
}
bool ReplaceClassMethod(const char *c, const char *s, IMP r, IMP *o) {
  return Replace(c, s, r, o, true);
}
} // namespace iaot
#endif
