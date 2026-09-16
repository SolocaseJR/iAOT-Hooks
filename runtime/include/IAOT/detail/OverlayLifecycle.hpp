#pragma once
#include <IAOT/Overlay.hpp>
#include <atomic>
#include <cstdint>

namespace iaot::detail {
// Generation and state share one atomic so stale work cannot publish a state
// after a newer Start/Stop request. No UIKit dependencies: used in host tests.
class OverlayLifecycle {
public:
  using Ticket = std::uint64_t;
  Ticket Begin(OverlayStatus status) {
    auto previous = state_.load();
    for (;;) {
      const auto next = ((previous & ~std::uint64_t{255}) + 256) |
                        static_cast<std::uint64_t>(status);
      if (state_.compare_exchange_weak(previous, next))
        return next >> 8;
    }
  }
  bool IsCurrent(Ticket ticket) const { return (state_.load() >> 8) == ticket; }
  bool Set(Ticket ticket, OverlayStatus status) {
    auto previous = state_.load();
    while ((previous >> 8) == ticket) {
      const auto next = (ticket << 8) | static_cast<std::uint64_t>(status);
      if (state_.compare_exchange_weak(previous, next))
        return true;
    }
    return false;
  }
  OverlayStatus Status() const {
    return static_cast<OverlayStatus>(state_.load() & 255);
  }
private:
  std::atomic<std::uint64_t> state_{static_cast<std::uint64_t>(OverlayStatus::Stopped)};
};
} // namespace iaot::detail
