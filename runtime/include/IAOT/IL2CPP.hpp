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

struct Options {
  const char *moduleName{nullptr};
  double timeoutSeconds{20.0};
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
IAOT_EXPORT const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                                         int parameterCount = -1);
IAOT_EXPORT FieldInfo *FindField(Il2CppClass *klass, const char *name);
IAOT_EXPORT const PropertyInfo *FindProperty(Il2CppClass *klass,
                                             const char *name);
IAOT_EXPORT std::uintptr_t NativeMethodPointer(const MethodInfo *method);
IAOT_EXPORT Il2CppObject *Invoke(const MethodInfo *method,
                                 Il2CppObject *instance,
                                 void **arguments = nullptr,
                                 Il2CppException **exception = nullptr);
IAOT_EXPORT std::uintptr_t ResolveInternalCall(const char *name);

namespace String {
IAOT_EXPORT Il2CppString *New(const char *utf8);
IAOT_EXPORT std::string ToUtf8(const Il2CppString *value);
} // namespace String

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
