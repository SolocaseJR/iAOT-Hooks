#pragma once
#include <IAOT/Types.hpp>
#include <cstddef>
#include <cstdint>

namespace iaot {
constexpr char kConfigMagic[8] = {'I','A','O','T','C','F','G','1'};
constexpr std::uint32_t kConfigVersion = 1;
constexpr std::uint32_t kMaxRuntimeString = 256;
constexpr std::uint32_t kMaxCallbackSlots = 64;
constexpr std::uint32_t kMaxFlags = 256;
constexpr std::uint32_t kMaxStrings = 32;
enum class ConfigValueKind : std::uint32_t { Invalid=0, I32=1, U32=2, I64=3, U64=4, F32=5, F64=6, Bool=7, FixedString=8, Pointer=9 };
struct RuntimeConfigHeader {
    char magic[8]; std::uint32_t version; std::uint32_t size;
    std::uint32_t flagsOffset; std::uint32_t flagsCount;
    std::uint32_t callbacksOffset; std::uint32_t callbacksCount;
    std::uint32_t stringsOffset; std::uint32_t stringsCount;
    std::uint32_t reserved[8];
};
struct RuntimeConfig {
    RuntimeConfigHeader header;
    std::uint32_t flags[kMaxFlags];
    std::uintptr_t callbacks[kMaxCallbackSlots];
    char strings[kMaxStrings][kMaxRuntimeString];
};
static_assert(sizeof(RuntimeConfigHeader) == 72, "stable IAOT header layout");
static_assert(offsetof(RuntimeConfig, flags) == 72, "stable flags offset");
static_assert(offsetof(RuntimeConfig, callbacks) == 1096, "stable callback offset");
#if UINTPTR_MAX == UINT64_MAX
static_assert(offsetof(RuntimeConfig, strings) == 1608, "stable string offset");
static_assert(sizeof(RuntimeConfig) == 9800, "stable 64-bit config size");
#endif
IAOT_EXPORT RuntimeConfig* GetConfig();
IAOT_EXPORT bool HasConfig();
IAOT_EXPORT bool ValidateConfig(const RuntimeConfig* cfg);
IAOT_EXPORT bool GetBool(std::uint32_t index); IAOT_EXPORT void SetBool(std::uint32_t index, bool value);
IAOT_EXPORT std::int32_t GetI32(std::uint32_t index); IAOT_EXPORT void SetI32(std::uint32_t index, std::int32_t value);
IAOT_EXPORT std::uint32_t GetU32(std::uint32_t index); IAOT_EXPORT void SetU32(std::uint32_t index, std::uint32_t value);
IAOT_EXPORT std::uint64_t GetU64(std::uint32_t index); IAOT_EXPORT void SetU64(std::uint32_t index, std::uint64_t value);
IAOT_EXPORT float GetF32(std::uint32_t index); IAOT_EXPORT void SetF32(std::uint32_t index, float value);
IAOT_EXPORT const char* GetString(std::uint32_t index); IAOT_EXPORT void SetString(std::uint32_t index, const char* value);
IAOT_EXPORT std::uintptr_t GetCallback(std::uint32_t index); IAOT_EXPORT void SetCallback(std::uint32_t index, std::uintptr_t value);
namespace detail { void SetRuntimeConfig(RuntimeConfig* cfg); void InitializeConfig(RuntimeConfig& cfg); }
}
