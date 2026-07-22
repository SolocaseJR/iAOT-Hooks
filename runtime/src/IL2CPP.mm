#include <IAOT/IL2CPP.hpp>
#include <IAOT/Logging.hpp>

#if defined(__APPLE__)
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstring>
#include <dlfcn.h>
#include <mach-o/dyld.h>
#include <mutex>
#include <string>
#include <thread>

namespace iaot::il2cpp {
namespace {
template <typename T> T Symbol(const char *name) {
  return reinterpret_cast<T>(dlsym(RTLD_DEFAULT, name));
}

struct Api {
  Il2CppDomain *(*domainGet)(){};
  const Il2CppAssembly **(*domainGetAssemblies)(const Il2CppDomain *,
                                                std::size_t *){};
  const Il2CppImage *(*assemblyGetImage)(const Il2CppAssembly *){};
  const char *(*imageGetName)(const Il2CppImage *){};
  Il2CppClass *(*classFromName)(const Il2CppImage *, const char *,
                                const char *){};
  Il2CppClass *(*classGetNestedTypes)(Il2CppClass *, void **){};
  const char *(*classGetName)(Il2CppClass *){};
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

bool ResolveApi() {
  Api api{};
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
  api.classGetName =
      Symbol<decltype(api.classGetName)>("il2cpp_class_get_name");
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
  if (!api.domainGet || !api.domainGetAssemblies || !api.assemblyGetImage ||
      !api.imageGetName)
    return false;
  auto *domain = api.domainGet();
  if (!domain)
    return false;
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
  return true;
}

bool RequestedModuleLoaded() {
  if (g_requestedModule.empty())
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
      if (ResolveApi()) {
        Logf("IL2CPP resolver %s; module=%s capabilities=0x%llx",
             StatusName(GetStatus()), GetResolvedModule(),
             static_cast<unsigned long long>(GetCapabilities()));
        return;
      }
      lock.lock();
    }
    g_imageChanged.wait_for(lock, std::chrono::milliseconds(100));
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
Il2CppClass *FindClass(const char *image, const char *namespc,
                       const char *name) {
  if (!name)
    return nullptr;
  const auto *resolved = FindImage(image);
  const auto api = ApiSnapshot();
  return resolved && api.classFromName
             ? api.classFromName(resolved, namespc ? namespc : OBF(""), name)
             : nullptr;
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
const MethodInfo *FindMethod(Il2CppClass *klass, const char *name,
                             int parameterCount) {
  const auto api = ApiSnapshot();
  if (!klass || !name)
    return nullptr;
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
    return result;
  }
  return api.classGetMethodFromName
             ? api.classGetMethodFromName(klass, name, parameterCount)
             : nullptr;
}
FieldInfo *FindField(Il2CppClass *klass, const char *name) {
  const auto api = ApiSnapshot();
  return klass && name && api.classGetFieldFromName
             ? api.classGetFieldFromName(klass, name)
             : nullptr;
}
const PropertyInfo *FindProperty(Il2CppClass *klass, const char *name) {
  const auto api = ApiSnapshot();
  return klass && name && api.classGetPropertyFromName
             ? api.classGetPropertyFromName(klass, name)
             : nullptr;
}
std::uintptr_t NativeMethodPointer(const MethodInfo *method) {
  const auto adapter = g_methodPointerAdapter.load(std::memory_order_acquire);
  return method && adapter ? adapter(method) : 0;
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
  return name && api.resolveIcall
             ? reinterpret_cast<std::uintptr_t>(api.resolveIcall(name))
             : 0;
}

namespace String {
Il2CppString *New(const char *utf8) {
  const auto api = ApiSnapshot();
  return utf8 && api.stringNew ? api.stringNew(utf8) : nullptr;
}
std::string ToUtf8(const Il2CppString *value) {
  const auto api = ApiSnapshot();
  if (!value || !api.stringChars || !api.stringLength)
    return {};
  const auto *characters = api.stringChars(value);
  const auto length = api.stringLength(value);
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

namespace Field {
std::size_t Offset(const FieldInfo *field) {
  const auto api = ApiSnapshot();
  return field && api.fieldGetOffset ? api.fieldGetOffset(field) : 0;
}
bool GetStatic(const FieldInfo *field, void *output) {
  const auto api = ApiSnapshot();
  if (!field || !output || !api.fieldStaticGetValue)
    return false;
  api.fieldStaticGetValue(field, output);
  return true;
}
bool SetStatic(FieldInfo *field, const void *value) {
  const auto api = ApiSnapshot();
  if (!field || !value || !api.fieldStaticSetValue)
    return false;
  api.fieldStaticSetValue(field, const_cast<void *>(value));
  return true;
}
bool GetInstance(Il2CppObject *object, const FieldInfo *field, void *output) {
  const auto api = ApiSnapshot();
  if (!object || !field || !output || !api.fieldGetValue)
    return false;
  api.fieldGetValue(object, field, output);
  return true;
}
bool SetInstance(Il2CppObject *object, FieldInfo *field, const void *value) {
  const auto api = ApiSnapshot();
  if (!object || !field || !value || !api.fieldSetValue)
    return false;
  api.fieldSetValue(object, field, const_cast<void *>(value));
  return true;
}
} // namespace Field
} // namespace iaot::il2cpp
#endif
