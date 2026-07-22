# Generic Hook Patterns

These fragments use placeholder ABIs and addresses. Replace them only with authorized, version-matched information.

```cpp
// 1. Constant return
extern "C" int Constant(void*) { return 7; }

// 2. Void early return
extern "C" void Ignore(void*) { return; }

// 3. Call original
extern "C" int Pass(void* self, int v) { return iaot::generated::Original_Pass().Call(self, v); }

// 4. Modify result
extern "C" int AddOne(void* self) { return iaot::generated::Original_AddOne().Call(self) + 1; }

// 5. Modify argument
extern "C" int ChangeArg(void* self, int v) { return iaot::generated::Original_ChangeArg().Call(self, 123); }

// 6. Conditionally block
extern "C" bool Allow(void* self) { return Features::Enabled() ? false : iaot::generated::Original_Allow().Call(self); }

// 7. Toggle/menu: use a local bool because config storage is atomic u32.
bool enabled = Features::Enabled();
if (ImGui::Checkbox("Enable Example", &enabled)) Features::SetEnabled(enabled);

// 12. Runtime IL2CPP method call
auto* type = iaot::il2cpp::FindClass("Assembly-CSharp", "Example", "Service");
auto* method = iaot::il2cpp::FindMethod(type, "Refresh", 0);
iaot::il2cpp::Invoke(method, instance);

// 13/14. Read/create IL2CPP strings
std::string native = iaot::il2cpp::String::ToUtf8(managed);
auto* replacement = iaot::il2cpp::String::New("Example");

// 15. Read a managed field
auto* field = iaot::il2cpp::FindField(type, "score");
int score = 0;
iaot::il2cpp::Field::GetInstance(instance, field, &score);
```

8. Native signature locator:

```yaml
locator: { type: pattern, pattern: "FD 7B ?? A9 F4 4F ?? A9", expected_matches: 1, section: "__text" }
```

9. Virtual address locator:

```yaml
locator: { type: vmaddr, value: 4296015872 }
```

10. Objective-C selector (registered automatically from the manifest):

```yaml
functions:
  - { name: ExampleTitle, kind: objc, class: ExampleController, selector: title, class_method: false, signature: "id(id, SEL)", replacement: IAOT_ExampleTitle, preserve_original: true }
```

```cpp
extern "C" id IAOT_ExampleTitle(id self, SEL command) {
    return iaot::generated::Original_ExampleTitle().Call(self, command);
}
```

11. Static IL2CPP locator:

```yaml
locator:
  type: il2cpp
  assembly: Assembly-CSharp
  namespace: Example
  class: Player
  method: SetName
  parameter_count: 1
```

16. Missing target: use `failure_behavior: disable_hook`; the reason appears in final validation.

17. Ambiguous pattern: omit `match_index` during discovery so the build fails, strengthen the pattern, and use an index only when the alternatives are intentionally equivalent.

18. Rendering override:

```yaml
analysis: { rendering_backend: metal }
```

19. Windows/WSL build:

```powershell
.\scripts\build-iaot.ps1 -InputIpa ".\Owned-Decrypted.ipa" -Manifest ".\MyHooks\hooks.yaml"
```

20. Final IPA:

```bash
iaot pipeline --manifest hooks.yaml --in Owned-Decrypted.ipa --out build/Owned-IAOT-unsigned.ipa --dylib build/MyHooks.dylib --reports build/reports
```
