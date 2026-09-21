#include <IAOT/Overlay.hpp>
#include <IAOT/detail/OverlayLifecycle.hpp>
#include <cassert>

namespace {
void DrawNothing() {}
} // namespace

int main() {
  iaot::detail::OverlayLifecycle lifecycle;
  assert(lifecycle.Status() == iaot::OverlayStatus::Stopped);
  const auto first = lifecycle.Begin(iaot::OverlayStatus::Starting);
  assert(lifecycle.Set(first, iaot::OverlayStatus::WaitingForScene));
  const auto stop = lifecycle.Begin(iaot::OverlayStatus::Stopping);
  assert(!lifecycle.IsCurrent(first));
  assert(!lifecycle.Set(first, iaot::OverlayStatus::Ready));
  assert(lifecycle.Set(stop, iaot::OverlayStatus::Stopped));
  const auto second = lifecycle.Begin(iaot::OverlayStatus::Starting);
  assert(!lifecycle.Set(stop, iaot::OverlayStatus::Stopped));
  assert(lifecycle.Set(second, iaot::OverlayStatus::Ready));
  assert(lifecycle.Status() == iaot::OverlayStatus::Ready);
  iaot::SetOverlayDrawCallback(&DrawNothing);
  iaot::SetOverlayBackgroundDrawCallback(&DrawNothing);
  iaot::SetOverlayLauncherAppearance(0, 0, 1.0f);
  iaot::SetOverlayLauncherIconVariant(1);
  assert(iaot::GetOverlayStatus() == iaot::OverlayStatus::Unavailable);
  assert(!iaot::StartOverlay());
  assert(!iaot::IsOverlayVisible());
  iaot::SetOverlayMinimized(false);
  assert(iaot::IsOverlayMinimized());
  iaot::OverlayOptions clientOptions;
  assert(!clientOptions.clientDrawsWindows);
  iaot::SetOverlayVisible(true);
  iaot::StopOverlay();
  return 0;
}
