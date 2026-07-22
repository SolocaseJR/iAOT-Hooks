#include <IAOT/Config.hpp>
#include <cassert>
#include <cstddef>
int main() {
  static_assert(sizeof(iaot::RuntimeConfigHeader) == 72);
  assert(offsetof(iaot::RuntimeConfig, flags) == 72);
  assert(offsetof(iaot::RuntimeConfig, callbacks) == 1096);
  assert(offsetof(iaot::RuntimeConfig, originals) == 9800);
  assert(sizeof(iaot::RuntimeConfig) == 10312);
  return 0;
}
