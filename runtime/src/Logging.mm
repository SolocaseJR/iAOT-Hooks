#include <IAOT/Logging.hpp>
#include <dlfcn.h>

namespace {
iaot::LogSink IAOTDefaultLogSink() noexcept {
  using Sink = void (*)(iaot::LogLevel, const char *) noexcept;
  static const auto sink = reinterpret_cast<Sink>(
      dlsym(RTLD_DEFAULT, OBF("IAOTProjectLogSink")));
  return sink;
}
} // namespace

// Optional runtime-resolved sink; projects that do not export it have no link
// dependency and continue using stdout plus any SetLogSink callback.
#define IAOT_DEFAULT_LOG_SINK IAOTDefaultLogSink()
#include "LoggingImplementation.hpp"
