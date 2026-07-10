TARGET := iphone:clang:latest:14.0
ARCHS := arm64
include $(THEOS)/makefiles/common.mk
LIBRARY_NAME := OwnedDemoHooks
OwnedDemoHooks_FILES += \
  ../../runtime/src/Runtime.mm \
  ../../runtime/src/HookRegistry.cpp \
  ../../runtime/src/Config.cpp \
  ../../runtime/src/AppFunction.cpp \
  ../../runtime/src/SegmentFinder.mm \
  ../../runtime/src/ObjCHooks.mm \
  ../../runtime/src/FunctionHooks.cpp \
  ../../runtime/src/SymbolResolver.mm \
  ../../runtime/src/Logging.mm \
  DemoTarget/DemoTarget.cpp \
  generated/IAOTGeneratedHooks.mm \
  hooks/UserHooks.mm \
  hooks/DemoStringHook.mm \
  hooks/DemoObjCHook.mm \
  hooks/DemoFunctionCaller.mm
OwnedDemoHooks_CFLAGS += -std=c++17 -I../../runtime/include -Igenerated
OwnedDemoHooks_CCFLAGS += -std=c++17 -I../../runtime/include -Igenerated
OwnedDemoHooks_OBJCCFLAGS += -std=c++17 -fobjc-arc -I../../runtime/include -Igenerated
OwnedDemoHooks_LDFLAGS += -ObjC
OwnedDemoHooks_FRAMEWORKS += Foundation
include $(THEOS_MAKE_PATH)/library.mk
