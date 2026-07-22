#include <IAOT/AppFunction.hpp>
#include <IAOT/Runtime.hpp>
#include <IAOT/SymbolResolver.hpp>
#if defined(__APPLE__)
#include <mach-o/loader.h>
#include <mach/mach.h>
#endif

namespace iaot {
Address Address::Runtime(std::uintptr_t v) {
  return {AddressKind::RuntimeAddress, v, nullptr};
}
Address Address::VmAddr(std::uintptr_t v) {
  return {AddressKind::VmAddr, v, nullptr};
}
Address Address::Rva(std::uintptr_t v) {
  return {AddressKind::Rva, v, nullptr};
}
Address Address::Symbol(const char *v) { return {AddressKind::Symbol, 0, v}; }
std::uintptr_t ResolveVmAddr(std::uintptr_t v) {
  return v ? static_cast<std::uintptr_t>(static_cast<std::intptr_t>(v) +
                                         GetMainImageSlide())
           : 0;
}
std::uintptr_t ResolveRva(std::uintptr_t v) {
  auto base = GetMainImageBase();
  return base ? base + v : 0;
}
std::uintptr_t ResolveSymbolAddress(const char *s) {
  return s ? ResolveExportedSymbol(s) : 0;
}
std::uintptr_t ResolveAddress(const Address &a) {
  switch (a.kind) {
  case AddressKind::RuntimeAddress:
    return a.value;
  case AddressKind::VmAddr:
    return ResolveVmAddr(a.value);
  case AddressKind::Rva:
    return ResolveRva(a.value);
  case AddressKind::Symbol:
    return ResolveSymbolAddress(a.symbol);
  }
  return 0;
}
bool IsAlignedInstructionAddress(std::uintptr_t v) {
  return v != 0 && (v & 3u) == 0;
}
bool IsAddressInMainImage(std::uintptr_t v) {
#if defined(__APPLE__)
  auto *h = GetMainMachHeader();
  if (!h || !v)
    return false;
  auto *p = reinterpret_cast<const std::uint8_t *>(h) + sizeof(*h);
  for (std::uint32_t i = 0; i < h->ncmds; ++i) {
    auto *lc = reinterpret_cast<const load_command *>(p);
    if (lc->cmdsize < sizeof(load_command))
      return false;
    if (lc->cmd == LC_SEGMENT_64) {
      auto *s = reinterpret_cast<const segment_command_64 *>(lc);
      auto start = static_cast<std::uintptr_t>(
          static_cast<std::intptr_t>(s->vmaddr) + GetMainImageSlide());
      if (v >= start && v < start + s->vmsize)
        return true;
    }
    p += lc->cmdsize;
  }
#else
  IAOT_UNUSED(v);
#endif
  return false;
}
bool IsExecutableAddress(std::uintptr_t v) {
#if defined(__APPLE__)
  vm_region_basic_info_data_64_t info{};
  mach_msg_type_number_t count = VM_REGION_BASIC_INFO_COUNT_64;
  mach_port_t object{};
  mach_vm_address_t address = v;
  mach_vm_size_t size{};
  return mach_vm_region(mach_task_self(), &address, &size,
                        VM_REGION_BASIC_INFO_64,
                        reinterpret_cast<vm_region_info_t>(&info), &count,
                        &object) == KERN_SUCCESS &&
         (info.protection & VM_PROT_EXECUTE) != 0;
#else
  return v != 0;
#endif
}
} // namespace iaot
