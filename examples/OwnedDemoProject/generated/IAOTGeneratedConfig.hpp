#pragma once
#include <IAOT/IAOT.hpp>
#include <cstdint>

enum class IAOTFlag : std::uint32_t {
    override_return_string = 0,
    enable_objc_demo_hook = 1,
};
enum class IAOTString : std::uint32_t {
    return_string_value = 0,
};
enum class IAOTCallback : std::uint32_t {
    demo_callback = 0,
};
inline constexpr bool IAOTDefaultFlag_override_return_string = false;
inline constexpr bool IAOTDefaultFlag_enable_objc_demo_hook = true;
inline constexpr const char* IAOTDefaultString_return_string_value = OBF("runtime default string");
inline constexpr const char* IAOTCallbackSymbol_demo_callback = OBF("IAOT_DemoCallback");
inline bool IAOTGetFlag(IAOTFlag v){return iaot::GetBool(static_cast<std::uint32_t>(v));}
inline void IAOTSetFlag(IAOTFlag v,bool x){iaot::SetBool(static_cast<std::uint32_t>(v),x);}
inline const char* IAOTGetString(IAOTString v){return iaot::GetString(static_cast<std::uint32_t>(v));}
inline void IAOTSetString(IAOTString v,const char* x){iaot::SetString(static_cast<std::uint32_t>(v),x);}
inline std::uintptr_t IAOTGetCallback(IAOTCallback v){return iaot::GetCallback(static_cast<std::uint32_t>(v));}
inline void IAOTSetCallback(IAOTCallback v,std::uintptr_t x){iaot::SetCallback(static_cast<std::uint32_t>(v),x);}
