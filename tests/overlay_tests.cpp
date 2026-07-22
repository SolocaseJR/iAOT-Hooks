#include <IAOT/Overlay.hpp>
#include <cassert>

namespace {
void DrawNothing() {}
} // namespace

int main() {
  iaot::SetOverlayDrawCallback(&DrawNothing);
  assert(!iaot::StartOverlay());
  assert(!iaot::IsOverlayVisible());
  iaot::SetOverlayVisible(true);
  iaot::StopOverlay();
  return 0;
}
