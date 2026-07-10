#include <IAOT/IAOT.hpp>
#include "IAOTGeneratedConfig.hpp"
#import <Foundation/Foundation.h>
static IMP g_originalGreeting = nullptr;
static NSString* IAOTGreeting(id self,SEL cmd){
    if(IAOTGetFlag(IAOTFlag::enable_objc_demo_hook)) return [NSString stringWithUTF8String:IAOTGetString(IAOTString::return_string_value)];
    if(g_originalGreeting){using Fn=NSString*(*)(id,SEL);return reinterpret_cast<Fn>(g_originalGreeting)(self,cmd);}
    return [NSString stringWithUTF8String:OBF("owned demo greeting")];
}
extern "C" bool IAOT_InstallDemoObjCHook(const iaot::HookContext& ctx){IAOT_UNUSED(ctx);return iaot::ReplaceInstanceMethod(OBF("IAOTDemoGreeter"),OBF("greetingText"),reinterpret_cast<IMP>(&IAOTGreeting),&g_originalGreeting);}
