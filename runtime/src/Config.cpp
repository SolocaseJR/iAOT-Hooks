#include <IAOT/Config.hpp>
#include <IAOT/Logging.hpp>
#include <algorithm>
#include <cstring>

namespace iaot {
namespace { RuntimeConfig* g_config = nullptr; const char* kEmptyString = OBF(""); }
namespace detail {
void SetRuntimeConfig(RuntimeConfig* cfg) { g_config = ValidateConfig(cfg) ? cfg : nullptr; }
void InitializeConfig(RuntimeConfig& cfg) {
    std::memset(&cfg, 0, sizeof(cfg));
    std::memcpy(cfg.header.magic, kConfigMagic, sizeof(kConfigMagic));
    cfg.header.version = kConfigVersion; cfg.header.size = sizeof(RuntimeConfig);
    cfg.header.flagsOffset = offsetof(RuntimeConfig, flags); cfg.header.flagsCount = kMaxFlags;
    cfg.header.callbacksOffset = offsetof(RuntimeConfig, callbacks); cfg.header.callbacksCount = kMaxCallbackSlots;
    cfg.header.stringsOffset = offsetof(RuntimeConfig, strings); cfg.header.stringsCount = kMaxStrings;
}
}
RuntimeConfig* GetConfig() { return g_config; }
bool HasConfig() { return g_config && ValidateConfig(g_config); }
bool ValidateConfig(const RuntimeConfig* cfg) {
    if (!cfg) return false;
    return std::memcmp(cfg->header.magic, kConfigMagic, sizeof(kConfigMagic)) == 0 &&
        cfg->header.version == kConfigVersion && cfg->header.size >= sizeof(RuntimeConfig) &&
        cfg->header.flagsOffset == offsetof(RuntimeConfig, flags) && cfg->header.flagsCount <= kMaxFlags &&
        cfg->header.callbacksOffset == offsetof(RuntimeConfig, callbacks) && cfg->header.callbacksCount <= kMaxCallbackSlots &&
        cfg->header.stringsOffset == offsetof(RuntimeConfig, strings) && cfg->header.stringsCount <= kMaxStrings;
}
static bool FlagIndex(std::uint32_t index, std::uint32_t width=1) { bool ok=HasConfig() && index < g_config->header.flagsCount && width <= g_config->header.flagsCount - index; if(!ok) Log(OBF("IAOT config flag access rejected")); return ok; }
static bool StringIndex(std::uint32_t index) { bool ok=HasConfig() && index < g_config->header.stringsCount; if(!ok) Log(OBF("IAOT config string access rejected")); return ok; }
static bool CallbackIndex(std::uint32_t index) { bool ok=HasConfig() && index < g_config->header.callbacksCount; if(!ok) Log(OBF("IAOT config callback access rejected")); return ok; }
bool GetBool(std::uint32_t i) { return FlagIndex(i) ? g_config->flags[i] != 0 : false; }
void SetBool(std::uint32_t i, bool v) { if (FlagIndex(i)) g_config->flags[i] = v ? 1u : 0u; }
std::int32_t GetI32(std::uint32_t i) { return FlagIndex(i) ? static_cast<std::int32_t>(g_config->flags[i]) : 0; }
void SetI32(std::uint32_t i, std::int32_t v) { if (FlagIndex(i)) g_config->flags[i] = static_cast<std::uint32_t>(v); }
std::uint32_t GetU32(std::uint32_t i) { return FlagIndex(i) ? g_config->flags[i] : 0; }
void SetU32(std::uint32_t i, std::uint32_t v) { if (FlagIndex(i)) g_config->flags[i] = v; }
std::uint64_t GetU64(std::uint32_t i) { std::uint64_t v{}; if (FlagIndex(i,2)) std::memcpy(&v, &g_config->flags[i], sizeof(v)); return v; }
void SetU64(std::uint32_t i, std::uint64_t v) { if (FlagIndex(i,2)) std::memcpy(&g_config->flags[i], &v, sizeof(v)); }
float GetF32(std::uint32_t i) { float v{}; if (FlagIndex(i)) std::memcpy(&v, &g_config->flags[i], sizeof(v)); return v; }
void SetF32(std::uint32_t i, float v) { if (FlagIndex(i)) std::memcpy(&g_config->flags[i], &v, sizeof(v)); }
const char* GetString(std::uint32_t i) { return StringIndex(i) ? g_config->strings[i] : kEmptyString; }
void SetString(std::uint32_t i, const char* v) { if (!StringIndex(i)) return; if (!v) v = kEmptyString; std::strncpy(g_config->strings[i], v, kMaxRuntimeString-1); g_config->strings[i][kMaxRuntimeString-1] = '\0'; }
std::uintptr_t GetCallback(std::uint32_t i) { return CallbackIndex(i) ? g_config->callbacks[i] : 0; }
void SetCallback(std::uint32_t i, std::uintptr_t v) { if (CallbackIndex(i)) g_config->callbacks[i] = v; }
}
