#include <IAOT/Config.hpp>
#include <IAOT/Logging.hpp>
#include <IAOT/Persistence.hpp>
#if defined(__APPLE__)
#import <Foundation/Foundation.h>
namespace iaot {
static NSString *Key(const char *key) {
  if (!key || !*key)
    return nil;
  return [NSString stringWithFormat:@"iaot.%s", key];
}
bool LoadPersistentBool(const char *key, std::uint32_t flagIndex,
                        bool fallback) {
  @autoreleasepool {
    NSString *valueKey = Key(key);
    if (!valueKey)
      return false;
    NSUserDefaults *defaults = [NSUserDefaults standardUserDefaults];
    id stored = [defaults objectForKey:valueKey];
    SetBool(flagIndex, [stored isKindOfClass:NSNumber.class]
                           ? [stored boolValue]
                           : fallback);
    return stored != nil;
  }
}
bool SavePersistentBool(const char *key, std::uint32_t flagIndex) {
  @autoreleasepool {
    NSString *valueKey = Key(key);
    if (!valueKey)
      return false;
    [[NSUserDefaults standardUserDefaults] setBool:GetBool(flagIndex)
                                            forKey:valueKey];
    return true;
  }
}
void RemovePersistentValue(const char *key) {
  @autoreleasepool {
    NSString *valueKey = Key(key);
    if (valueKey)
      [[NSUserDefaults standardUserDefaults] removeObjectForKey:valueKey];
  }
}
} // namespace iaot
#endif
