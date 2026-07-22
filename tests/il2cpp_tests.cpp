#include <IAOT/IL2CPP.hpp>
#include <cassert>
int main() {
  assert(!iaot::il2cpp::InitializeAsync());
  assert(iaot::il2cpp::GetStatus() == iaot::il2cpp::Status::NotApplicable);
  assert(iaot::il2cpp::GetCapabilities() == 0);
  assert(iaot::il2cpp::String::ToUtf8(nullptr).empty());
  return 0;
}
