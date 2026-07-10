#pragma once
#include <IAOT/Logging.hpp>
#include <cstdint>
#include <type_traits>
#include <utility>
#ifndef IAOT_ENABLE_CALL_VALIDATION
#define IAOT_ENABLE_CALL_VALIDATION 0
#endif
namespace iaot {
enum class AddressKind : std::uint32_t { RuntimeAddress=0, VmAddr=1, Rva=2, Symbol=3 };
struct Address {
    AddressKind kind{AddressKind::RuntimeAddress}; std::uintptr_t value{}; const char* symbol{};
    static Address Runtime(std::uintptr_t v); static Address VmAddr(std::uintptr_t v); static Address Rva(std::uintptr_t v); static Address Symbol(const char* v);
};
IAOT_EXPORT std::uintptr_t ResolveAddress(const Address& address);
IAOT_EXPORT std::uintptr_t ResolveVmAddr(std::uintptr_t address); IAOT_EXPORT std::uintptr_t ResolveRva(std::uintptr_t rva); IAOT_EXPORT std::uintptr_t ResolveSymbolAddress(const char* symbol);
IAOT_EXPORT bool IsExecutableAddress(std::uintptr_t address); IAOT_EXPORT bool IsAddressInMainImage(std::uintptr_t address); IAOT_EXPORT bool IsAlignedInstructionAddress(std::uintptr_t address);
template<typename Signature> class AppFunction;
template<typename Ret, typename... Args> class AppFunction<Ret(Args...)> {
public:
    using Fn = Ret (*)(Args...);
    AppFunction() = default; explicit AppFunction(Address address): address_(address) {}
    static AppFunction FromRuntime(std::uintptr_t v){ return AppFunction(Address::Runtime(v)); }
    static AppFunction FromVmAddr(std::uintptr_t v){ return AppFunction(Address::VmAddr(v)); }
    static AppFunction FromRva(std::uintptr_t v){ return AppFunction(Address::Rva(v)); }
    static AppFunction FromSymbol(const char* v){ return AppFunction(Address::Symbol(v)); }
    bool Resolve(){ resolved_ = ResolveAddress(address_); return resolved_ != 0; }
    bool IsResolved() const { return resolved_ != 0; }
    std::uintptr_t RuntimeAddress() const { return resolved_; }
    Fn Pointer() const { return reinterpret_cast<Fn>(resolved_); }
    Ret Call(Args... args) const {
        auto address = resolved_;
        if (!address) { address = ResolveAddress(address_); resolved_ = address; }
        if (!address) { Log(OBF("AppFunction call failed: unresolved address")); if constexpr (!std::is_void_v<Ret>) return Ret{}; else return; }
#if IAOT_ENABLE_CALL_VALIDATION
        if (!IsExecutableAddress(address)) { Log(OBF("AppFunction call failed: address is not executable")); if constexpr (!std::is_void_v<Ret>) return Ret{}; else return; }
#endif
        auto fn = reinterpret_cast<Fn>(address);
        if constexpr (std::is_void_v<Ret>) fn(std::forward<Args>(args)...); else return fn(std::forward<Args>(args)...);
    }
private: Address address_{}; mutable std::uintptr_t resolved_{};
};
}
#define IAOT_APP_FN(signature, vmaddr) iaot::AppFunction<signature>::FromVmAddr(vmaddr)
#define IAOT_APP_FN_RVA(signature, rva) iaot::AppFunction<signature>::FromRva(rva)
#define IAOT_APP_FN_SYMBOL(signature, symbol) iaot::AppFunction<signature>::FromSymbol(symbol)
