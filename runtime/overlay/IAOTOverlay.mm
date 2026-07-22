#include <IAOT/Logging.hpp>
#include <IAOT/Overlay.hpp>
#include <atomic>
#include <cstdio>
#include <string>

#if defined(__APPLE__) && IAOT_ENABLE_OVERLAY
#import <Metal/Metal.h>
#import <MetalKit/MetalKit.h>
#import <UIKit/UIKit.h>
#include <backends/imgui_impl_metal.h>
#include <imgui.h>
#include <imgui_internal.h>

namespace {
std::atomic<iaot::OverlayDrawFn> g_drawCallback{nullptr};
std::atomic_bool g_visible{false};
UIWindow *g_window = nil;
bool g_minimized = false;
float g_minimizedDiameter = 58.0f;
char g_title[128]{};
iaot::OverlayOptions g_pendingOptions{};
char g_pendingTitle[128]{};
id g_sceneObserver = nil;
id g_applicationObserver = nil;

void UpdatePointer(MTKView *view, NSSet<UITouch *> *touches, bool down) {
  if (!ImGui::GetCurrentContext() || touches.count == 0)
    return;
  const CGPoint location = [[touches anyObject] locationInView:view];
  ImGuiIO &io = ImGui::GetIO();
  io.AddMousePosEvent(static_cast<float>(location.x),
                      static_cast<float>(location.y));
  io.AddMouseButtonEvent(0, down);
}

void DrawOverlay() {
  static ImVec2 position(24.0f, 120.0f);
  if (g_minimized) {
    ImGui::SetNextWindowPos(position, ImGuiCond_Always);
    ImGui::SetNextWindowSize(ImVec2(g_minimizedDiameter, g_minimizedDiameter));
    ImGui::SetNextWindowBgAlpha(0.0f);
    constexpr ImGuiWindowFlags flags =
        ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoResize |
        ImGuiWindowFlags_NoScrollbar | ImGuiWindowFlags_NoSavedSettings |
        ImGuiWindowFlags_NoBringToFrontOnFocus;
    if (ImGui::Begin(OBF("##IAOTMinimized"), nullptr, flags)) {
      const ImVec2 windowPosition = ImGui::GetWindowPos();
      const ImVec2 center(windowPosition.x + g_minimizedDiameter * 0.5f,
                          windowPosition.y + g_minimizedDiameter * 0.5f);
      ImGui::GetWindowDrawList()->AddCircleFilled(
          center, g_minimizedDiameter * 0.48f, IM_COL32(52, 120, 246, 235));
      const ImVec2 labelSize = ImGui::CalcTextSize(OBF("IAOT"));
      ImGui::GetWindowDrawList()->AddText(
          ImVec2(center.x - labelSize.x * 0.5f, center.y - labelSize.y * 0.5f),
          IM_COL32(255, 255, 255, 255), OBF("IAOT"));
      ImGui::SetCursorScreenPos(windowPosition);
      ImGui::InvisibleButton(OBF("##IAOTOpen"),
                             ImVec2(g_minimizedDiameter, g_minimizedDiameter));
      if (ImGui::IsItemActive() &&
          ImGui::IsMouseDragging(ImGuiMouseButton_Left)) {
        const ImVec2 delta = ImGui::GetIO().MouseDelta;
        position.x += delta.x;
        position.y += delta.y;
      } else if (ImGui::IsItemClicked(ImGuiMouseButton_Left)) {
        g_minimized = false;
      }
    }
    ImGui::End();
    return;
  }

  ImGui::SetNextWindowPos(ImVec2(24.0f, 100.0f), ImGuiCond_FirstUseEver);
  ImGui::SetNextWindowSize(ImVec2(360.0f, 300.0f), ImGuiCond_FirstUseEver);
  if (ImGui::Begin(g_title, nullptr, ImGuiWindowFlags_NoCollapse)) {
    if (ImGui::Button(OBF("Minimize")))
      g_minimized = true;
    ImGui::Separator();
    if (const auto callback = g_drawCallback.load())
      callback();
    else
      ImGui::TextUnformatted(OBF("Authorized diagnostic overlay is ready."));
  }
  ImGui::End();
}
} // namespace

@interface IAOTOverlayView : MTKView
@end

@implementation IAOTOverlayView
- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  UpdatePointer(self, touches, true);
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  UpdatePointer(self, touches, true);
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  UpdatePointer(self, touches, false);
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches
               withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  UpdatePointer(self, touches, false);
}
@end

@interface IAOTOverlayWindow : UIWindow
@end

@implementation IAOTOverlayWindow
- (BOOL)pointInside:(CGPoint)point withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  if (!ImGui::GetCurrentContext())
    return NO;
  ImGuiContext &context = *ImGui::GetCurrentContext();
  const ImVec2 position(static_cast<float>(point.x),
                        static_cast<float>(point.y));
  for (ImGuiWindow *window : context.Windows) {
    if (window && window->Active && !window->Hidden &&
        window->OuterRectClipped.Contains(position))
      return YES;
  }
  return NO;
}
@end

@interface IAOTOverlayController : UIViewController <MTKViewDelegate>
@property(nonatomic, strong) id<MTLDevice> metalDevice;
@property(nonatomic, strong) id<MTLCommandQueue> commandQueue;
@end

@implementation IAOTOverlayController
- (void)loadView {
  self.metalDevice = MTLCreateSystemDefaultDevice();
  IAOTOverlayView *view =
      [[IAOTOverlayView alloc] initWithFrame:UIScreen.mainScreen.bounds
                                      device:self.metalDevice];
  view.backgroundColor = UIColor.clearColor;
  view.opaque = NO;
  view.clearColor = MTLClearColorMake(0.0, 0.0, 0.0, 0.0);
  view.colorPixelFormat = MTLPixelFormatBGRA8Unorm;
  view.preferredFramesPerSecond = 60;
  view.delegate = self;
  self.view = view;
}

- (void)viewDidLoad {
  [super viewDidLoad];
  self.commandQueue = [self.metalDevice newCommandQueue];
  IMGUI_CHECKVERSION();
  ImGui::CreateContext();
  ImGui::StyleColorsDark();
  ImGui::GetIO().IniFilename = nullptr;
  ImGui_ImplMetal_Init(self.metalDevice);
}

- (void)mtkView:(MTKView *)view drawableSizeWillChange:(CGSize)size {
  IAOT_UNUSED(view);
  IAOT_UNUSED(size);
}

- (void)drawInMTKView:(MTKView *)view {
  MTLRenderPassDescriptor *pass = view.currentRenderPassDescriptor;
  id<CAMetalDrawable> drawable = view.currentDrawable;
  if (!pass || !drawable || !ImGui::GetCurrentContext())
    return;

  ImGuiIO &io = ImGui::GetIO();
  io.DisplaySize = ImVec2(static_cast<float>(view.bounds.size.width),
                          static_cast<float>(view.bounds.size.height));
  io.DisplayFramebufferScale =
      ImVec2(static_cast<float>(view.contentScaleFactor),
             static_cast<float>(view.contentScaleFactor));
  io.DeltaTime =
      1.0f / static_cast<float>(MAX(view.preferredFramesPerSecond, 1));
  ImGui_ImplMetal_NewFrame(pass);
  ImGui::NewFrame();
  DrawOverlay();
  ImGui::Render();

  id<MTLCommandBuffer> commandBuffer = [self.commandQueue commandBuffer];
  id<MTLRenderCommandEncoder> encoder =
      [commandBuffer renderCommandEncoderWithDescriptor:pass];
  ImGui_ImplMetal_RenderDrawData(ImGui::GetDrawData(), commandBuffer, encoder);
  [encoder endEncoding];
  [commandBuffer presentDrawable:drawable];
  [commandBuffer commit];
}

- (void)shutdown {
  if (ImGui::GetCurrentContext()) {
    ImGui_ImplMetal_Shutdown();
    ImGui::DestroyContext();
  }
}
@end

namespace iaot {
namespace {
bool StartOverlayOnMain(const OverlayOptions &options);

void ClearLaunchObservers() {
  NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
  if (g_sceneObserver)
    [center removeObserver:g_sceneObserver];
  if (g_applicationObserver)
    [center removeObserver:g_applicationObserver];
  g_sceneObserver = nil;
  g_applicationObserver = nil;
}

void ScheduleWhenForegroundSceneExists(const OverlayOptions &options) {
  g_pendingOptions = options;
  std::snprintf(g_pendingTitle, sizeof(g_pendingTitle), "%s",
                options.title ? options.title : OBF("IAOT Diagnostic Overlay"));
  g_pendingOptions.title = g_pendingTitle;
  if (g_sceneObserver || g_applicationObserver)
    return;
  void (^retry)(NSNotification *) = ^(NSNotification *) {
    ClearLaunchObservers();
    StartOverlayOnMain(g_pendingOptions);
  };
  NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
  if (@available(iOS 13.0, *)) {
    g_sceneObserver = [center addObserverForName:UISceneDidActivateNotification
                                          object:nil
                                           queue:NSOperationQueue.mainQueue
                                      usingBlock:retry];
  }
  g_applicationObserver =
      [center addObserverForName:UIApplicationDidBecomeActiveNotification
                          object:nil
                           queue:NSOperationQueue.mainQueue
                      usingBlock:retry];
}

bool StartOverlayOnMain(const OverlayOptions &options) {
  if (g_window)
    return true;
  if (options.backend != OverlayBackend::Auto &&
      options.backend != OverlayBackend::Metal) {
    Log(OBF("IAOT overlay backend is unsupported on iOS"));
    return false;
  }
  if (!MTLCreateSystemDefaultDevice()) {
    Log(OBF("IAOT overlay could not create a Metal device"));
    return false;
  }

  g_minimized = options.startMinimized;
  g_minimizedDiameter =
      options.minimizedDiameter > 32.0f ? options.minimizedDiameter : 58.0f;
  std::snprintf(g_title, sizeof(g_title), "%s",
                options.title ? options.title : OBF("IAOT Diagnostic Overlay"));

  IAOTOverlayWindow *window = nil;
  if (@available(iOS 13.0, *)) {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
      if ([scene isKindOfClass:UIWindowScene.class] &&
          scene.activationState == UISceneActivationStateForegroundActive) {
        window = [[IAOTOverlayWindow alloc]
            initWithWindowScene:(UIWindowScene *)scene];
        break;
      }
    }
    if (!window) {
      id sceneManifest = [NSBundle.mainBundle
          objectForInfoDictionaryKey:@"UIApplicationSceneManifest"];
      if (sceneManifest) {
        ScheduleWhenForegroundSceneExists(options);
        Log(OBF("IAOT overlay is waiting for an active foreground scene"));
        return true;
      }
      window =
          [[IAOTOverlayWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    }
  } else {
    window =
        [[IAOTOverlayWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
  }
  ClearLaunchObservers();
  window.backgroundColor = UIColor.clearColor;
  window.windowLevel = UIWindowLevelAlert + 1.0;
  window.rootViewController = [IAOTOverlayController new];
  window.hidden = NO;
  g_window = window;
  g_visible.store(true, std::memory_order_release);
  return true;
}
} // namespace

bool StartOverlay(const OverlayOptions &options) {
  if (NSThread.isMainThread)
    return StartOverlayOnMain(options);
  const OverlayBackend backend = options.backend;
  const bool startMinimized = options.startMinimized;
  const float minimizedDiameter = options.minimizedDiameter;
  const std::string title =
      options.title ? options.title : OBF("IAOT Diagnostic Overlay");
  dispatch_async(dispatch_get_main_queue(), ^{
    OverlayOptions copied;
    copied.backend = backend;
    copied.startMinimized = startMinimized;
    copied.minimizedDiameter = minimizedDiameter;
    copied.title = title.c_str();
    StartOverlayOnMain(copied);
  });
  return true;
}

void StopOverlay() {
  dispatch_async(dispatch_get_main_queue(), ^{
    ClearLaunchObservers();
    if ([g_window.rootViewController isKindOfClass:IAOTOverlayController.class])
      [(IAOTOverlayController *)g_window.rootViewController shutdown];
    g_window.hidden = YES;
    g_window = nil;
    g_visible.store(false, std::memory_order_release);
  });
}

void SetOverlayDrawCallback(OverlayDrawFn callback) {
  g_drawCallback.store(callback);
}
bool IsOverlayVisible() { return g_visible.load(std::memory_order_acquire); }
void SetOverlayVisible(bool visible) {
  dispatch_async(dispatch_get_main_queue(), ^{
    g_window.hidden = !visible;
    g_visible.store(visible && g_window != nil, std::memory_order_release);
  });
}
} // namespace iaot

#if IAOT_OVERLAY_AUTOSTART
__attribute__((constructor)) static void IAOT_OverlayConstructor() {
  iaot::OverlayOptions options;
#if IAOT_OVERLAY_BACKEND_METAL
  options.backend = iaot::OverlayBackend::Metal;
#endif
  iaot::StartOverlay(options);
}
#endif
#endif
