#if !defined(__APPLE__)
#include <IAOT/Config.hpp>
#include <IAOT/Persistence.hpp>
#include <mutex>
#include <string>
#include <unordered_map>
namespace iaot {
namespace {
std::mutex mutex;
std::unordered_map<std::string, bool> values;
} // namespace
bool LoadPersistentBool(const char *key, std::uint32_t index, bool fallback) {
  if (!key)
    return false;
  std::lock_guard<std::mutex> lock(mutex);
  auto it = values.find(key);
  SetBool(index, it == values.end() ? fallback : it->second);
  return it != values.end();
}
bool SavePersistentBool(const char *key, std::uint32_t index) {
  if (!key)
    return false;
  std::lock_guard<std::mutex> lock(mutex);
  values[key] = GetBool(index);
  return true;
}
void RemovePersistentValue(const char *key) {
  if (!key)
    return;
  std::lock_guard<std::mutex> lock(mutex);
  values.erase(key);
}
} // namespace iaot
#endif
