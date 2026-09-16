#include "IAOTGeneratedOriginals.hpp"
#include "UserFeatures.hpp"
#include <IAOT/IAOT.hpp>
#include <cstdint>

// These callbacks operate only on the synthetic MockAnoSDK framework declared
// in this example. They model native wrappers with the ABI `int64_t(int)` and
// return zero without entering the mock SDK's lower-level dispatch routine.
extern "C" std::int64_t IAOT_MockAnoSDKGetReportData(int) { return 0; }
extern "C" std::int64_t IAOT_MockAnoSDKGetReportData2(int) { return 0; }
extern "C" std::int64_t IAOT_MockAnoSDKGetReportData3(int) { return 0; }
extern "C" std::int64_t IAOT_MockAnoSDKGetReportData4(int) { return 0; }

extern "C" void IAOT_MockEnableGameReport(const void *) {}

extern "C" void IAOT_MockSendDataToServer(void *instance,
                                            const void *methodInfo) {
  if (MockFeatures::DisableTransport())
    return;

  iaot::generated::Original_MockSendDataToServer().Call(instance, methodInfo);
}

extern "C" void IAOT_MockSendDiagnosticInfo(void *instance,
                                              const void *methodInfo) {
  if (MockFeatures::DisableTransport())
    return;

  iaot::generated::Original_MockSendDiagnosticInfo().Call(instance, methodInfo);
}

extern "C" bool IAOT_InstallMockUserMenu(const iaot::HookContext &context);

extern "C" bool IAOT_InstallMockUserHooks(const iaot::HookContext &context) {
  return IAOT_InstallMockUserMenu(context);
}
