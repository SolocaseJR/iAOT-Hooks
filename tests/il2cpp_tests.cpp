#include <IAOT/IL2CPP.hpp>
#include <cassert>
int main() {
  assert(!iaot::il2cpp::InitializeAsync());
  assert(iaot::il2cpp::GetStatus() == iaot::il2cpp::Status::NotApplicable);
  assert(iaot::il2cpp::GetCapabilities() == 0);
  assert(iaot::il2cpp::String::ToUtf8(nullptr).empty());
  // Name-only lookups degrade to null when no IL2CPP runtime is present.
  assert(!iaot::il2cpp::FindClass("System", "String"));
  assert(!iaot::il2cpp::FindMethod(nullptr, "Ctor", {"System.Char*", "System.Int32"}));
  assert(!iaot::il2cpp::FindObjectField(nullptr, "_items"));
  assert(iaot::il2cpp::Field::ValueOffset(nullptr) == 0);
  return 0;
}
