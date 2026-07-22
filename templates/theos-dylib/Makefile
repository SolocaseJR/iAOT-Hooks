TARGET := iphone:clang:latest:14.0
ARCHS := arm64
IAOT_ROOT := ../..
IAOT_LIBRARY_NAME := ExampleHooks
include generated/IAOTGeneratedProject.mk
IAOT_ENABLE_OVERLAY ?= 1
IAOT_OVERLAY_BACKEND ?= auto
IAOT_OVERLAY_AUTOSTART ?= 1
include $(THEOS)/makefiles/common.mk

LIBRARY_NAME := ExampleHooks
ExampleHooks_FILES += \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.cpp) \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.mm) \
  generated/IAOTGeneratedHooks.mm \
  $(wildcard user/*.mm)
ExampleHooks_CFLAGS += -std=c++17 -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ExampleHooks_CCFLAGS += -std=c++17 -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ExampleHooks_OBJCCFLAGS += -std=c++17 -fobjc-arc -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ExampleHooks_LDFLAGS += -ObjC
ExampleHooks_FRAMEWORKS += Foundation

include $(IAOT_ROOT)/runtime/overlay/IAOTOverlay.mk
include $(THEOS_MAKE_PATH)/library.mk
