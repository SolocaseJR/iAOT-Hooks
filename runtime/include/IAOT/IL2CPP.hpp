#pragma once
#include <IAOT/Types.hpp>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <string>

// Runtime IL2CPP access by managed name.
//
// Every class, method and field is looked up by name through the game's own
// exported il2cpp_* API while the app runs. Nothing here needs a metadata
// dump, a native address, or a field offset from a previous build, so a game
// update needs no regeneration: names that still exist keep resolving, and a
// renamed or removed member returns null instead of a stale address.
namespace iaot::il2cpp {
struct Il2CppDomain;
struct Il2CppAssembly;
struct Il2CppImage;
struct Il2CppClass;
struct Il2CppType;
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
  // Image that exports the il2cpp_* API (e.g. "UnityFramework"); null or
  // "main" means the main executable.
  const char *moduleName{nullptr};
  double timeoutSeconds{20.0};
};

// Optional override for NativeMethodPointer. The default reads
// MethodInfo::methodPointer, which every IL2CPP version since Unity 5 keeps
// as the first MethodInfo member.
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
// Looks the class up in the named assembly first. A null/empty name, or a
// name that is not a loaded managed assembly (e.g. "UnityFramework"),
// searches every loaded assembly instead. Nested types may be written as
// "Outer+Inner" or "Outer/Inner".
IAOT_EXPORT Il2CppClass *FindClass(const char *assemblyOrImageName,
                                   const char *namespc, const char *name);
// Same as FindClass(nullptr, namespc, name): search every loaded assembly.
IAOT_EXPORT Il2CppClass *FindClass(const char *namespc, const char *name);
IAOT_EXPORT Il2CppClass *FindNestedClass(Il2CppClass *parent, const char *name);
IAOT_EXPORT Il2CppClass *GetParentClass(Il2CppClass *klass);
// The live (inflated, for generics) class of a managed object.
IAOT_EXPORT Il2CppClass *ObjectClass(const Il2CppObject *object);
IAOT_EXPORT const char *ClassName(Il2CppClass *klass);
IAOT_EXPORT const char *ClassNamespace(Il2CppClass *klass);

// Finds a method declared on `klass` or, if none matches there, on its
// nearest base class that declares one. parameterCount -1 accepts any
// arity. Returns null when several same-arity overloads match; pass the
// parameter types (below) to pick one.
IAOT_EXPORT const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                                          int parameterCount = -1);
// Overload-exact lookup by managed parameter type names, for example
// {"System.Char*", "System.Int32", "System.Int32"}. Nested types may use
// '+' or '/'. A name without a namespace ("Int32") matches by simple name.
IAOT_EXPORT const MethodInfo *
FindMethod(Il2CppClass *klass, const char *name,
           std::initializer_list<const char *> parameterTypes);
// Fields declared on `klass` or any base class.
IAOT_EXPORT FieldInfo *FindField(Il2CppClass *klass, const char *name);
// Field lookup through an object's live class, which is required for fields
// declared on generic base classes (Player`1, List`1, ...).
IAOT_EXPORT FieldInfo *FindObjectField(const Il2CppObject *object,
                                       const char *name);
IAOT_EXPORT const PropertyInfo *FindProperty(Il2CppClass *klass,
                                             const char *name);
// Native entry point of a method. Generic shared code expects the same
// MethodInfo as its final argument, so pass it when calling.
IAOT_EXPORT std::uintptr_t NativeMethodPointer(const MethodInfo *method);
IAOT_EXPORT Il2CppObject *Invoke(const MethodInfo *method,
                                 Il2CppObject *instance,
                                 void **arguments = nullptr,
                                 Il2CppException **exception = nullptr);
IAOT_EXPORT std::uintptr_t ResolveInternalCall(const char *name);

namespace Array {
// Bytes per element (pointer size for reference types, the struct size for
// value types) and the element class of a managed array object.
IAOT_EXPORT std::size_t ElementSize(const Il2CppObject *array);
IAOT_EXPORT Il2CppClass *ElementClass(const Il2CppObject *array);
} // namespace Array

namespace String {
// Uses il2cpp_string_new, or the game's own System.String.Ctor /
// CreateString(char*, int, int) found by name when that export is stripped.
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
// Unity 2022.2+ IL2CPP encodes a GC handle as a pointer-sized value, not the
// older 32-bit slot index; truncating it makes GetTarget fault.
using Handle = std::uintptr_t;

// Roots and pins `object` so the collector will neither reclaim it nor move
// it. Returns a handle to release later with Unpin(), or 0 if pinning is
// unavailable (in which case the caller cannot safely cache the object).
IAOT_EXPORT Handle Pin(Il2CppObject *object);
IAOT_EXPORT Il2CppObject *GetTarget(Handle handle);
IAOT_EXPORT void Unpin(Handle handle);
} // namespace GC

namespace Field {
// Offset from the start of an object (including its 16-byte header).
IAOT_EXPORT std::size_t Offset(const FieldInfo *field);
// Offset inside an unboxed value-type instance, e.g. an element of a struct
// array such as Dictionary`2+Entry[].
IAOT_EXPORT std::size_t ValueOffset(const FieldInfo *field);
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
