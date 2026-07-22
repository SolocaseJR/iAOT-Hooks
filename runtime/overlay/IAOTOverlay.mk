# Optional Dear ImGui diagnostic overlay for authorized host apps.
IAOT_ENABLE_OVERLAY ?= 0
IAOT_OVERLAY_BACKEND ?= auto
IAOT_OVERLAY_AUTOSTART ?= 1

ifeq ($(IAOT_ENABLE_OVERLAY),1)
IAOT_IMGUI_ROOT := $(IAOT_ROOT)/vendor/imgui
ifeq ($(wildcard $(IAOT_IMGUI_ROOT)/imgui.cpp),)
$(error Dear ImGui is missing. Run $(IAOT_ROOT)/scripts/fetch-imgui.sh first)
endif
ifeq ($(filter auto metal,$(IAOT_OVERLAY_BACKEND)),)
$(error IAOT_OVERLAY_BACKEND must be auto or metal on iOS)
endif
$(IAOT_LIBRARY_NAME)_FILES += \
  $(IAOT_ROOT)/runtime/overlay/IAOTOverlay.mm \
  $(IAOT_IMGUI_ROOT)/imgui.cpp \
  $(IAOT_IMGUI_ROOT)/imgui_draw.cpp \
  $(IAOT_IMGUI_ROOT)/imgui_tables.cpp \
  $(IAOT_IMGUI_ROOT)/imgui_widgets.cpp \
  $(IAOT_IMGUI_ROOT)/backends/imgui_impl_metal.mm
$(IAOT_LIBRARY_NAME)_CCFLAGS += -DIAOT_ENABLE_OVERLAY=1 -I$(IAOT_IMGUI_ROOT)
$(IAOT_LIBRARY_NAME)_OBJCCFLAGS += -DIAOT_ENABLE_OVERLAY=1 -DIAOT_OVERLAY_AUTOSTART=$(IAOT_OVERLAY_AUTOSTART) -I$(IAOT_IMGUI_ROOT)
ifeq ($(IAOT_OVERLAY_BACKEND),metal)
$(IAOT_LIBRARY_NAME)_OBJCCFLAGS += -DIAOT_OVERLAY_BACKEND_METAL=1
endif
$(IAOT_LIBRARY_NAME)_FRAMEWORKS += UIKit Metal MetalKit QuartzCore
endif
