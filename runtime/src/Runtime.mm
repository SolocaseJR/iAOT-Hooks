#include <IAOT/Runtime.hpp>
#include <IAOT/HookRegistry.hpp>
#include <IAOT/Logging.hpp>
#include <IAOT/SegmentFinder.hpp>
#include <cstdio>
#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#include <mach-o/dyld.h>
namespace iaot {
namespace { HookContext g_context{}; bool g_initialized=false; char g_bundleId[256]{}; char g_executableName[256]{}; }
bool Initialize(){ if(g_initialized)return true; if(_dyld_image_count()==0){Log(OBF("IAOT could not locate the main image"));return false;} auto* h=reinterpret_cast<const mach_header_64*>(_dyld_get_image_header(0)); auto slide=_dyld_get_image_vmaddr_slide(0); if(!h){Log(OBF("IAOT main image header is unavailable"));return false;} @autoreleasepool { NSBundle* b=[NSBundle mainBundle]; NSString* executableKey=[NSString stringWithUTF8String:OBF("CFBundleExecutable")]; const char* bundle=[[b bundleIdentifier] UTF8String]; const char* executable=[[b objectForInfoDictionaryKey:executableKey] UTF8String]; if(bundle) std::snprintf(g_bundleId,sizeof(g_bundleId),"%s",bundle); if(executable) std::snprintf(g_executableName,sizeof(g_executableName),"%s",executable); g_context={reinterpret_cast<std::uintptr_t>(h),slide,h,g_bundleId,g_executableName,nullptr}; }
    std::size_t size{}; auto* cfg=static_cast<RuntimeConfig*>(FindSegmentData(h,OBF("__IAOTCFG"),slide,&size)); if(cfg&&size>=sizeof(RuntimeConfig)&&ValidateConfig(cfg)){g_context.config=cfg;detail::SetRuntimeConfig(cfg);} else Log(OBF("IAOT config segment absent or invalid; safe defaults will be used"));
    RegisterGeneratedHooks(); g_initialized=true; bool ok=InstallAllHooks(g_context); Log(ok?OBF("IAOT initialized"):OBF("IAOT initialized with hook failures")); return ok;
}
bool IsInitialized(){return g_initialized;} const HookContext& GetContext(){return g_context;} std::uintptr_t GetMainImageBase(){return g_context.mainImageBase;} std::intptr_t GetMainImageSlide(){return g_context.mainImageSlide;} const mach_header_64* GetMainMachHeader(){return g_context.mainHeader;} const char* GetBundleId(){return g_context.bundleId?g_context.bundleId:"";} const char* GetExecutableName(){return g_context.executableName?g_context.executableName:"";}
}
__attribute__((constructor)) static void IAOT_Constructor(){iaot::Initialize();}
#endif
