TARGET := iphone:clang:latest:14.0
ARCHS := arm64
include $(THEOS)/makefiles/common.mk
LIBRARY_NAME := DemoUserHooks
DemoUserHooks_FILES += \
  ../../runtime/src/Runtime.mm \
  ../../runtime/src/HookRegistry.cpp \
  ../../runtime/src/Config.cpp \
  ../../runtime/src/AppFunction.cpp \
  ../../runtime/src/SegmentFinder.mm \
  ../../runtime/src/ObjCHooks.mm \
  ../../runtime/src/FunctionHooks.cpp \
  ../../runtime/src/SymbolResolver.mm \
  ../../runtime/src/Logging.mm \
  generated/IAOTGeneratedHooks.mm \
  hooks/UserHooks.mm
DemoUserHooks_CFLAGS += -std=c++17 -I../../runtime/include -Igenerated
DemoUserHooks_CCFLAGS += -std=c++17 -I../../runtime/include -Igenerated
DemoUserHooks_OBJCCFLAGS += -std=c++17 -fobjc-arc -I../../runtime/include -Igenerated
DemoUserHooks_LDFLAGS += -ObjC
DemoUserHooks_FRAMEWORKS += Foundation
include $(THEOS_MAKE_PATH)/library.mk
