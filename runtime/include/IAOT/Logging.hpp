#pragma once
#include <IAOT/Types.hpp>
namespace iaot {
enum class LogLevel : std::uint32_t {
  Debug = 0,
  Info = 1,
  Warning = 2,
  Error = 3,
  Off = 4
};
IAOT_EXPORT void Log(const char *msg) noexcept;
IAOT_EXPORT void Logf(const char *fmt, ...) noexcept;
IAOT_EXPORT void LogMessage(LogLevel level, const char *msg) noexcept;
IAOT_EXPORT void SetLogLevel(LogLevel level) noexcept;
IAOT_EXPORT LogLevel GetLogLevel() noexcept;
IAOT_EXPORT void SetLoggingEnabled(bool enabled) noexcept;
IAOT_EXPORT bool IsLoggingEnabled() noexcept;
// Called synchronously after stdout output. The message is borrowed only for
// the duration of the call. Sinks must not throw or block waiting on UI work.
// Recursive logging is printed, but is not delivered to the sink again.
using LogSink = void (*)(LogLevel level, const char *message) noexcept;
IAOT_EXPORT void SetLogSink(LogSink sink) noexcept;
} // namespace iaot
