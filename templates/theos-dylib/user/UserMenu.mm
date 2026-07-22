#include "IAOTGeneratedProject.hpp"
#include "UserFeatures.hpp"
#include <IAOT/IAOT.hpp>
#if IAOT_ENABLE_OVERLAY
#include <imgui.h>
#endif

namespace {
void DrawMenu() {
#if IAOT_ENABLE_OVERLAY
  bool enabled = Features::ExampleHookEnabled();
  if (ImGui::Checkbox("Enable Example Hook", &enabled)) {
    Features::SetExampleHookEnabled(enabled);
#if IAOT_ENABLE_PERSISTENCE
    iaot::SavePersistentBool(
        "example_hook_enabled",
        static_cast<std::uint32_t>(IAOTFlag::example_hook_enabled));
#endif
  }
  ImGui::Separator();
  ImGui::Text("Framework: %s",
              iaot::IsInitialized() ? "ready" : "initializing");
  ImGui::Text("IL2CPP: %s",
              iaot::il2cpp::StatusName(iaot::il2cpp::GetStatus()));
  for (std::size_t index = 0; index < iaot::GetRegisteredHookCount(); ++index) {
    const auto diagnostic = iaot::GetHookDiagnostic(index);
    ImGui::BulletText("%s: %u", diagnostic.name ? diagnostic.name : "unknown",
                      static_cast<unsigned>(diagnostic.status));
  }
#endif
}
} // namespace

extern "C" bool IAOT_InstallUserMenu(const iaot::HookContext &) {
#if IAOT_ENABLE_PERSISTENCE
  iaot::LoadPersistentBool(
      "example_hook_enabled",
      static_cast<std::uint32_t>(IAOTFlag::example_hook_enabled),
      IAOTDefaultFlag_example_hook_enabled);
#endif
#if IAOT_ENABLE_OVERLAY
  iaot::SetOverlayDrawCallback(&DrawMenu);
#endif
  return true;
}
