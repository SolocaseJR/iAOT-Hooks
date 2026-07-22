#include <IAOT/Logging.hpp>
#include <atomic>
#include <cstdarg>
#include <cstdio>
namespace iaot {
namespace {
std::atomic<LogLevel> g_level{LogLevel::Info};
const char *Name(LogLevel l) {
  switch (l) {
  case LogLevel::Debug:
    return "DEBUG";
  case LogLevel::Info:
    return "INFO";
  case LogLevel::Warning:
    return "WARN";
  case LogLevel::Error:
    return "ERROR";
  case LogLevel::Off:
    return "OFF";
  }
  return "?";
}
} // namespace
void SetLogLevel(LogLevel v) noexcept { g_level.store(v); }
LogLevel GetLogLevel() noexcept { return g_level.load(); }
void SetLoggingEnabled(bool v) noexcept {
  SetLogLevel(v ? LogLevel::Info : LogLevel::Off);
}
bool IsLoggingEnabled() noexcept { return GetLogLevel() != LogLevel::Off; }
void LogMessage(LogLevel level, const char *msg) noexcept {
  if (!msg || level < GetLogLevel() || GetLogLevel() == LogLevel::Off)
    return;
  std::printf("[IAOT][%s] %s\n", Name(level), msg);
}
void Log(const char *msg) noexcept { LogMessage(LogLevel::Info, msg); }
void Logf(const char *fmt, ...) noexcept {
  if (GetLogLevel() > LogLevel::Info || !fmt)
    return;
  std::va_list args;
  va_start(args, fmt);
  std::printf("[IAOT][INFO] ");
  std::vprintf(fmt, args);
  std::printf("\n");
  va_end(args);
}
} // namespace iaot
