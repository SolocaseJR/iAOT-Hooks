// Shared by the Apple and portable translation units; compile exactly one.
#include <IAOT/Logging.hpp>
#include <atomic>
#include <cstdarg>
#include <cstdio>
#include <cstring>

namespace iaot {
namespace {
std::atomic<LogLevel> g_level{LogLevel::Info};
std::atomic<LogSink> g_sink{nullptr};
thread_local bool g_deliveringLog = false;
const char *Name(LogLevel level) {
  switch (level) {
  case LogLevel::Debug: return "DEBUG";
  case LogLevel::Info: return "INFO";
  case LogLevel::Warning: return "WARN";
  case LogLevel::Error: return "ERROR";
  case LogLevel::Off: return "OFF";
  }
  return "?";
}
} // namespace
void SetLogLevel(LogLevel level) noexcept { g_level.store(level); }
LogLevel GetLogLevel() noexcept { return g_level.load(); }
void SetLoggingEnabled(bool enabled) noexcept {
  SetLogLevel(enabled ? LogLevel::Info : LogLevel::Off);
}
bool IsLoggingEnabled() noexcept { return GetLogLevel() != LogLevel::Off; }
void SetLogSink(LogSink sink) noexcept { g_sink.store(sink); }
void LogMessage(LogLevel level, const char *message) noexcept {
  const auto threshold = GetLogLevel();
  if (!message || level == LogLevel::Off || level < threshold ||
      threshold == LogLevel::Off)
    return;
  std::printf("[IAOT][%s] %s\n", Name(level), message);
  auto sink = g_sink.load();
#if defined(IAOT_DEFAULT_LOG_SINK)
  if (!sink)
    sink = IAOT_DEFAULT_LOG_SINK;
#endif
  if (sink && !g_deliveringLog) {
    g_deliveringLog = true;
    sink(level, message);
    g_deliveringLog = false;
  }
}
void Log(const char *message) noexcept { LogMessage(LogLevel::Info, message); }
void Logf(const char *format, ...) noexcept {
  if (!format || GetLogLevel() > LogLevel::Info)
    return;
  char buffer[4096]{};
  std::va_list args;
  va_start(args, format);
  const int count = std::vsnprintf(buffer, sizeof(buffer), format, args);
  va_end(args);
  if (count < 0) {
    LogMessage(LogLevel::Error, "log formatting failed");
    return;
  }
  if (static_cast<std::size_t>(count) >= sizeof(buffer)) {
    constexpr char suffix[] = " [truncated]";
    std::memcpy(buffer + sizeof(buffer) - sizeof(suffix), suffix, sizeof(suffix));
  }
  LogMessage(LogLevel::Info, buffer);
}
} // namespace iaot
