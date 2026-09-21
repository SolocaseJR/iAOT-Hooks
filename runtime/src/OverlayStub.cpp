#include <IAOT/Logging.hpp>
#include <IAOT/Overlay.hpp>

#if !defined(IAOT_ENABLE_OVERLAY) || !IAOT_ENABLE_OVERLAY
namespace iaot {
namespace {
OverlayDrawFn g_backgroundDrawCallback = nullptr;
OverlayDrawFn g_drawCallback = nullptr;
}
bool StartOverlay(const OverlayOptions &options) {
  IAOT_UNUSED(options);
  Log(OBF("IAOT overlay is disabled in this build"));
  return false;
}
void StopOverlay() {}
OverlayStatus GetOverlayStatus() { return OverlayStatus::Unavailable; }
void SetOverlayBackgroundDrawCallback(OverlayDrawFn callback) {
  g_backgroundDrawCallback = callback;
}
void SetOverlayDrawCallback(OverlayDrawFn callback) {
  g_drawCallback = callback;
}
bool IsOverlayVisible() { return false; }
bool IsOverlayMinimized() { return true; }
void SetOverlayMinimized(bool minimized) { IAOT_UNUSED(minimized); }
void SetOverlayVisible(bool visible) { IAOT_UNUSED(visible); }
void SetOverlayCaptureHidden(bool hidden) { IAOT_UNUSED(hidden); }
bool IsOverlayCaptureWindow(const void *window) {
  IAOT_UNUSED(window);
  return false;
}
void SetOverlayLauncherAppearance(u32 background, u32 accent, float opacity) {
  IAOT_UNUSED(background);
  IAOT_UNUSED(accent);
  IAOT_UNUSED(opacity);
}
void SetOverlayLauncherIconVariant(u32 variant) { IAOT_UNUSED(variant); }
} // namespace iaot
#endif
