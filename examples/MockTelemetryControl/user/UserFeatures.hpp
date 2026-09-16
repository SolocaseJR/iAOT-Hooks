#pragma once

#include "IAOTGeneratedConfig.hpp"

namespace MockFeatures {

inline bool DisableTransport() {
  return IAOTGetFlag(IAOTFlag::disable_mock_transport);
}

inline void SetDisableTransport(bool enabled) {
  IAOTSetFlag(IAOTFlag::disable_mock_transport, enabled);
}

} // namespace MockFeatures
