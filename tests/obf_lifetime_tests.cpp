#include <IAOT/Obf.hpp>

#include <cassert>
#include <cstring>

namespace {
const char *const kGlobalLookupName = OBF("Assets.Scripts.Models.Profile");

void ClobberStack() {
  volatile unsigned char bytes[4096]{};
  for (unsigned index = 0; index < sizeof(bytes); ++index)
    bytes[index] = static_cast<unsigned char>(index);
}
} // namespace

int main() {
  ClobberStack();
  assert(std::strcmp(kGlobalLookupName, "Assets.Scripts.Models.Profile") == 0);

  const char *first = OBF("persistent local value");
  ClobberStack();
  assert(std::strcmp(first, "persistent local value") == 0);

  // The decryptor remains owned by its call site instead of a returned
  // temporary, so a pointer retained by a generated/static table stays valid.
  const char *second = OBF("persistent local value");
  assert(std::strcmp(second, "persistent local value") == 0);
  return 0;
}
