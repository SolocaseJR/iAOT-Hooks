#pragma once
#include <IAOT/AppFunction.hpp>
#include <IAOT/Config.hpp>
#include <cstdint>

namespace iaot {
template <typename Signature>
AppFunction<Signature> OriginalFunction(std::uint32_t slot) {
  return AppFunction<Signature>::FromRuntime(GetOriginal(slot));
}
} // namespace iaot
