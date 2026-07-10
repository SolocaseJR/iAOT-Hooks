#include <IAOT/Logging.hpp>
#include <atomic>
#include <cstdarg>
#include <cstdio>
namespace iaot {
namespace { std::atomic_bool g_enabled{true}; }
void SetLoggingEnabled(bool v) noexcept { g_enabled.store(v); }
bool IsLoggingEnabled() noexcept { return g_enabled.load(); }
void Log(const char* msg) noexcept { if (!g_enabled.load() || !msg) return; std::printf("[IAOT] %s\n", msg); }
void Logf(const char* fmt, ...) noexcept { if (!g_enabled.load() || !fmt) return; std::va_list args; va_start(args,fmt);
    std::printf("[IAOT] "); std::vprintf(fmt,args); std::printf("\n");
    va_end(args);
}
}
