# Architecture

IAOT separates executable behavior from runtime-selectable data.

```text
hooks.yaml -> iaot generate -> generated C++ glue
                                  |
runtime C++/ObjC++ + hooks .mm/.cpp + glue -> Theos/host build -> signed code
hooks.yaml + owned Mach-O -> conservative offline patcher -> staged Mach-O -> sign package
```

The source-distributed runtime owns config access, image context, explicit hook registration, Objective-C replacement, and typed calls into existing signed code. User modules contain all app-specific behavior. Generated glue gives stable indices, typed address wrappers, and explicit registration so dead stripping/static-library constructor ordering is not relied upon.

Rust is optional tooling only. It never supplies runtime hook behavior. The offline writer creates only audited ARM64 cave templates; arbitrary instruction/code strings are outside the model. It supports conservative thin ARM64 executable mutation and fails closed when the existing load-command area cannot hold the new segment commands.
