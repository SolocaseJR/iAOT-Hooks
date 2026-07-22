#include "IAOTGeneratedConfig.hpp"
#include "IAOTGeneratedFunctions.hpp"
#import <Foundation/Foundation.h>
#include <IAOT/IAOT.hpp>
extern "C" id IAOTGreeting(id self, SEL cmd) {
  if (IAOTGetFlag(IAOTFlag::enable_objc_demo_hook))
    return [NSString
        stringWithUTF8String:IAOTGetString(IAOTString::return_string_value)];
  auto original = iaot::generated::Original_ObjCMessageExample();
  if (original.Resolve())
    return original.Call(self, cmd);
  return [NSString stringWithUTF8String:OBF("owned demo greeting")];
}
