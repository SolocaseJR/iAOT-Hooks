#include <IAOT/IL2CPP.hpp>
#include <IAOT/Logging.hpp>
#include <IAOT/SymbolResolver.hpp>

#if defined(__APPLE__)
#import <Foundation/Foundation.h>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <dlfcn.h>
#include <mach-o/dyld.h>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <vector>

namespace iaot::il2cpp {
namespace {
template <typename T> T Symbol(const char *name) {
  return reinterpret_cast<T>(dlsym(RTLD_DEFAULT, name));
}

struct Api {
  const Il2CppImage *(*getCorlib)(){};
  Il2CppDomain *(*domainGet)(){};
  const Il2CppAssembly **(*domainGetAssemblies)(const Il2CppDomain *,
                                                std::size_t *){};
  const Il2CppImage *(*assemblyGetImage)(const Il2CppAssembly *){};
  const char *(*imageGetName)(const Il2CppImage *){};
  Il2CppClass *(*classFromName)(const Il2CppImage *, const char *,
                                const char *){};
  Il2CppClass *(*classGetNestedTypes)(Il2CppClass *, void **){};
  Il2CppClass *(*classGetParent)(Il2CppClass *){};
  const char *(*classGetName)(Il2CppClass *){};
  const char *(*classGetNamespace)(Il2CppClass *){};
  Il2CppClass *(*classGetElementClass)(Il2CppClass *){};
  int (*classArrayElementSize)(const Il2CppClass *){};
  Il2CppClass *(*objectGetClass)(const Il2CppObject *){};
  const MethodInfo *(*classGetMethods)(Il2CppClass *, void **){};
  const MethodInfo *(*classGetMethodFromName)(Il2CppClass *, const char *,
                                              int){};
  const char *(*methodGetName)(const MethodInfo *){};
  std::uint32_t (*methodGetParamCount)(const MethodInfo *){};
  const Il2CppType *(*methodGetParam)(const MethodInfo *, std::uint32_t){};
  char *(*typeGetName)(const Il2CppType *){};
  void (*freeMemory)(void *){};
  FieldInfo *(*classGetFieldFromName)(Il2CppClass *, const char *){};
  const PropertyInfo *(*classGetPropertyFromName)(Il2CppClass *,
                                                  const char *){};
  std::size_t (*fieldGetOffset)(const FieldInfo *){};
  void (*fieldStaticGetValue)(const FieldInfo *, void *){};
  void (*fieldStaticSetValue)(FieldInfo *, void *){};
  void (*fieldGetValue)(Il2CppObject *, const FieldInfo *, void *){};
  void (*fieldSetValue)(Il2CppObject *, FieldInfo *, void *){};
  Il2CppString *(*stringNew)(const char *){};
  const std::uint16_t *(*stringChars)(const Il2CppString *){};
  std::int32_t (*stringLength)(const Il2CppString *){};
  Il2CppObject *(*runtimeInvoke)(const MethodInfo *, void *, void **,
                                 Il2CppException **){};
  void *(*resolveIcall)(const char *){};
  void *(*threadAttach)(Il2CppDomain *){};
  GC::Handle (*gcHandleNew)(Il2CppObject *, bool){};
  Il2CppObject *(*gcHandleGetTarget)(GC::Handle){};
  void (*gcHandleFree)(GC::Handle){};
};

Api g_api{};
std::atomic<Status> g_status{Status::NotDetected};
std::atomic<std::uint64_t> g_capabilities{0};
std::atomic<MethodPointerAdapter> g_methodPointerAdapter{nullptr};
std::atomic_bool g_apiPublished{false};
std::once_flag g_startOnce;
std::mutex g_apiMutex;
std::mutex g_imageMutex;
std::condition_variable g_imageChanged;
std::mutex g_classCacheMutex;
std::unordered_map<std::string, Il2CppClass *> g_classCache;
std::string g_moduleName;
std::string g_requestedModule;
double g_timeout = 20.0;

void ImageAdded(const mach_header *, std::intptr_t) {
  g_imageChanged.notify_all();
}

Api ApiSnapshot() {
  if (!g_apiPublished.load(std::memory_order_acquire))
    return {};
  std::lock_guard<std::mutex> lock(g_apiMutex);
  return g_api;
}

enum class ApiResult { Unavailable, NotReady, Ready };

ApiResult ResolveApi() {
  Api api{};
  api.getCorlib = Symbol<decltype(api.getCorlib)>("il2cpp_get_corlib");
  api.domainGet = Symbol<decltype(api.domainGet)>("il2cpp_domain_get");
  api.domainGetAssemblies =
      Symbol<decltype(api.domainGetAssemblies)>("il2cpp_domain_get_assemblies");
  api.assemblyGetImage =
      Symbol<decltype(api.assemblyGetImage)>("il2cpp_assembly_get_image");
  api.imageGetName =
      Symbol<decltype(api.imageGetName)>("il2cpp_image_get_name");
  api.classFromName =
      Symbol<decltype(api.classFromName)>("il2cpp_class_from_name");
  api.classGetNestedTypes = Symbol<decltype(api.classGetNestedTypes)>(
      "il2cpp_class_get_nested_types");
  api.classGetParent =
      Symbol<decltype(api.classGetParent)>("il2cpp_class_get_parent");
  api.classGetName =
      Symbol<decltype(api.classGetName)>("il2cpp_class_get_name");
  api.classGetNamespace =
      Symbol<decltype(api.classGetNamespace)>("il2cpp_class_get_namespace");
  api.classGetElementClass = Symbol<decltype(api.classGetElementClass)>(
      "il2cpp_class_get_element_class");
  api.classArrayElementSize = Symbol<decltype(api.classArrayElementSize)>(
      "il2cpp_class_array_element_size");
  api.objectGetClass =
      Symbol<decltype(api.objectGetClass)>("il2cpp_object_get_class");
  api.classGetMethods =
      Symbol<decltype(api.classGetMethods)>("il2cpp_class_get_methods");
  api.classGetMethodFromName = Symbol<decltype(api.classGetMethodFromName)>(
      "il2cpp_class_get_method_from_name");
  api.methodGetName =
      Symbol<decltype(api.methodGetName)>("il2cpp_method_get_name");
  api.methodGetParamCount = Symbol<decltype(api.methodGetParamCount)>(
      "il2cpp_method_get_param_count");
  api.methodGetParam =
      Symbol<decltype(api.methodGetParam)>("il2cpp_method_get_param");
  api.typeGetName = Symbol<decltype(api.typeGetName)>("il2cpp_type_get_name");
  api.freeMemory = Symbol<decltype(api.freeMemory)>("il2cpp_free");
  api.classGetFieldFromName = Symbol<decltype(api.classGetFieldFromName)>(
      "il2cpp_class_get_field_from_name");
  api.classGetPropertyFromName = Symbol<decltype(api.classGetPropertyFromName)>(
      "il2cpp_class_get_property_from_name");
  api.fieldGetOffset =
      Symbol<decltype(api.fieldGetOffset)>("il2cpp_field_get_offset");
  api.fieldStaticGetValue = Symbol<decltype(api.fieldStaticGetValue)>(
      "il2cpp_field_static_get_value");
  api.fieldStaticSetValue = Symbol<decltype(api.fieldStaticSetValue)>(
      "il2cpp_field_static_set_value");
  api.fieldGetValue =
      Symbol<decltype(api.fieldGetValue)>("il2cpp_field_get_value");
  api.fieldSetValue =
      Symbol<decltype(api.fieldSetValue)>("il2cpp_field_set_value");
  api.stringNew = Symbol<decltype(api.stringNew)>("il2cpp_string_new");
  api.stringChars = Symbol<decltype(api.stringChars)>("il2cpp_string_chars");
  api.stringLength = Symbol<decltype(api.stringLength)>("il2cpp_string_length");
  api.runtimeInvoke =
      Symbol<decltype(api.runtimeInvoke)>("il2cpp_runtime_invoke");
  api.resolveIcall = Symbol<decltype(api.resolveIcall)>("il2cpp_resolve_icall");
  api.threadAttach = Symbol<decltype(api.threadAttach)>("il2cpp_thread_attach");
  api.gcHandleNew = Symbol<decltype(api.gcHandleNew)>("il2cpp_gchandle_new");
  api.gcHandleGetTarget = Symbol<decltype(api.gcHandleGetTarget)>(
      "il2cpp_gchandle_get_target");
  api.gcHandleFree =
      Symbol<decltype(api.gcHandleFree)>("il2cpp_gchandle_free");
  if (!api.domainGet || !api.domainGetAssemblies || !api.assemblyGetImage ||
      !api.imageGetName)
    return ApiResult::Unavailable;
  // The worker starts as soon as the module is mapped, which can be while the
  // host is still inside dlopen and before il2cpp_init. il2cpp_domain_get then
  // lazily allocates the domain through an uninitialized GC and faults. The
  // corlib image is published only once initialization has run, and reading
  // it has no side effects, so it is the readiness gate.
  if (!api.getCorlib)
    return ApiResult::Unavailable;
  if (!api.getCorlib())
    return ApiResult::NotReady;
  auto *domain = api.domainGet();
  if (!domain)
    return ApiResult::NotReady;
  std::uint64_t capabilities = Assemblies | NativeMethodPointers;
  if (api.classFromName)
    capabilities |= Classes;
  if (api.classGetMethodFromName ||
      (api.classGetMethods && api.methodGetName && api.methodGetParamCount))
    capabilities |= Methods;
  if (api.classGetFieldFromName && api.fieldGetOffset &&
      api.fieldStaticGetValue && api.fieldStaticSetValue && api.fieldGetValue &&
      api.fieldSetValue)
    capabilities |= Fields;
  if (api.classGetPropertyFromName)
    capabilities |= Properties;
  if (api.stringChars && api.stringLength &&
      (api.stringNew || (api.classFromName && api.classGetMethods)))
    capabilities |= Strings;
  if (api.runtimeInvoke)
    capabilities |= Invocation;
  if (api.resolveIcall)
    capabilities |= InternalCalls;
  if (api.threadAttach)
    capabilities |= ThreadAttach;
  if (api.threadAttach)
    api.threadAttach(domain);
  Dl_info info{};
  if (dladdr(reinterpret_cast<void *>(api.domainGet), &info) && info.dli_fname)
    g_moduleName = info.dli_fname;
  {
    std::lock_guard<std::mutex> lock(g_apiMutex);
    g_api = api;
    g_capabilities.store(capabilities, std::memory_order_release);
    g_apiPublished.store(true, std::memory_order_release);
  }
  const std::uint64_t core =
      Assemblies | Classes | Methods | Fields | Strings | Invocation;
  g_status.store((capabilities & core) == core ? Status::Ready
                                               : Status::PartiallyAvailable,
                 std::memory_order_release);
  return ApiResult::Ready;
}

bool RequestedModuleLoaded() {
  if (g_requestedModule.empty() || g_requestedModule == "main")
    return true;
  for (std::uint32_t index = 0; index < _dyld_image_count(); ++index) {
    const char *name = _dyld_get_image_name(index);
    if (name && std::strstr(name, g_requestedModule.c_str()))
      return true;
  }
  return false;
}

void InitializeWorker() {
  const auto deadline = std::chrono::steady_clock::now() +
                        std::chrono::milliseconds(static_cast<long long>(
                            g_timeout > 0 ? g_timeout * 1000.0 : 1.0));
  g_status.store(Status::WaitingForModule, std::memory_order_release);
  std::unique_lock<std::mutex> lock(g_imageMutex);
  while (std::chrono::steady_clock::now() < deadline) {
    if (RequestedModuleLoaded()) {
      g_status.store(Status::WaitingForRuntime, std::memory_order_release);
      lock.unlock();
      const auto result = ResolveApi();
      if (result == ApiResult::Ready) {
        Logf("IL2CPP resolver %s; module=%s capabilities=0x%llx",
             StatusName(GetStatus()), GetResolvedModule(),
             static_cast<unsigned long long>(GetCapabilities()));
        return;
      }
      lock.lock();
    }
    g_imageChanged.wait_for(lock, std::chrono::milliseconds(100));
  }
  lock.unlock();
  g_status.store(Status::RuntimeFunctionsUnavailable,
                 std::memory_order_release);
  Log(OBF("IL2CPP resolver timed out; unrelated IAOT systems remain enabled"));
}

bool SameImageName(const char *actual, const char *requested) {
  if (!actual || !requested)
    return false;
  if (std::strcmp(actual, requested) == 0)
    return true;
  const char *actualDot = std::strrchr(actual, '.');
  const char *requestedDot = std::strrchr(requested, '.');
  const std::size_t actualLength =
      actualDot ? static_cast<std::size_t>(actualDot - actual)
                : std::strlen(actual);
  const std::size_t requestedLength =
      requestedDot ? static_cast<std::size_t>(requestedDot - requested)
                   : std::strlen(requested);
  return actualLength == requestedLength &&
         std::strncmp(actual, requested, actualLength) == 0;
}

// Every loaded managed image, in domain order.
std::vector<const Il2CppImage *> Images(const Api &api) {
  std::vector<const Il2CppImage *> images;
  if (!api.domainGet || !api.domainGetAssemblies || !api.assemblyGetImage)
    return images;
  auto *domain = api.domainGet();
  std::size_t count = 0;
  const auto **assemblies =
      domain ? api.domainGetAssemblies(domain, &count) : nullptr;
  if (!assemblies || count > 100000)
    return images;
  images.reserve(count);
  for (std::size_t index = 0; index < count; ++index)
    if (const auto *image = api.assemblyGetImage(assemblies[index]))
      images.push_back(image);
  return images;
}

Il2CppClass *TopLevelClass(const Api &api, const Il2CppImage *image,
                           const char *namespc, const char *name) {
  if (!api.classFromName)
    return nullptr;
  if (image)
    return api.classFromName(image, namespc, name);
  for (const auto *candidate : Images(api))
    if (auto *klass = api.classFromName(candidate, namespc, name))
      return klass;
  return nullptr;
}

// "Outer+Inner" / "Outer/Inner" -> the nested class.
Il2CppClass *ResolveClass(const Api &api, const Il2CppImage *image,
                          const char *namespc, const char *name) {
  const char *split = std::strpbrk(name, "+/");
  if (!split)
    return TopLevelClass(api, image, namespc, name);
  auto *klass = TopLevelClass(api, image, namespc,
                              std::string(name, split).c_str());
  while (klass && split) {
    const char *part = split + 1;
    split = std::strpbrk(part, "+/");
    const std::string nested =
        split ? std::string(part, split) : std::string(part);
    klass = FindNestedClass(klass, nested.c_str());
  }
  return klass;
}

// Managed type names compare with '/' and '+' as the same nesting separator.
// A wanted name without a namespace matches the actual name's last segment.
bool SameTypeName(const char *actual, const char *wanted) {
  if (!actual || !wanted)
    return false;
  auto same = [](const char *a, const char *b) {
    for (; *a && *b; ++a, ++b) {
      const char x = *a == '/' ? '+' : *a;
      const char y = *b == '/' ? '+' : *b;
      if (x != y)
        return false;
    }
    return *a == *b;
  };
  if (same(actual, wanted))
    return true;
  if (std::strchr(wanted, '.'))
    return false;
  const char *dot = std::strrchr(actual, '.');
  return dot && same(dot + 1, wanted);
}

bool ParameterTypesMatch(const Api &api, const MethodInfo *method,
                         std::initializer_list<const char *> types) {
  std::uint32_t index = 0;
  for (const char *wanted : types) {
    const auto *type = api.methodGetParam(method, index++);
    char *actual = type ? api.typeGetName(type) : nullptr;
    const bool match = SameTypeName(actual, wanted);
    if (actual && api.freeMemory)
      api.freeMemory(actual);
    if (!match)
      return false;
  }
  return true;
}

// Searches `klass`, then each base class, for methods named `name` that pass
// `accept`. The first class with any match decides; several matches there
// are ambiguous and set `ambiguous`.
template <typename Accept>
const MethodInfo *FindDeclaredMethod(const Api &api, Il2CppClass *klass,
                                     const char *name, int parameterCount,
                                     Accept &&accept, bool &ambiguous) {
  ambiguous = false;
  for (int depth = 0; klass && depth < 64;
       ++depth, klass = api.classGetParent ? api.classGetParent(klass) : nullptr) {
    void *iterator = nullptr;
    const MethodInfo *result = nullptr;
    std::size_t visited = 0;
    while (const auto *candidate = api.classGetMethods(klass, &iterator)) {
      if (++visited > 100000)
        return nullptr;
      const char *candidateName = api.methodGetName(candidate);
      if (!candidateName || std::strcmp(candidateName, name) != 0)
        continue;
      if (parameterCount >= 0 && api.methodGetParamCount(candidate) !=
                                     static_cast<std::uint32_t>(parameterCount))
        continue;
      if (!accept(candidate))
        continue;
      if (result) {
        Logf("IL2CPP method is ambiguous: %s/%d; pass parameter types", name,
             parameterCount);
        ambiguous = true;
        return nullptr;
      }
      result = candidate;
    }
    if (result)
      return result;
  }
  return nullptr;
}

void AppendUtf8(std::string &output, std::uint32_t codepoint) {
  if (codepoint <= 0x7f)
    output.push_back(static_cast<char>(codepoint));
  else if (codepoint <= 0x7ff) {
    output.push_back(static_cast<char>(0xc0 | (codepoint >> 6)));
    output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
  } else if (codepoint <= 0xffff) {
    output.push_back(static_cast<char>(0xe0 | (codepoint >> 12)));
    output.push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3f)));
    output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
  } else {
    output.push_back(static_cast<char>(0xf0 | (codepoint >> 18)));
    output.push_back(static_cast<char>(0x80 | ((codepoint >> 12) & 0x3f)));
    output.push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3f)));
    output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
  }
}

// Managed object header (klass + monitor), the base of every field offset.
constexpr std::size_t kObjectHeaderSize = 2 * sizeof(void *);
} // namespace

bool InitializeAsync(const Options &options) {
  std::call_once(g_startOnce, [&] {
    if (options.moduleName)
      g_requestedModule = options.moduleName;
    g_timeout = options.timeoutSeconds;
    g_status.store(Status::Initializing, std::memory_order_release);
    _dyld_register_func_for_add_image(ImageAdded);
    std::thread(InitializeWorker).detach();
  });
  return true;
}
void MarkNotApplicable() {
  Status expected = Status::NotDetected;
  g_status.compare_exchange_strong(expected, Status::NotApplicable,
                                   std::memory_order_acq_rel);
}
Status GetStatus() { return g_status.load(std::memory_order_acquire); }
const char *StatusName(Status status) {
  switch (status) {
  case Status::NotApplicable:
    return "not_applicable";
  case Status::NotDetected:
    return "not_detected";
  case Status::WaitingForModule:
    return "waiting_for_module";
  case Status::WaitingForRuntime:
    return "waiting_for_il2cpp";
  case Status::Initializing:
    return "initializing";
  case Status::Ready:
    return "ready";
  case Status::PartiallyAvailable:
    return "partially_available";
  case Status::UnsupportedVersion:
    return "unsupported_version";
  case Status::MetadataMissing:
    return "metadata_missing";
  case Status::RuntimeFunctionsUnavailable:
    return "runtime_functions_unavailable";
  case Status::MethodUnresolved:
    return "method_unresolved";
  case Status::InitializationFailed:
    return "initialization_failed";
  }
  return "unknown";
}
std::uint64_t GetCapabilities() {
  return g_capabilities.load(std::memory_order_acquire);
}
bool IsReady() {
  const auto status = GetStatus();
  return status == Status::Ready || status == Status::PartiallyAvailable;
}
const char *GetResolvedModule() {
  if (!g_apiPublished.load(std::memory_order_acquire))
    return OBF("");
  return g_moduleName.empty() ? OBF("") : g_moduleName.c_str();
}
void SetMethodPointerAdapter(MethodPointerAdapter adapter) {
  g_methodPointerAdapter.store(adapter, std::memory_order_release);
}

const Il2CppAssembly *FindAssembly(const char *name) {
  if (!name || !(GetCapabilities() & Assemblies))
    return nullptr;
  const auto api = ApiSnapshot();
  if (!api.domainGet || !api.domainGetAssemblies || !api.assemblyGetImage ||
      !api.imageGetName)
    return nullptr;
  auto *domain = api.domainGet();
  std::size_t count = 0;
  const auto **assemblies =
      domain ? api.domainGetAssemblies(domain, &count) : nullptr;
  if (!assemblies || count > 100000)
    return nullptr;
  for (std::size_t index = 0; index < count; ++index) {
    const auto *image = api.assemblyGetImage(assemblies[index]);
    if (image && SameImageName(api.imageGetName(image), name))
      return assemblies[index];
  }
  return nullptr;
}
const Il2CppImage *FindImage(const char *name) {
  const auto *assembly = FindAssembly(name);
  const auto api = ApiSnapshot();
  return assembly && api.assemblyGetImage ? api.assemblyGetImage(assembly)
                                          : nullptr;
}

Il2CppClass *FindClass(const char *image, const char *namespc,
                       const char *name) {
  if (!name || !*name || !(GetCapabilities() & Classes))
    return nullptr;
  const char *space = namespc ? namespc : "";
  std::string key = image ? image : "";
  key.push_back('\n');
  key += space;
  key.push_back('\n');
  key += name;
  {
    std::lock_guard<std::mutex> lock(g_classCacheMutex);
    if (const auto found = g_classCache.find(key); found != g_classCache.end())
      return found->second;
  }
  const auto api = ApiSnapshot();
  Il2CppClass *klass = nullptr;
  if (image && *image)
    if (const auto *resolved = FindImage(image))
      klass = ResolveClass(api, resolved, space, name);
  // Not a managed assembly name (e.g. the native "UnityFramework"), or the
  // type lives elsewhere: every loaded assembly is searched by name.
  if (!klass)
    klass = ResolveClass(api, nullptr, space, name);
  if (klass) {
    std::lock_guard<std::mutex> lock(g_classCacheMutex);
    g_classCache.emplace(std::move(key), klass);
  }
  return klass;
}
Il2CppClass *FindClass(const char *namespc, const char *name) {
  return FindClass(nullptr, namespc, name);
}
Il2CppClass *FindNestedClass(Il2CppClass *parent, const char *name) {
  const auto api = ApiSnapshot();
  if (!parent || !name || !api.classGetNestedTypes || !api.classGetName)
    return nullptr;
  void *iterator = nullptr;
  while (auto *nested = api.classGetNestedTypes(parent, &iterator))
    if (const char *actual = api.classGetName(nested);
        actual && std::strcmp(actual, name) == 0)
      return nested;
  return nullptr;
}
Il2CppClass *GetParentClass(Il2CppClass *klass) {
  if (!klass)
    return nullptr;
  const auto api = ApiSnapshot();
  return api.classGetParent ? api.classGetParent(klass) : nullptr;
}
Il2CppClass *ObjectClass(const Il2CppObject *object) {
  if (!object)
    return nullptr;
  const auto api = ApiSnapshot();
  if (api.objectGetClass)
    return api.objectGetClass(object);
  // Il2CppObject always starts with its class pointer.
  return *reinterpret_cast<Il2CppClass *const *>(object);
}
const char *ClassName(Il2CppClass *klass) {
  if (!klass)
    return nullptr;
  const auto api = ApiSnapshot();
  return api.classGetName ? api.classGetName(klass) : nullptr;
}
const char *ClassNamespace(Il2CppClass *klass) {
  if (!klass)
    return nullptr;
  const auto api = ApiSnapshot();
  return api.classGetNamespace ? api.classGetNamespace(klass) : nullptr;
}

const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                              int parameterCount) {
  const auto api = ApiSnapshot();
  if (!klass || !name)
    return nullptr;
  if (api.classGetMethods && api.methodGetName && api.methodGetParamCount) {
    bool ambiguous = false;
    if (const auto *method =
            FindDeclaredMethod(api, klass, name, parameterCount,
                               [](const MethodInfo *) { return true; }, ambiguous))
      return method;
    if (ambiguous)
      return nullptr;
  }
  return api.classGetMethodFromName
             ? api.classGetMethodFromName(klass, name, parameterCount)
             : nullptr;
}
const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                              std::initializer_list<const char *> types) {
  const auto api = ApiSnapshot();
  if (!klass || !name)
    return nullptr;
  if (!api.classGetMethods || !api.methodGetName || !api.methodGetParamCount)
    return nullptr;
  const int count = static_cast<int>(types.size());
  if (!api.methodGetParam || !api.typeGetName) {
    Logf("IL2CPP parameter type exports are missing; %s/%d resolved by arity",
         name, count);
    return FindMethod(klass, name, count);
  }
  bool ambiguous = false;
  return FindDeclaredMethod(
      api, klass, name, count,
      [&](const MethodInfo *candidate) {
        return ParameterTypesMatch(api, candidate, types);
      },
      ambiguous);
}
FieldInfo *FindField(Il2CppClass *klass, const char *name) {
  const auto api = ApiSnapshot();
  // il2cpp_class_get_field_from_name already walks the base classes.
  return klass && name && api.classGetFieldFromName
             ? api.classGetFieldFromName(klass, name)
             : nullptr;
}
FieldInfo *FindObjectField(const Il2CppObject *object, const char *name) {
  return FindField(ObjectClass(object), name);
}
const PropertyInfo *FindProperty(Il2CppClass *klass, const char *name) {
  const auto api = ApiSnapshot();
  return klass && name && api.classGetPropertyFromName
             ? api.classGetPropertyFromName(klass, name)
             : nullptr;
}
std::uintptr_t NativeMethodPointer(const MethodInfo *method) {
  if (!method)
    return 0;
  if (const auto adapter = g_methodPointerAdapter.load(std::memory_order_acquire))
    return adapter(method);
  // MethodInfo::methodPointer is the first member in every IL2CPP version
  // since Unity 5. Only accept an address inside a loaded image.
  const auto pointer = *reinterpret_cast<const std::uintptr_t *>(method);
  Dl_info info{};
  if (!pointer || !dladdr(reinterpret_cast<const void *>(pointer), &info) ||
      !info.dli_fbase)
    return 0;
  return pointer;
}
Il2CppObject *Invoke(const MethodInfo *method, Il2CppObject *instance,
                     void **arguments, Il2CppException **exception) {
  const auto api = ApiSnapshot();
  if (!method || !api.runtimeInvoke)
    return nullptr;
  if (api.threadAttach)
    if (auto *domain = api.domainGet())
      api.threadAttach(domain);
  Il2CppException *localException = nullptr;
  return api.runtimeInvoke(method, instance, arguments,
                           exception ? exception : &localException);
}
std::uintptr_t ResolveInternalCall(const char *name) {
  const auto api = ApiSnapshot();
  if (name && api.resolveIcall)
    return reinterpret_cast<std::uintptr_t>(api.resolveIcall(name));
  return 0;
}

namespace Array {
std::size_t ElementSize(const Il2CppObject *array) {
  const auto api = ApiSnapshot();
  auto *klass = ObjectClass(array);
  return klass && api.classArrayElementSize
             ? static_cast<std::size_t>(api.classArrayElementSize(klass))
             : 0;
}
Il2CppClass *ElementClass(const Il2CppObject *array) {
  const auto api = ApiSnapshot();
  auto *klass = ObjectClass(array);
  return klass && api.classGetElementClass ? api.classGetElementClass(klass)
                                           : nullptr;
}
} // namespace Array

namespace String {
namespace {
// Stripped players keep the managed constructors that il2cpp_string_new
// would otherwise wrap. Both take (char*, startIndex, length).
using StaticCtor = Il2CppString *(*)(const std::uint16_t *, std::int32_t,
                                     std::int32_t, const MethodInfo *);
using InstanceCtor = Il2CppString *(*)(void *, const std::uint16_t *,
                                       std::int32_t, std::int32_t,
                                       const MethodInfo *);
struct ManagedFactory {
  const MethodInfo *method{};
  std::uintptr_t pointer{};
  bool isStatic{};
};
ManagedFactory FindManagedFactory() {
  static std::mutex mutex;
  static ManagedFactory cached;
  std::lock_guard<std::mutex> lock(mutex);
  if (cached.pointer)
    return cached;
  auto *klass = FindClass(OBF("System"), OBF("String"));
  if (!klass)
    return {};
  const char *charPointer = OBF("System.Char*");
  const char *int32 = OBF("System.Int32");
  if (const auto *method =
          FindMethod(klass, OBF("Ctor"), {charPointer, int32, int32}))
    cached = {method, NativeMethodPointer(method), true};
  if (!cached.pointer)
    if (const auto *method =
            FindMethod(klass, OBF("CreateString"), {charPointer, int32, int32}))
      cached = {method, NativeMethodPointer(method), false};
  return cached;
}
} // namespace

Il2CppString *New(const char *utf8) {
  const auto api = ApiSnapshot();
  if (!utf8)
    return nullptr;
  if (api.stringNew)
    return api.stringNew(utf8);

  const auto factory = FindManagedFactory();
  if (!factory.pointer)
    return nullptr;

  @autoreleasepool {
    NSString *value = [NSString stringWithUTF8String:utf8];
    if (!value)
      return nullptr;
    const NSUInteger length = value.length;
    std::vector<std::uint16_t> utf16(length + 1, 0);
    [value getCharacters:reinterpret_cast<unichar *>(utf16.data())
                    range:NSMakeRange(0, length)];
    const auto count = static_cast<std::int32_t>(length);
    if (factory.isStatic)
      return reinterpret_cast<StaticCtor>(factory.pointer)(
          utf16.data(), 0, count, factory.method);
    return reinterpret_cast<InstanceCtor>(factory.pointer)(
        nullptr, utf16.data(), 0, count, factory.method);
  }
}
std::string ToUtf8(const Il2CppString *value) {
  const auto api = ApiSnapshot();
  if (!value)
    return {};
  const std::uint16_t *characters = nullptr;
  std::int32_t length = 0;
  if (api.stringChars && api.stringLength) {
    characters = api.stringChars(value);
    length = api.stringLength(value);
  } else {
    // Without the exports, read System.String's fixed runtime layout: the
    // object header, an int32 length at 0x10, then UTF-16 code units at 0x14.
    const auto *bytes = reinterpret_cast<const std::uint8_t *>(value);
    std::memcpy(&length, bytes + kObjectHeaderSize, sizeof(length));
    characters = reinterpret_cast<const std::uint16_t *>(
        bytes + kObjectHeaderSize + sizeof(length));
  }
  if (!characters || length < 0 || length > 100000000)
    return {};
  std::string result;
  result.reserve(static_cast<std::size_t>(length));
  for (std::int32_t index = 0; index < length; ++index) {
    std::uint32_t codepoint = characters[index];
    if (codepoint >= 0xd800 && codepoint <= 0xdbff && index + 1 < length) {
      const std::uint32_t low = characters[index + 1];
      if (low >= 0xdc00 && low <= 0xdfff) {
        codepoint = 0x10000 + ((codepoint - 0xd800) << 10) + (low - 0xdc00);
        ++index;
      }
    }
    if (codepoint >= 0xd800 && codepoint <= 0xdfff)
      codepoint = 0xfffd;
    AppendUtf8(result, codepoint);
  }
  return result;
}
} // namespace String

namespace GC {
Handle Pin(Il2CppObject *object) {
  const auto api = ApiSnapshot();
  if (!object || !api.gcHandleNew)
    return 0;
  return api.gcHandleNew(object, /*pinned=*/true);
}
Il2CppObject *GetTarget(Handle handle) {
  const auto api = ApiSnapshot();
  if (!handle || !api.gcHandleGetTarget)
    return nullptr;
  return api.gcHandleGetTarget(handle);
}
void Unpin(Handle handle) {
  const auto api = ApiSnapshot();
  if (handle && api.gcHandleFree)
    api.gcHandleFree(handle);
}
} // namespace GC

namespace Field {
std::size_t Offset(const FieldInfo *field) {
  const auto api = ApiSnapshot();
  if (field && api.fieldGetOffset)
    return api.fieldGetOffset(field);
  return 0;
}
std::size_t ValueOffset(const FieldInfo *field) {
  // A value type's field offsets include the boxed object header even though
  // an unboxed instance (an array element, a struct field) has none.
  const auto offset = Offset(field);
  return offset >= kObjectHeaderSize ? offset - kObjectHeaderSize : 0;
}
bool GetStatic(const FieldInfo *field, void *output) {
  if (!field || !output)
    return false;
  const auto api = ApiSnapshot();
  if (api.fieldStaticGetValue) {
    api.fieldStaticGetValue(field, output);
    return true;
  }
  return false;
}
bool SetStatic(FieldInfo *field, const void *value) {
  if (!field || !value)
    return false;
  const auto api = ApiSnapshot();
  if (api.fieldStaticSetValue) {
    api.fieldStaticSetValue(field, const_cast<void *>(value));
    return true;
  }
  return false;
}
bool GetInstance(Il2CppObject *object, const FieldInfo *field, void *output) {
  if (!object || !field || !output)
    return false;
  const auto api = ApiSnapshot();
  if (api.fieldGetValue) {
    api.fieldGetValue(object, field, output);
    return true;
  }
  return false;
}
bool SetInstance(Il2CppObject *object, FieldInfo *field, const void *value) {
  if (!object || !field || !value)
    return false;
  const auto api = ApiSnapshot();
  if (api.fieldSetValue) {
    api.fieldSetValue(object, field, const_cast<void *>(value));
    return true;
  }
  return false;
}
} // namespace Field
} // namespace iaot::il2cpp
#endif
