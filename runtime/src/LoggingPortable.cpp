#if !defined(__APPLE__)
#include <IAOT/Logging.hpp>
#include <atomic>
#include <cstdarg>
#include <cstdio>
namespace iaot {
namespace { std::atomic_bool g_enabled{true}; }
void SetLoggingEnabled(bool value) noexcept { g_enabled.store(value); }
bool IsLoggingEnabled() noexcept { return g_enabled.load(); }
void Log(const char* msg) noexcept { if(g_enabled.load()&&msg) std::printf("[IAOT] %s\n",msg); }
void Logf(const char* fmt,...) noexcept { if(!g_enabled.load()||!fmt)return; std::printf("[IAOT] "); std::va_list args; va_start(args,fmt); std::vprintf(fmt,args); va_end(args); std::printf("\n"); }
}
#endif
