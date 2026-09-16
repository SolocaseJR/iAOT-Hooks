TARGET := iphone:clang:latest:14.0
ARCHS := arm64
IAOT_ROOT := ../..
IAOT_LIBRARY_NAME := ExampleHooks
include generated/IAOTGeneratedProject.mk
IAOT_ENABLE_OVERLAY ?= 1
IAOT_OVERLAY_BACKEND ?= auto
IAOT_OVERLAY_AUTOSTART ?= 0
IAOT_RUNTIME_AUTOSTART ?= 1
IAOT_MINIMAL_RUNTIME ?= 0
# Release builds strip debuggable symbols (Theos' FINALPACKAGE strip/optimize
# pass) and turn on compile-time string/constant obfuscation (obfusheader.h,
# see IAOT/Obf.hpp) so `strings`/a Mach-O dump of the shipped dylib does not
# reveal log text, class/method names used for IL2CPP lookups, etc. Development
# builds keep both off for readable logs and faster iteration.
IAOT_ENABLE_OBF ?= $(if $(filter release,$(IAOT_BUILD_MODE)),1,0)
ifeq ($(IAOT_BUILD_MODE),release)
FINALPACKAGE := 1
endif
include $(THEOS)/makefiles/common.mk

LIBRARY_NAME := ExampleHooks
ifeq ($(IAOT_MINIMAL_RUNTIME),1)
ExampleHooks_FILES += $(IAOT_ROOT)/runtime/minimal/IAOTInert.cpp
else
ExampleHooks_FILES += \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.cpp) \
  $(wildcard $(IAOT_ROOT)/runtime/src/*.mm) \
  generated/IAOTGeneratedHooks.mm \
  $(wildcard user/*.mm)
endif
ExampleHooks_CFLAGS += -std=c++17 -DIAOT_RUNTIME_AUTOSTART=$(IAOT_RUNTIME_AUTOSTART) -DIAOT_ENABLE_OBF=$(IAOT_ENABLE_OBF) -Wno-format-security -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ExampleHooks_CCFLAGS += -std=c++17 -DIAOT_RUNTIME_AUTOSTART=$(IAOT_RUNTIME_AUTOSTART) -DIAOT_ENABLE_OBF=$(IAOT_ENABLE_OBF) -Wno-format-security -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ExampleHooks_OBJCCFLAGS += -std=c++17 -fobjc-arc -DIAOT_RUNTIME_AUTOSTART=$(IAOT_RUNTIME_AUTOSTART) -DIAOT_ENABLE_OBF=$(IAOT_ENABLE_OBF) -Wno-format-security -I$(IAOT_ROOT)/runtime/include -Igenerated -Iuser
ifneq ($(IAOT_MINIMAL_RUNTIME),1)
ExampleHooks_LDFLAGS += -ObjC
ExampleHooks_FRAMEWORKS += Foundation
endif

ifneq ($(IAOT_MINIMAL_RUNTIME),1)
include $(IAOT_ROOT)/runtime/overlay/IAOTOverlay.mk
endif
include $(THEOS_MAKE_PATH)/library.mk
