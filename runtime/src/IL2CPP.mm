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
#include <memory>
#include <mutex>
#include <string>
#include <thread>
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
  const MethodInfo *(*classGetMethods)(Il2CppClass *, void **){};
  const MethodInfo *(*classGetMethodFromName)(Il2CppClass *, const char *,
                                              int){};
  const char *(*methodGetName)(const MethodInfo *){};
  std::uint32_t (*methodGetParamCount)(const MethodInfo *){};
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
using StaticStringFactory = Il2CppString *(*)(void *, const std::uint16_t *,
                                              const MethodInfo *);
std::atomic<StaticStringFactory> g_staticStringFactory{nullptr};
std::uintptr_t g_staticStringFactoryRva = 0;
const OfflineMetadata *g_offlineMetadata = nullptr;

constexpr std::uint64_t kOfflineClassMagic = 0x49414F54434C4153ULL;
constexpr std::uint64_t kOfflineMethodMagic = 0x49414F544D455448ULL;
constexpr std::uint64_t kOfflineFieldMagic = 0x49414F544649454CULL;
struct OfflineClassHandle {
  std::uint64_t magic{kOfflineClassMagic};
  const OfflineClassBinding *binding{};
};
struct OfflineMethodHandle {
  std::uint64_t magic{kOfflineMethodMagic};
  const OfflineMethodBinding *binding{};
};
struct OfflineFieldHandle {
  std::uint64_t magic{kOfflineFieldMagic};
  const OfflineFieldBinding *binding{};
};
std::mutex g_offlineMutex;
std::vector<std::unique_ptr<OfflineClassHandle>> g_offlineClasses;
std::vector<std::unique_ptr<OfflineMethodHandle>> g_offlineMethods;
std::vector<std::unique_ptr<OfflineFieldHandle>> g_offlineFields;
std::once_flag g_startOnce;
std::mutex g_apiMutex;
std::mutex g_imageMutex;
std::condition_variable g_imageChanged;
std::string g_moduleName;
std::string g_requestedModule;
double g_timeout = 20.0;

bool ResolveOfflineMetadata();

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
  api.classGetMethods =
      Symbol<decltype(api.classGetMethods)>("il2cpp_class_get_methods");
  api.classGetMethodFromName = Symbol<decltype(api.classGetMethodFromName)>(
      "il2cpp_class_get_method_from_name");
  api.methodGetName =
      Symbol<decltype(api.methodGetName)>("il2cpp_method_get_name");
  api.methodGetParamCount = Symbol<decltype(api.methodGetParamCount)>(
      "il2cpp_method_get_param_count");
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
  std::uint64_t capabilities = Assemblies;
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
  if (api.stringNew && api.stringChars && api.stringLength)
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
    if (g_methodPointerAdapter.load(std::memory_order_acquire))
      capabilities |= NativeMethodPointers;
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

std::uintptr_t RequestedModuleBase() {
  if (g_requestedModule.empty() || g_requestedModule == "main") {
    return _dyld_image_count()
               ? reinterpret_cast<std::uintptr_t>(_dyld_get_image_header(0))
               : 0;
  }
  return FindImageBaseByName(g_requestedModule.c_str());
}

void InitializeWorker() {
  const auto deadline = std::chrono::steady_clock::now() +
                        std::chrono::milliseconds(static_cast<long long>(
                            g_timeout > 0 ? g_timeout * 1000.0 : 1.0));
  g_status.store(Status::WaitingForModule, std::memory_order_release);
  std::unique_lock<std::mutex> lock(g_imageMutex);
  while (std::chrono::steady_clock::now() < deadline) {
    if (g_staticStringFactoryRva &&
        !g_staticStringFactory.load(std::memory_order_acquire)) {
      const auto base = RequestedModuleBase();
      if (base) {
        g_staticStringFactory.store(
            reinterpret_cast<StaticStringFactory>(base + g_staticStringFactoryRva),
            std::memory_order_release);
        Logf("IL2CPP static UTF-16 string factory configured at +0x%llx",
             static_cast<unsigned long long>(g_staticStringFactoryRva));
      }
    }
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
      // Exports exist but IL2CPP is still booting: keep waiting rather than
      // locking in the offline-only resolver.
      if (result == ApiResult::Unavailable && ResolveOfflineMetadata()) {
        Logf("IL2CPP offline metadata resolver active; module=%s capabilities=0x%llx",
             GetResolvedModule(),
             static_cast<unsigned long long>(GetCapabilities()));
        return;
      }
      lock.lock();
    }
    g_imageChanged.wait_for(lock, std::chrono::milliseconds(100));
  }
  lock.unlock();
  if (ResolveOfflineMetadata()) {
    Logf("IL2CPP runtime not initialized before timeout; offline metadata "
         "resolver active; module=%s",
         GetResolvedModule());
    return;
  }
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

bool OfflineModuleMatches(const char *binding, const char *requested) {
  if (!binding || !requested)
    return false;
  if (SameImageName(binding, requested))
    return true;
  return std::strcmp(requested, "main") == 0 &&
         (g_requestedModule.empty() ||
          SameImageName(binding, g_requestedModule.c_str()));
}

std::uintptr_t OfflineModuleBase(const char *module) {
  if (!module || !*module || std::strcmp(module, "main") == 0)
    return RequestedModuleBase();
  if (const auto base = FindImageBaseByName(module))
    return base;
  return RequestedModuleBase();
}

bool ResolveOfflineMetadata() {
  if (!g_offlineMetadata && !g_staticStringFactoryRva)
    return false;

  std::uint64_t capabilities = 0;
  if (g_offlineMetadata) {
    if (g_offlineMetadata->classCount)
      capabilities |= Classes;
    if (g_offlineMetadata->methodCount)
      capabilities |= Methods | NativeMethodPointers;
    if (g_offlineMetadata->fieldCount)
      capabilities |= Fields;
    if (g_offlineMetadata->internalCallCount)
      capabilities |= InternalCalls;
  }
  if (g_staticStringFactoryRva)
    capabilities |= Strings;
  if (!capabilities)
    return false;

  g_moduleName = g_requestedModule;
  g_capabilities.store(capabilities, std::memory_order_release);
  g_apiPublished.store(true, std::memory_order_release);
  g_status.store(Status::PartiallyAvailable, std::memory_order_release);
  return true;
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
} // namespace

bool InitializeAsync(const Options &options) {
  std::call_once(g_startOnce, [&] {
    if (options.moduleName)
      g_requestedModule = options.moduleName;
    g_timeout = options.timeoutSeconds;
    g_staticStringFactoryRva = options.stringFactoryRva;
    g_offlineMetadata = options.offlineMetadata;
    if (g_staticStringFactoryRva) {
      if (const auto base = RequestedModuleBase()) {
        g_staticStringFactory.store(
            reinterpret_cast<StaticStringFactory>(base + g_staticStringFactoryRva),
            std::memory_order_release);
      }
    }
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
std::size_t GetOfflineClassCount() {
  return g_offlineMetadata ? g_offlineMetadata->classCount : 0;
}
bool GetOfflineClassAt(std::size_t index, const char **module,
                       const char **namespc, const char **name) {
  if (!g_offlineMetadata || index >= g_offlineMetadata->classCount)
    return false;
  const auto &binding = g_offlineMetadata->classes[index];
  if (module) *module = binding.module;
  if (namespc) *namespc = binding.namespc;
  if (name) *name = binding.name;
  return true;
}
void SetMethodPointerAdapter(MethodPointerAdapter adapter) {
  std::lock_guard<std::mutex> lock(g_apiMutex);
  g_methodPointerAdapter.store(adapter, std::memory_order_release);
  if (adapter)
    g_capabilities.fetch_or(NativeMethodPointers, std::memory_order_acq_rel);
  else
    g_capabilities.fetch_and(~static_cast<std::uint64_t>(NativeMethodPointers),
                             std::memory_order_acq_rel);
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

OfflineClassHandle *OfflineClass(Il2CppClass *klass) {
  std::lock_guard<std::mutex> lock(g_offlineMutex);
  for (const auto &candidate : g_offlineClasses)
    if (reinterpret_cast<Il2CppClass *>(candidate.get()) == klass)
      return candidate.get();
  return nullptr;
}

OfflineMethodHandle *OfflineMethod(const MethodInfo *method) {
  std::lock_guard<std::mutex> lock(g_offlineMutex);
  for (const auto &candidate : g_offlineMethods)
    if (reinterpret_cast<const MethodInfo *>(candidate.get()) == method)
      return candidate.get();
  return nullptr;
}

OfflineFieldHandle *OfflineField(const FieldInfo *field) {
  std::lock_guard<std::mutex> lock(g_offlineMutex);
  for (const auto &candidate : g_offlineFields)
    if (reinterpret_cast<const FieldInfo *>(candidate.get()) == field)
      return candidate.get();
  return nullptr;
}

const OfflineClassBinding *FindOfflineClassBinding(const char *image,
                                                   const char *namespc,
                                                   const char *name) {
  if (!g_offlineMetadata || !image || !name)
    return nullptr;
  for (std::size_t index = 0; index < g_offlineMetadata->classCount; ++index) {
    const auto &binding = g_offlineMetadata->classes[index];
    if (OfflineModuleMatches(binding.module, image) &&
        std::strcmp(binding.namespc ? binding.namespc : "",
                    namespc ? namespc : "") == 0 &&
        std::strcmp(binding.name ? binding.name : "", name) == 0)
      return &binding;
  }
  return nullptr;
}

Il2CppClass *FindClass(const char *image, const char *namespc,
                       const char *name) {
  if (!name)
    return nullptr;
  // Prefer a real runtime class whenever IL2CPP metadata is available.
  // Offline handles are address-resolution fallbacks and must never shadow a
  // real class because they cannot be passed to il2cpp_runtime_invoke.
  const auto *resolved = FindImage(image);
  const auto api = ApiSnapshot();
  if (resolved && api.classFromName)
    if (auto *runtimeClass =
            api.classFromName(resolved, namespc ? namespc : OBF(""), name))
      return runtimeClass;

  if (const auto *binding = FindOfflineClassBinding(image, namespc, name)) {
    std::lock_guard<std::mutex> lock(g_offlineMutex);
    for (const auto &candidate : g_offlineClasses)
      if (candidate->binding == binding)
        return reinterpret_cast<Il2CppClass *>(candidate.get());
    auto candidate = std::make_unique<OfflineClassHandle>();
    candidate->binding = binding;
    auto *result = reinterpret_cast<Il2CppClass *>(candidate.get());
    g_offlineClasses.push_back(std::move(candidate));
    return result;
  }
  return nullptr;
}
Il2CppClass *FindNestedClass(Il2CppClass *parent, const char *name) {
  const auto api = ApiSnapshot();
  if (!parent || !name || OfflineClass(parent) || !api.classGetNestedTypes ||
      !api.classGetName)
    return nullptr;
  void *iterator = nullptr;
  while (auto *nested = api.classGetNestedTypes(parent, &iterator))
    if (const char *actual = api.classGetName(nested);
        actual && std::strcmp(actual, name) == 0)
      return nested;
  return nullptr;
}
Il2CppClass *GetParentClass(Il2CppClass *klass) {
  if (!klass || OfflineClass(klass))
    return nullptr;
  const auto api = ApiSnapshot();
  return api.classGetParent ? api.classGetParent(klass) : nullptr;
}
const char *ClassName(Il2CppClass *klass) {
  if (!klass || OfflineClass(klass))
    return nullptr;
  const auto api = ApiSnapshot();
  return api.classGetName ? api.classGetName(klass) : nullptr;
}
// Returns the handle for an audited offline method binding. A null `module`
// matches any module (used when the class came from the live runtime API).
const MethodInfo *FindOfflineMethod(const char *module, const char *namespc,
                                    const char *klass, const char *name,
                                    int parameterCount) {
  if (!g_offlineMetadata || !klass || !name)
    return nullptr;
  std::lock_guard<std::mutex> lock(g_offlineMutex);
  for (std::size_t index = 0; index < g_offlineMetadata->methodCount; ++index) {
    const auto &binding = g_offlineMetadata->methods[index];
    if ((module && std::strcmp(binding.module ? binding.module : "", module) != 0) ||
        std::strcmp(binding.namespc ? binding.namespc : "",
                    namespc ? namespc : "") != 0 ||
        std::strcmp(binding.klass ? binding.klass : "", klass) != 0 ||
        std::strcmp(binding.name ? binding.name : "", name) != 0 ||
        (parameterCount >= 0 && binding.parameterCount >= 0 &&
         binding.parameterCount != parameterCount))
      continue;
    for (const auto &candidate : g_offlineMethods)
      if (candidate->binding == &binding)
        return reinterpret_cast<const MethodInfo *>(candidate.get());
    auto candidate = std::make_unique<OfflineMethodHandle>();
    candidate->binding = &binding;
    auto *result = reinterpret_cast<const MethodInfo *>(candidate.get());
    g_offlineMethods.push_back(std::move(candidate));
    return result;
  }
  return nullptr;
}

const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                              int parameterCount) {
  if (const auto *offlineClass = OfflineClass(klass);
      offlineClass && g_offlineMetadata && name) {
    const auto *binding = offlineClass->binding;
    return FindOfflineMethod(binding->module ? binding->module : "",
                             binding->namespc, binding->name, name,
                             parameterCount);
  }
  const auto api = ApiSnapshot();
  if (!klass || !name)
    return nullptr;
  // A live runtime MethodInfo has no native pointer without an adapter, and
  // the runtime cannot tell same-arity overloads apart. When the project
  // declared an audited binding for this exact class/method/arity, use it.
  if (g_offlineMetadata && api.classGetName && api.classGetNamespace &&
      !g_methodPointerAdapter.load(std::memory_order_acquire))
    if (const auto *offline = FindOfflineMethod(
            nullptr, api.classGetNamespace(klass), api.classGetName(klass),
            name, parameterCount))
      return offline;
  if (api.classGetMethods && api.methodGetName && api.methodGetParamCount) {
    void *iterator = nullptr;
    const MethodInfo *result = nullptr;
    std::size_t visited = 0;
    while (const auto *candidate = api.classGetMethods(klass, &iterator)) {
      if (++visited > 100000)
        return nullptr;
      const char *candidateName = api.methodGetName(candidate);
      const bool countMatches =
          parameterCount < 0 || api.methodGetParamCount(candidate) ==
                                    static_cast<std::uint32_t>(parameterCount);
      if (candidateName && std::strcmp(candidateName, name) == 0 &&
          countMatches) {
        if (result) {
          Logf("IL2CPP method is ambiguous: %s/%d", name, parameterCount);
          return nullptr;
        }
        result = candidate;
      }
    }
    if (result)
      return result;
    // Unity's method iterator only enumerates methods declared on the class.
    // The named lookup can also resolve inherited methods, including generic
    // ClientSystem<T>.get_Instance accessors used by StageManager.
    if (api.classGetMethodFromName)
      return api.classGetMethodFromName(klass, name, parameterCount);
    return nullptr;
  }
  return api.classGetMethodFromName
             ? api.classGetMethodFromName(klass, name, parameterCount)
             : nullptr;
}
FieldInfo *FindField(Il2CppClass *klass, const char *name) {
  if (const auto *offlineClass = OfflineClass(klass);
      offlineClass && g_offlineMetadata && name) {
    std::lock_guard<std::mutex> lock(g_offlineMutex);
    for (std::size_t index = 0; index < g_offlineMetadata->fieldCount;
         ++index) {
      const auto &binding = g_offlineMetadata->fields[index];
      if (std::strcmp(binding.module ? binding.module : "",
                      offlineClass->binding->module
                          ? offlineClass->binding->module
                          : "") != 0 ||
          std::strcmp(binding.namespc ? binding.namespc : "",
                      offlineClass->binding->namespc
                          ? offlineClass->binding->namespc
                          : "") != 0 ||
          std::strcmp(binding.klass ? binding.klass : "",
                      offlineClass->binding->name
                          ? offlineClass->binding->name
                          : "") != 0 ||
          std::strcmp(binding.name ? binding.name : "", name) != 0)
        continue;
      for (const auto &candidate : g_offlineFields)
        if (candidate->binding == &binding)
          return reinterpret_cast<FieldInfo *>(candidate.get());
      auto candidate = std::make_unique<OfflineFieldHandle>();
      candidate->binding = &binding;
      auto *result = reinterpret_cast<FieldInfo *>(candidate.get());
      g_offlineFields.push_back(std::move(candidate));
      return result;
    }
  }
  const auto api = ApiSnapshot();
  return klass && name && !OfflineClass(klass) && api.classGetFieldFromName
             ? api.classGetFieldFromName(klass, name)
             : nullptr;
}
const PropertyInfo *FindProperty(Il2CppClass *klass, const char *name) {
  const auto api = ApiSnapshot();
  return klass && name && !OfflineClass(klass) && api.classGetPropertyFromName
             ? api.classGetPropertyFromName(klass, name)
             : nullptr;
}
std::uintptr_t NativeMethodPointer(const MethodInfo *method) {
  const auto adapter = g_methodPointerAdapter.load(std::memory_order_acquire);
  if (method && adapter)
    if (const auto address = adapter(method))
      return address;
  if (const auto *offline = OfflineMethod(method)) {
    const auto *binding = offline->binding;
    const auto base = binding ? OfflineModuleBase(binding->module) : 0;
    return base && binding ? base + binding->rva : 0;
  }
  return 0;
}
std::uintptr_t OfflineMethodPointer(const char *assemblyOrImageName,
                                    const char *namespc, const char *klass,
                                    const char *name, int parameterCount) {
  if (!g_offlineMetadata || !assemblyOrImageName || !klass || !name)
    return 0;
  const char *wantedNamespace = namespc ? namespc : "";
  for (std::size_t index = 0; index < g_offlineMetadata->methodCount; ++index) {
    const auto &binding = g_offlineMetadata->methods[index];
    if (!OfflineModuleMatches(binding.module, assemblyOrImageName) ||
        std::strcmp(binding.namespc ? binding.namespc : "",
                    wantedNamespace) != 0 ||
        std::strcmp(binding.klass ? binding.klass : "", klass) != 0 ||
        std::strcmp(binding.name ? binding.name : "", name) != 0 ||
        (parameterCount >= 0 && binding.parameterCount >= 0 &&
         binding.parameterCount != parameterCount))
      continue;
    const auto base = OfflineModuleBase(binding.module);
    return base ? base + binding.rva : 0;
  }
  return 0;
}
Il2CppObject *Invoke(const MethodInfo *method, Il2CppObject *instance,
                     void **arguments, Il2CppException **exception) {
  const auto api = ApiSnapshot();
  if (!method || !api.runtimeInvoke)
    return nullptr;
  if (OfflineMethod(method)) {
    Logf("refusing to runtime-invoke an offline method handle");
    return nullptr;
  }
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
    if (const auto address = reinterpret_cast<std::uintptr_t>(api.resolveIcall(name)))
      return address;
  if (name && g_offlineMetadata)
    for (std::size_t index = 0; index < g_offlineMetadata->internalCallCount;
         ++index) {
      const auto &binding = g_offlineMetadata->internalCalls[index];
      if (binding.name && std::strcmp(binding.name, name) == 0) {
        const auto base = OfflineModuleBase(binding.module);
        return base ? base + binding.rva : 0;
      }
    }
  return 0;
}

namespace String {
Il2CppString *New(const char *utf8) {
  const auto api = ApiSnapshot();
  if (!utf8)
    return nullptr;
  if (api.stringNew)
    return api.stringNew(utf8);

  const auto factory = g_staticStringFactory.load(std::memory_order_acquire);
  if (!factory)
    return nullptr;

  @autoreleasepool {
    NSString *value = [NSString stringWithUTF8String:utf8];
    if (!value)
      return nullptr;
    const NSUInteger length = value.length;
    std::vector<std::uint16_t> utf16(length + 1, 0);
    [value getCharacters:reinterpret_cast<unichar *>(utf16.data())
                    range:NSMakeRange(0, length)];
    return factory(nullptr, utf16.data(), nullptr);
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
    // Hardened targets (e.g. this game) ship with the il2cpp C exports stripped,
    // so il2cpp_string_chars/il2cpp_string_length resolve to null and the API
    // path above is unavailable. The managed System.String memory layout is
    // stable across every il2cpp version: an Il2CppObject header (klass +
    // monitor = 16 bytes on 64-bit), an int32 length at offset 0x10, then the
    // UTF-16 code units at offset 0x14. Read it directly so text reads still
    // work without the runtime API. This is the only way to recover string
    // contents when the exports are absent.
    const auto *bytes = reinterpret_cast<const std::uint8_t *>(value);
    std::memcpy(&length, bytes + 16, sizeof(length));
    characters = reinterpret_cast<const std::uint16_t *>(bytes + 20);
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
// Offline handles are synthetic and must never reach the live il2cpp_field_*
// API: it would read their bytes as a FieldInfo. Check them first.
std::size_t Offset(const FieldInfo *field) {
  if (const auto *offline = OfflineField(field))
    return offline->binding ? offline->binding->offset : 0;
  const auto api = ApiSnapshot();
  if (field && api.fieldGetOffset)
    return api.fieldGetOffset(field);
  return 0;
}
bool GetStatic(const FieldInfo *field, void *output) {
  if (!field || !output || OfflineField(field))
    return false;
  const auto api = ApiSnapshot();
  if (api.fieldStaticGetValue) {
    api.fieldStaticGetValue(field, output);
    return true;
  }
  return false;
}
bool SetStatic(FieldInfo *field, const void *value) {
  if (!field || !value || OfflineField(field))
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
  if (const auto *offline = OfflineField(field)) {
    if (!offline->binding)
      return false;
    std::memcpy(output,
                reinterpret_cast<const std::uint8_t *>(object) +
                    offline->binding->offset,
                offline->binding->size);
    return true;
  }
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
  if (const auto *offline = OfflineField(field)) {
    if (!offline->binding)
      return false;
    std::memcpy(reinterpret_cast<std::uint8_t *>(object) +
                    offline->binding->offset,
                value, offline->binding->size);
    return true;
  }
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
