# Theos Usage

The template compiles `runtime/src`, generated glue, and hook `.cpp`/`.mm` files into one arm64 dylib with C++17. Use `.mm` for Foundation, UIKit, Objective-C runtime, dyld, or Mach-O APIs; keep portable logic in `.cpp`.

Run `iaot generate hooks.yaml`, add new user files to `<Library>_FILES`, then run `make`. Include `runtime/include` and `generated`, enable ARC for Objective-C++, and retain `-ObjC`.

Generated registration is preferred to static auto-registration because linkers can omit unreferenced objects. The optional `IAOT_REGISTER_HOOK` macro exists but is less reliable with archives/dead stripping.

Theos only builds the dylib. On jailed iOS, embed it in an app you own or may test, arrange authorized loading, and sign/re-sign the whole package. IAOT is not a tweak-loader workflow.
