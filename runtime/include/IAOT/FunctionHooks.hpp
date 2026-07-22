#pragma once
#include <cstdint>
namespace iaot {
template <typename Fn> struct FunctionHook {
  const char *name;
  std::uintptr_t target;
  Fn replacement;
  Fn *originalOut;
};
} // namespace iaot
