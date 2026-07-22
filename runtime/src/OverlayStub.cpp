#include <IAOT/Logging.hpp>
#include <IAOT/Overlay.hpp>

#if !defined(IAOT_ENABLE_OVERLAY) || !IAOT_ENABLE_OVERLAY
namespace iaot {
namespace {
OverlayDrawFn g_drawCallback = nullptr;
}
bool StartOverlay(const OverlayOptions &options) {
  IAOT_UNUSED(options);
  Log(OBF("IAOT overlay is disabled in this build"));
  return false;
}
void StopOverlay() {}
void SetOverlayDrawCallback(OverlayDrawFn callback) {
  g_drawCallback = callback;
}
bool IsOverlayVisible() { return false; }
void SetOverlayVisible(bool visible) { IAOT_UNUSED(visible); }
} // namespace iaot
#endif
