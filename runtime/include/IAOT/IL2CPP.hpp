#pragma once
#include <IAOT/Types.hpp>
#include <cstddef>
#include <cstdint>
#include <string>

namespace iaot::il2cpp {
struct Il2CppDomain;
struct Il2CppAssembly;
struct Il2CppImage;
struct Il2CppClass;
struct MethodInfo;
struct FieldInfo;
struct PropertyInfo;
struct Il2CppObject;
struct Il2CppException;
struct Il2CppString;

enum class Status : std::uint32_t {
  NotApplicable = 0,
  NotDetected,
  WaitingForModule,
  WaitingForRuntime,
  Initializing,
  Ready,
  PartiallyAvailable,
  UnsupportedVersion,
  MetadataMissing,
  RuntimeFunctionsUnavailable,
  MethodUnresolved,
  InitializationFailed
};

enum Capability : std::uint64_t {
  Assemblies = 1ull << 0,
  Classes = 1ull << 1,
  Methods = 1ull << 2,
  Fields = 1ull << 3,
  Properties = 1ull << 4,
  Strings = 1ull << 5,
  Invocation = 1ull << 6,
  InternalCalls = 1ull << 7,
  ThreadAttach = 1ull << 8,
  NativeMethodPointers = 1ull << 9
};

// Versioned, dump-derived bindings used when a shipping IL2CPP binary has
// stripped the public il2cpp_* metadata API. The runtime creates stable
// synthetic handles so the normal Find* APIs can still be used.
struct OfflineClassBinding {
  const char *module{nullptr};
  const char *namespc{nullptr};
  const char *name{nullptr};
};
struct OfflineMethodBinding {
  const char *module{nullptr};
  const char *namespc{nullptr};
  const char *klass{nullptr};
  const char *name{nullptr};
  int parameterCount{-1};
  std::uintptr_t rva{0};
};
struct OfflineFieldBinding {
  const char *module{nullptr};
  const char *namespc{nullptr};
  const char *klass{nullptr};
  const char *name{nullptr};
  std::uintptr_t offset{0};
  std::size_t size{sizeof(void *)};
};
struct OfflineInternalCallBinding {
  const char *name{nullptr};
  const char *module{nullptr};
  std::uintptr_t rva{0};
};
struct OfflineMetadata {
  const OfflineClassBinding *classes{nullptr};
  std::size_t classCount{0};
  const OfflineMethodBinding *methods{nullptr};
  std::size_t methodCount{0};
  const OfflineFieldBinding *fields{nullptr};
  std::size_t fieldCount{0};
  const OfflineInternalCallBinding *internalCalls{nullptr};
  std::size_t internalCallCount{0};
};

struct Options {
  const char *moduleName{nullptr};
  double timeoutSeconds{20.0};
  // Optional target-relative address of a generated UTF-16 string factory.
  // This lets stripped Unity IL2CPP binaries create managed strings without
  // exporting the il2cpp_string_new debugging API.
  std::uintptr_t stringFactoryRva{0};
  // Optional dump-derived metadata/address bindings for stripped builds.
  const OfflineMetadata *offlineMetadata{nullptr};
};

using MethodPointerAdapter = std::uintptr_t (*)(const MethodInfo *method);

IAOT_EXPORT bool InitializeAsync(const Options &options = {});
IAOT_EXPORT void MarkNotApplicable();
IAOT_EXPORT Status GetStatus();
IAOT_EXPORT const char *StatusName(Status status);
IAOT_EXPORT std::uint64_t GetCapabilities();
IAOT_EXPORT bool IsReady();
IAOT_EXPORT const char *GetResolvedModule();
IAOT_EXPORT void SetMethodPointerAdapter(MethodPointerAdapter adapter);

IAOT_EXPORT const Il2CppAssembly *FindAssembly(const char *assemblyOrImageName);
IAOT_EXPORT const Il2CppImage *FindImage(const char *assemblyOrImageName);
IAOT_EXPORT Il2CppClass *FindClass(const char *assemblyOrImageName,
                                   const char *namespc, const char *name);
IAOT_EXPORT Il2CppClass *FindNestedClass(Il2CppClass *parent, const char *name);
IAOT_EXPORT Il2CppClass *GetParentClass(Il2CppClass *klass);
// Returns the live managed class name when runtime IL2CPP metadata is
// available. Offline class handles intentionally return nullptr.
IAOT_EXPORT const char *ClassName(Il2CppClass *klass);
IAOT_EXPORT const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                                          int parameterCount = -1);
IAOT_EXPORT FieldInfo *FindField(Il2CppClass *klass, const char *name);
IAOT_EXPORT const PropertyInfo *FindProperty(Il2CppClass *klass,
                                             const char *name);
IAOT_EXPORT std::uintptr_t NativeMethodPointer(const MethodInfo *method);
// Resolves a generated dump binding directly by managed name. This is useful
// for stripped players where a real MethodInfo cannot be obtained (notably
// inherited generic singleton accessors), but a versioned offline RVA exists.
IAOT_EXPORT std::uintptr_t
OfflineMethodPointer(const char *assemblyOrImageName, const char *namespc,
                     const char *klass, const char *name,
                     int parameterCount = -1);
IAOT_EXPORT Il2CppObject *Invoke(const MethodInfo *method,
                                 Il2CppObject *instance,
                                 void **arguments = nullptr,
                                 Il2CppException **exception = nullptr);
IAOT_EXPORT std::uintptr_t ResolveInternalCall(const char *name);

namespace String {
IAOT_EXPORT Il2CppString *New(const char *utf8);
IAOT_EXPORT std::string ToUtf8(const Il2CppString *value);
} // namespace String

// A managed object created via String::New (or any other allocation) is not
// automatically rooted just because a native pointer to it exists somewhere.
// The GC has no visibility into native globals/statics, so an unrooted
// object can be collected -- and its memory handed to the next managed
// allocation -- as soon as a collection runs. Anything held across more than
// one native call must be pinned with GC::Pin() first.
namespace GC {
// Roots and pins `object` so the collector will neither reclaim it nor move
// it. Returns a handle to release later with Unpin(), or 0 if pinning is
// unavailable (in which case the caller cannot safely cache the object).
IAOT_EXPORT std::uint32_t Pin(Il2CppObject *object);
IAOT_EXPORT Il2CppObject *GetTarget(std::uint32_t handle);
IAOT_EXPORT void Unpin(std::uint32_t handle);
} // namespace GC

namespace Field {
IAOT_EXPORT std::size_t Offset(const FieldInfo *field);
IAOT_EXPORT bool GetStatic(const FieldInfo *field, void *output);
IAOT_EXPORT bool SetStatic(FieldInfo *field, const void *value);
IAOT_EXPORT bool GetInstance(Il2CppObject *object, const FieldInfo *field,
                             void *output);
IAOT_EXPORT bool SetInstance(Il2CppObject *object, FieldInfo *field,
                             const void *value);
template <typename T> bool GetStatic(const FieldInfo *field, T &output) {
  return GetStatic(field, static_cast<void *>(&output));
}
template <typename T> bool SetStatic(FieldInfo *field, const T &value) {
  return SetStatic(field, static_cast<const void *>(&value));
}
template <typename T>
bool GetInstance(Il2CppObject *object, const FieldInfo *field, T &output) {
  return GetInstance(object, field, static_cast<void *>(&output));
}
template <typename T>
bool SetInstance(Il2CppObject *object, FieldInfo *field, const T &value) {
  return SetInstance(object, field, static_cast<const void *>(&value));
}
} // namespace Field
} // namespace iaot::il2cpp
