#pragma once
#include <IAOT/Types.hpp>
namespace iaot {
IAOT_EXPORT void Log(const char* msg) noexcept;
IAOT_EXPORT void Logf(const char* fmt, ...) noexcept;
IAOT_EXPORT void SetLoggingEnabled(bool enabled) noexcept;
IAOT_EXPORT bool IsLoggingEnabled() noexcept;
}
