#include <IAOT/Config.hpp>
#include <IAOT/Persistence.hpp>
#include <cassert>
int main() {
  iaot::RuntimeConfig config{};
  iaot::detail::InitializeConfig(config);
  iaot::detail::SetRuntimeConfig(&config);
  iaot::SetBool(4, true);
  assert(iaot::SavePersistentBool("test", 4));
  iaot::SetBool(4, false);
  assert(iaot::LoadPersistentBool("test", 4, false));
  assert(iaot::GetBool(4));
  iaot::RemovePersistentValue("test");
  return 0;
}
