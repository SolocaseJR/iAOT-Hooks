#include "IAOTGeneratedProject.hpp"
#include "UserFeatures.hpp"
#include <IAOT/IAOT.hpp>

#if IAOT_ENABLE_OVERLAY
#include <imgui.h>
#endif

namespace {

void DrawMockMenu() {
#if IAOT_ENABLE_OVERLAY
  if (ImGui::BeginTabBar("MockTelemetryTabs")) {
    if (ImGui::BeginTabItem("Account")) {
      bool disabled = MockFeatures::DisableTransport();
      if (ImGui::Checkbox("Disable Mock SendToSvr", &disabled)) {
        MockFeatures::SetDisableTransport(disabled);
#if IAOT_ENABLE_PERSISTENCE
        iaot::SavePersistentBool(
            "disable_mock_transport",
            static_cast<std::uint32_t>(IAOTFlag::disable_mock_transport));
#endif
      }
      ImGui::TextUnformatted(
          "Controls both synthetic transport callbacks in this demo.");
      ImGui::EndTabItem();
    }
    ImGui::EndTabBar();
  }
#endif
}

} // namespace

extern "C" bool IAOT_InstallMockUserMenu(const iaot::HookContext &) {
#if IAOT_ENABLE_PERSISTENCE
  iaot::LoadPersistentBool(
      "disable_mock_transport",
      static_cast<std::uint32_t>(IAOTFlag::disable_mock_transport),
      IAOTDefaultFlag_disable_mock_transport);
#endif
#if IAOT_ENABLE_OVERLAY
  iaot::SetOverlayDrawCallback(&DrawMockMenu);
#endif
  return true;
}
