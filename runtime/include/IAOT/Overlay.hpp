#pragma once
#include <IAOT/Types.hpp>

namespace iaot {
enum class OverlayBackend : u32 {
  Auto = 0,
  Metal = 1,
};

using OverlayDrawFn = void (*)();

struct OverlayOptions {
  OverlayBackend backend{OverlayBackend::Auto};
  bool startMinimized{false};
  float minimizedDiameter{58.0f};
  const char *title{nullptr};
};

// Available when the Theos target is built with IAOT_ENABLE_OVERLAY=1.
IAOT_EXPORT bool StartOverlay(const OverlayOptions &options = {});
IAOT_EXPORT void StopOverlay();
IAOT_EXPORT void SetOverlayDrawCallback(OverlayDrawFn callback);
IAOT_EXPORT bool IsOverlayVisible();
IAOT_EXPORT void SetOverlayVisible(bool visible);
} // namespace iaot
