# Jailed / No-JIT Model

IAOT assumes a non-jailbroken device, no JIT, no unsigned executable memory, no native runtime compilation, no arbitrary process attachment, and no loading arbitrary new dylibs after signing. Hook bodies exist in signed executable pages before launch.

Runtime data can change a precompiled decision:

```cpp
std::string hkReturningSomeString() {
    if (IAOTGetFlag(IAOTFlag::override_return_string))
        return std::string(IAOTGetString(IAOTString::return_string_value));
    return std::string(OBF("compiled fallback"));
}
```

The selected bytes are data; the branch and return implementation are already compiled. IAOT never maps writable+executable pages or generates a new function on-device. A final app package must include all executable components and be signed/re-signed.
