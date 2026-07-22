#pragma once
#include "IAOTGeneratedConfig.hpp"
namespace Features {
inline bool ExampleHookEnabled() {
  return IAOTGetFlag(IAOTFlag::example_hook_enabled);
}
inline void SetExampleHookEnabled(bool value) {
  IAOTSetFlag(IAOTFlag::example_hook_enabled, value);
}
} // namespace Features
