TARGET := iphone:clang:latest:14.0
ARCHS := arm64
IAOT_ROOT := ../..
IAOT_LIBRARY_NAME := OwnedDemoHooks
include generated/IAOTGeneratedProject.mk
IAOT_ENABLE_OVERLAY ?= 0
IAOT_OVERLAY_BACKEND ?= auto
IAOT_OVERLAY_AUTOSTART ?= 1
include $(THEOS)/makefiles/common.mk
LIBRARY_NAME := OwnedDemoHooks
OwnedDemoHooks_FILES += \
  $(wildcard ../../runtime/src/*.cpp) \
  $(wildcard ../../runtime/src/*.mm) \
  DemoTarget/DemoTarget.cpp \
  generated/IAOTGeneratedHooks.mm \
  hooks/UserHooks.mm \
  hooks/DemoStringHook.mm \
  hooks/DemoObjCHook.mm \
  hooks/DemoFunctionCaller.mm
OwnedDemoHooks_CFLAGS += -std=c++17 -I../../runtime/include -Igenerated -Ihooks
OwnedDemoHooks_CCFLAGS += -std=c++17 -I../../runtime/include -Igenerated -Ihooks
OwnedDemoHooks_OBJCCFLAGS += -std=c++17 -fobjc-arc -I../../runtime/include -Igenerated -Ihooks
OwnedDemoHooks_LDFLAGS += -ObjC
OwnedDemoHooks_FRAMEWORKS += Foundation

include $(IAOT_ROOT)/runtime/overlay/IAOTOverlay.mk
include $(THEOS_MAKE_PATH)/library.mk
