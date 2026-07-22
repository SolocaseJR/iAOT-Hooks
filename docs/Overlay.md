# Minimal ImGui Overlay

The optional overlay creates a transparent alert-level `UIWindow` with its own `MTKView` and the official Dear ImGui Metal backend. It does not patch the host renderer; analysis still reports host Metal/OpenGL ES/Unity/UIKit/SceneKit/SpriteKit evidence.

The default template shows one persisted checkbox, framework/IL2CPP status, and per-hook diagnostics. It can minimize to a draggable circle. Touches outside active ImGui windows pass through. Initialization is dispatched to the main thread and waits for an active foreground `UIWindowScene`; failure to obtain Metal returns false without crashing the host.

Supported overlay backends are `auto`/`metal` and disabled. OpenGL ES is not added as a new overlay backend because Apple deprecated it; Vulkan is not native on iOS.
