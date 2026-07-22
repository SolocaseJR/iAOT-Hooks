# Typed Application and Original Calls

`AppFunction<Ret(Args...)>` describes an existing signed function by runtime address, static VM address plus main slide, main-image RVA, or exported symbol. `OriginalFunction<Signature>(slot)` reads a generated runtime trampoline address.

Calls return a default value (or no-op for `void`) when unresolved and can optionally validate executable memory, but no runtime check proves a C++ ABI. Member functions model `this` explicitly. Objective-C methods include `self` and `_cmd` and should normally use the Objective-C helpers.

Generated wrappers live in `IAOTGeneratedFunctions.hpp` and `IAOTGeneratedOriginals.hpp`.
