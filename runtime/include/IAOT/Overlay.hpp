#pragma once
#include <IAOT/Types.hpp>

namespace iaot {
enum class OverlayBackend : u32 {
  Auto = 0,
  Metal = 1,
};

using OverlayDrawFn = void (*)();

enum class OverlayStatus : u32 {
  Stopped, Starting, WaitingForScene, Ready, Stopping, Failed, Unavailable
};

struct OverlayOptions {
  OverlayBackend backend{OverlayBackend::Auto};
  bool startMinimized{false};
  float minimizedDiameter{58.0f};
  // Optional launcher artwork in packed 0xRRGGBBAA format. StartOverlay copies
  // the pixels before returning; a supplied image is limited to 1024x1024.
  const u32 *launcherIconRgba{nullptr};
  u32 launcherIconWidth{0};
  u32 launcherIconHeight{0};
  const u32 *launcherAlternateIconRgba{nullptr};
  u32 launcherAlternateIconWidth{0};
  u32 launcherAlternateIconHeight{0};
  const char *title{nullptr};
};

// Available when the Theos target is built with IAOT_ENABLE_OVERLAY=1.
// True means accepted, not rendered. GetOverlayStatus distinguishes deferred
// startup from a ready window. Concurrent requests use latest-request-wins.
IAOT_EXPORT bool StartOverlay(const OverlayOptions &options = {});
IAOT_EXPORT OverlayStatus GetOverlayStatus();
// Invalidates queued startup immediately. UIKit teardown runs on the main
// queue (synchronously when called on that queue).
IAOT_EXPORT void StopOverlay();
// Invoked every overlay frame before the minimized/menu UI path. Use this
// for non-interactive foreground/background drawing that must remain visible
// while the menu launcher is minimized.
IAOT_EXPORT void SetOverlayBackgroundDrawCallback(OverlayDrawFn callback);
IAOT_EXPORT void SetOverlayDrawCallback(OverlayDrawFn callback);
// RGBA values are packed as 0xRRGGBBAA. Opacity only affects the launcher
// plate; accent controls its themed rim.
IAOT_EXPORT void SetOverlayLauncherAppearance(u32 backgroundRgba,
                                              u32 accentRgba,
                                              float opacity);
// Selects the primary (0) or alternate (1) launcher icon.
IAOT_EXPORT void SetOverlayLauncherIconVariant(u32 variant);
IAOT_EXPORT bool IsOverlayVisible();
IAOT_EXPORT void SetOverlayVisible(bool visible);
// Temporarily excludes framework-owned windows from a synchronous host capture
// without changing the user's visible/minimized preference. Safe from either
// the main thread or a capture worker.
IAOT_EXPORT void SetOverlayCaptureHidden(bool hidden);
// True only for one of the framework-owned render/input windows. Capture
// adapters can use this to filter candidates without mutating UIKit state.
IAOT_EXPORT bool IsOverlayCaptureWindow(const void *window);
} // namespace iaot
