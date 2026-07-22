#pragma once
#include <IAOT/Types.hpp>
#include <cstdint>
namespace iaot {
IAOT_EXPORT bool LoadPersistentBool(const char *key, std::uint32_t flagIndex,
                                    bool fallback);
IAOT_EXPORT bool SavePersistentBool(const char *key, std::uint32_t flagIndex);
IAOT_EXPORT void RemovePersistentValue(const char *key);
} // namespace iaot
