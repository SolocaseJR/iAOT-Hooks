#pragma once
#include <cstddef>
#include <cstdint>

#include "IAOT/Obf.hpp"

#if defined(_MSC_VER)
#define IAOT_INLINE __forceinline
#define IAOT_NOINLINE __declspec(noinline)
#define IAOT_EXPORT __declspec(dllexport)
#define IAOT_IMPORT __declspec(dllimport)
#else
#define IAOT_INLINE inline __attribute__((always_inline))
#define IAOT_NOINLINE __attribute__((noinline))
#define IAOT_EXPORT __attribute__((visibility("default")))
#define IAOT_IMPORT
#endif
#define IAOT_UNUSED(x) (void)(x)
#define IAOT_ARRAY_COUNT(x) (sizeof(x) / sizeof((x)[0]))

namespace iaot {
using u8 = std::uint8_t;
using u16 = std::uint16_t;
using u32 = std::uint32_t;
using u64 = std::uint64_t;
using i8 = std::int8_t;
using i16 = std::int16_t;
using i32 = std::int32_t;
using i64 = std::int64_t;
constexpr std::uintptr_t AlignDown(std::uintptr_t value,
                                   std::size_t alignment) {
  return value & ~(alignment - 1);
}
constexpr std::uintptr_t AlignUp(std::uintptr_t value, std::size_t alignment) {
  return (value + alignment - 1) & ~(alignment - 1);
}
constexpr bool IsAligned(std::uintptr_t value, std::size_t alignment) {
  return alignment != 0 && (value % alignment) == 0;
}
} // namespace iaot
