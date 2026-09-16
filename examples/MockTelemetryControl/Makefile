TARGET := iphone:clang:latest:14.0
ARCHS := arm64
IAOT_ROOT := ../..
IAOT_LIBRARY_NAME := MockTelemetryControl

include generated/IAOTGeneratedProject.mk

IAOT_ENABLE_OVERLAY ?= 1
IAOT_OVERLAY_BACKEND ?= metal
IAOT_OVERLAY_AUTOSTART ?= 0
IAOT_RUNTIME_AUTOSTART ?= 1
IAOT_MINIMAL_RUNTIME ?= 0
IAOT_ENABLE_OBF ?= 0

include $(THEOS)/makefiles/common.mk

LIBRARY_NAME := MockTelemetryControl
MockTelemetryControl_FILES += \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.cpp) \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.mm) \
  generated/IAOTGeneratedHooks.mm \
  $(wildcard user/*.mm)
MockTelemetryControl_CFLAGS += -std=c++17 -DIAOT_RUNTIME_AUTOSTART=$(IAOT_RUNTIME_AUTOSTART) -DIAOT_ENABLE_OBF=$(IAOT_ENABLE_OBF) -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
MockTelemetryControl_CCFLAGS += $(MockTelemetryControl_CFLAGS)
MockTelemetryControl_OBJCCFLAGS += $(MockTelemetryControl_CFLAGS) -fobjc-arc
MockTelemetryControl_LDFLAGS += -ObjC
MockTelemetryControl_FRAMEWORKS += Foundation

include $(IAOT_ROOT)/runtime/overlay/IAOTOverlay.mk
include $(THEOS_MAKE_PATH)/library.mk
