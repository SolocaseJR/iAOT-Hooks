# AppFunction

`AppFunction<Ret(Args...)>` turns an address description into a typed pointer and calls existing signed app code. It does not patch memory or bypass no-JIT rules.

- Runtime: already-slid live address.
- VM address: static address from a matching owned binary; runtime address is `vmaddr + dyld_slide`.
- RVA: `main_image_runtime_base + rva`.
- Symbol: exported-symbol lookup.

```cpp
auto freeFn = iaot::AppFunction<int()>::FromSymbol(OBF("OwnedFunction"));
auto member = iaot::AppFunction<int(void*, int)>::FromRva(0x1234);
int result = member.Call(self, 7); // model C++ `this` explicitly
```

Use Objective-C helpers or a correctly typed `objc_msgSend` form for Objective-C methods (`self` and `_cmd` are ABI parameters). The signature and address must exactly match. Non-trivial values such as `std::string` are ABI-sensitive; a compiler can handle them only when both sides truly share the same ABI/signature. A mismatch can crash immediately. Optional call validation is diagnostic, not proof of signature correctness.
