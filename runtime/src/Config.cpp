#include <IAOT/Config.hpp>
#include <IAOT/Logging.hpp>
#include <array>
#include <atomic>
#include <cstring>

namespace iaot {
namespace {
constexpr std::size_t kMaxConfigImages = 16;
std::array<RuntimeConfig *, kMaxConfigImages> g_configs{};
std::size_t g_configCount = 0;
const char *kEmptyString = OBF("");
std::atomic_flag g_compoundLock = ATOMIC_FLAG_INIT;

struct CompoundGuard {
  CompoundGuard() {
    while (g_compoundLock.test_and_set(std::memory_order_acquire)) {
    }
  }
  ~CompoundGuard() { g_compoundLock.clear(std::memory_order_release); }
};

RuntimeConfig *Primary() {
  return g_configCount && ValidateConfig(g_configs[0]) ? g_configs[0] : nullptr;
}

template <typename Fn> void ForEachConfig(Fn &&fn) {
  for (std::size_t index = 0; index < g_configCount; ++index)
    if (ValidateConfig(g_configs[index]))
      fn(g_configs[index]);
}

bool FlagIndex(const RuntimeConfig *config, std::uint32_t index,
               std::uint32_t width = 1) {
  return config && index < config->header.flagsCount &&
         width <= config->header.flagsCount - index;
}
bool StringIndex(const RuntimeConfig *config, std::uint32_t index) {
  return config && index < config->header.stringsCount;
}
bool CallbackIndex(const RuntimeConfig *config, std::uint32_t index) {
  return config && index < config->header.callbacksCount;
}
bool OriginalIndex(const RuntimeConfig *config, std::uint32_t index) {
  return config &&
         config->header.reserved[0] == offsetof(RuntimeConfig, originals) &&
         index < config->header.reserved[1] && index < kMaxOriginals;
}
} // namespace

namespace detail {
void ResetRuntimeConfigsForTesting() {
  CompoundGuard guard;
  g_configs.fill(nullptr);
  g_configCount = 0;
}
void SetRuntimeConfig(RuntimeConfig *config) {
  ResetRuntimeConfigsForTesting();
  AddRuntimeConfig(config, 0);
}
void AddRuntimeConfig(RuntimeConfig *config, std::intptr_t slide) {
  if (!ValidateConfig(config))
    return;
  CompoundGuard guard;
  for (std::size_t index = 0; index < g_configCount; ++index)
    if (g_configs[index] == config)
      return;
  if (g_configCount >= g_configs.size()) {
    Log(OBF("IAOT runtime-config registry is full"));
    return;
  }
  RuntimeConfig *primary =
      g_configCount && ValidateConfig(g_configs[0]) ? g_configs[0] : nullptr;
  if (primary && primary != config) {
    std::memcpy(config->flags, primary->flags, sizeof(config->flags));
    std::memcpy(config->callbacks, primary->callbacks,
                sizeof(config->callbacks));
    std::memcpy(config->strings, primary->strings, sizeof(config->strings));
  }
  for (std::uint32_t index = 0; index < config->header.reserved[1]; ++index) {
    const auto value = config->originals[index];
    if (value)
      config->originals[index] = static_cast<std::uintptr_t>(
          static_cast<std::intptr_t>(value) + slide);
  }
  g_configs[g_configCount++] = config;
}
void InitializeConfig(RuntimeConfig &config) {
  std::memset(&config, 0, sizeof(config));
  std::memcpy(config.header.magic, kConfigMagic, sizeof(kConfigMagic));
  config.header.version = kConfigVersion;
  config.header.size = sizeof(RuntimeConfig);
  config.header.flagsOffset = offsetof(RuntimeConfig, flags);
  config.header.flagsCount = kMaxFlags;
  config.header.callbacksOffset = offsetof(RuntimeConfig, callbacks);
  config.header.callbacksCount = kMaxCallbackSlots;
  config.header.stringsOffset = offsetof(RuntimeConfig, strings);
  config.header.stringsCount = kMaxStrings;
  config.header.reserved[0] = offsetof(RuntimeConfig, originals);
  config.header.reserved[1] = kMaxOriginals;
}
} // namespace detail

RuntimeConfig *GetConfig() {
  CompoundGuard guard;
  return Primary();
}
bool HasConfig() { return GetConfig() != nullptr; }
bool ValidateConfig(const RuntimeConfig *config) {
  if (!config)
    return false;
  return std::memcmp(config->header.magic, kConfigMagic,
                     sizeof(kConfigMagic)) == 0 &&
         config->header.version == kConfigVersion &&
         config->header.size >= sizeof(RuntimeConfig) &&
         config->header.flagsOffset == offsetof(RuntimeConfig, flags) &&
         config->header.flagsCount <= kMaxFlags &&
         config->header.callbacksOffset == offsetof(RuntimeConfig, callbacks) &&
         config->header.callbacksCount <= kMaxCallbackSlots &&
         config->header.stringsOffset == offsetof(RuntimeConfig, strings) &&
         config->header.stringsCount <= kMaxStrings &&
         config->header.reserved[0] == offsetof(RuntimeConfig, originals) &&
         config->header.reserved[1] <= kMaxOriginals;
}

bool GetBool(std::uint32_t index) {
  CompoundGuard guard;
  auto *config = Primary();
  if (!FlagIndex(config, index))
    return false;
  return config->flags[index] != 0;
}
void SetBool(std::uint32_t index, bool value) {
  CompoundGuard guard;
  ForEachConfig([&](RuntimeConfig *config) {
    if (FlagIndex(config, index))
      config->flags[index] = value ? 1u : 0u;
  });
}
std::int32_t GetI32(std::uint32_t index) {
  return static_cast<std::int32_t>(GetU32(index));
}
void SetI32(std::uint32_t index, std::int32_t value) {
  SetU32(index, static_cast<std::uint32_t>(value));
}
std::uint32_t GetU32(std::uint32_t index) {
  CompoundGuard guard;
  auto *config = Primary();
  return FlagIndex(config, index) ? config->flags[index] : 0;
}
void SetU32(std::uint32_t index, std::uint32_t value) {
  CompoundGuard guard;
  ForEachConfig([&](RuntimeConfig *config) {
    if (FlagIndex(config, index))
      config->flags[index] = value;
  });
}
std::uint64_t GetU64(std::uint32_t index) {
  CompoundGuard guard;
  auto *config = Primary();
  std::uint64_t value{};
  if (!FlagIndex(config, index, 2))
    return value;
  std::memcpy(&value, &config->flags[index], sizeof(value));
  return value;
}
void SetU64(std::uint32_t index, std::uint64_t value) {
  CompoundGuard guard;
  ForEachConfig([&](RuntimeConfig *config) {
    if (FlagIndex(config, index, 2))
      std::memcpy(&config->flags[index], &value, sizeof(value));
  });
}
float GetF32(std::uint32_t index) {
  const std::uint32_t bits = GetU32(index);
  float value{};
  std::memcpy(&value, &bits, sizeof(value));
  return value;
}
void SetF32(std::uint32_t index, float value) {
  std::uint32_t bits{};
  std::memcpy(&bits, &value, sizeof(bits));
  SetU32(index, bits);
}
const char *GetString(std::uint32_t index) {
  CompoundGuard guard;
  auto *config = Primary();
  if (!StringIndex(config, index))
    return kEmptyString;
  thread_local char copy[kMaxRuntimeString]{};
  std::strncpy(copy, config->strings[index], kMaxRuntimeString - 1);
  copy[kMaxRuntimeString - 1] = '\0';
  return copy;
}
void SetString(std::uint32_t index, const char *value) {
  if (!value)
    value = kEmptyString;
  CompoundGuard guard;
  ForEachConfig([&](RuntimeConfig *config) {
    if (!StringIndex(config, index))
      return;
    std::strncpy(config->strings[index], value, kMaxRuntimeString - 1);
    config->strings[index][kMaxRuntimeString - 1] = '\0';
  });
}
std::uintptr_t GetCallback(std::uint32_t index) {
  CompoundGuard guard;
  auto *config = Primary();
  return CallbackIndex(config, index) ? config->callbacks[index] : 0;
}
void SetCallback(std::uint32_t index, std::uintptr_t value) {
  CompoundGuard guard;
  ForEachConfig([&](RuntimeConfig *config) {
    if (CallbackIndex(config, index))
      config->callbacks[index] = value;
  });
}
std::uintptr_t GetOriginal(std::uint32_t index) {
  CompoundGuard guard;
  for (std::size_t configIndex = 0; configIndex < g_configCount;
       ++configIndex) {
    auto *config = g_configs[configIndex];
    if (!OriginalIndex(config, index))
      continue;
    const auto value = config->originals[index];
    if (value)
      return value;
  }
  return 0;
}
} // namespace iaot
