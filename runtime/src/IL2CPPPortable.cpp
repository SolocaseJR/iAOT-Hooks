#if !defined(__APPLE__)
#include <IAOT/IL2CPP.hpp>
namespace iaot::il2cpp {
bool InitializeAsync(const Options &) { return false; }
void MarkNotApplicable() {}
Status GetStatus() { return Status::NotApplicable; }
const char *StatusName(Status) { return "not_applicable"; }
std::uint64_t GetCapabilities() { return 0; }
bool IsReady() { return false; }
const char *GetResolvedModule() { return ""; }
void SetMethodPointerAdapter(MethodPointerAdapter) {}
const Il2CppAssembly *FindAssembly(const char *) { return nullptr; }
const Il2CppImage *FindImage(const char *) { return nullptr; }
Il2CppClass *FindClass(const char *, const char *, const char *) {
  return nullptr;
}
Il2CppClass *FindNestedClass(Il2CppClass *, const char *) { return nullptr; }
const MethodInfo *FindMethod(Il2CppClass *, const char *, int) {
  return nullptr;
}
FieldInfo *FindField(Il2CppClass *, const char *) { return nullptr; }
const PropertyInfo *FindProperty(Il2CppClass *, const char *) {
  return nullptr;
}
std::uintptr_t NativeMethodPointer(const MethodInfo *) { return 0; }
Il2CppObject *Invoke(const MethodInfo *, Il2CppObject *, void **,
                     Il2CppException **) {
  return nullptr;
}
std::uintptr_t ResolveInternalCall(const char *) { return 0; }
namespace String {
Il2CppString *New(const char *) { return nullptr; }
std::string ToUtf8(const Il2CppString *) { return {}; }
} // namespace String
namespace Field {
std::size_t Offset(const FieldInfo *) { return 0; }
bool GetStatic(const FieldInfo *, void *) { return false; }
bool SetStatic(FieldInfo *, const void *) { return false; }
bool GetInstance(Il2CppObject *, const FieldInfo *, void *) { return false; }
bool SetInstance(Il2CppObject *, FieldInfo *, const void *) { return false; }
} // namespace Field
} // namespace iaot::il2cpp
#endif
