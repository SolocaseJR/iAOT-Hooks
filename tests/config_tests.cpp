#include <IAOT/Config.hpp>
#include <cassert>
#include <cstring>
int main() {
  iaot::RuntimeConfig cfg{};
  iaot::RuntimeConfig second{};
  iaot::detail::InitializeConfig(cfg);
  iaot::detail::InitializeConfig(second);
  assert(iaot::ValidateConfig(&cfg));
  iaot::detail::SetRuntimeConfig(&cfg);
  iaot::SetBool(0, true);
  iaot::SetCallback(0, 77);
  second.originals[3] = 123;
  iaot::detail::AddRuntimeConfig(&second, 10);
  assert(iaot::GetOriginal(3) == 133);
  assert(second.flags[0] == 1 && second.callbacks[0] == 77);
  iaot::SetI32(1, -7);
  assert(iaot::GetI32(1) == -7);
  iaot::SetU32(2, 42);
  assert(iaot::GetU32(2) == 42);
  iaot::SetU64(4, 0x1122334455667788ULL);
  assert(iaot::GetU64(4) == 0x1122334455667788ULL);
  iaot::SetF32(6, 1.5f);
  assert(iaot::GetF32(6) == 1.5f);
  iaot::SetString(0, "demo");
  assert(std::strcmp(iaot::GetString(0), "demo") == 0);
  iaot::SetCallback(0, 123);
  assert(iaot::GetCallback(0) == 123 && second.callbacks[0] == 123);
  assert(!iaot::GetBool(999));
  cfg.header.version = 99;
  assert(!iaot::ValidateConfig(&cfg));
  return 0;
}
