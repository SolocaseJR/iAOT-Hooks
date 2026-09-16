#include <IAOT/Logging.hpp>
#include <IAOT/Overlay.hpp>
#include <IAOT/detail/OverlayLifecycle.hpp>
#include <atomic>
#include <cstdio>
#include <mutex>
#include <memory>
#include <string>
#include <vector>

#if defined(__APPLE__) && IAOT_ENABLE_OVERLAY
#import <Metal/Metal.h>
#import <MetalKit/MetalKit.h>
#import <UIKit/UIKit.h>
#import <CoreText/CoreText.h>
#include <backends/imgui_impl_metal.h>
#include <imgui.h>
#include <imgui_internal.h>

namespace {
std::atomic<iaot::OverlayDrawFn> g_backgroundDrawCallback{nullptr};
std::atomic<iaot::OverlayDrawFn> g_drawCallback{nullptr};
std::atomic_bool g_visible{false};
iaot::detail::OverlayLifecycle g_lifecycle;
using OverlayTicket = iaot::detail::OverlayLifecycle::Ticket;
struct OwnedOverlayOptions {
  iaot::OverlayOptions options{};
  std::string title;
  std::vector<iaot::u32> primary;
  std::vector<iaot::u32> alternate;
};
std::shared_ptr<OwnedOverlayOptions> g_ownedOptions;
UIWindow *g_window = nil;
UIWindow *g_inputWindow = nil;
UIView *g_renderView = nil;
std::atomic_bool g_minimized{false};
float g_minimizedDiameter = 58.0f;
ImVec2 g_launcherPosition(24.0f, 140.0f);
std::mutex g_hitRegionMutex;
CGRect g_menuHitRegion = CGRectNull;
ImVec2 g_launcherHitPosition(24.0f, 140.0f);
const iaot::u32 *g_launcherIconPixels = nullptr;
iaot::u32 g_launcherIconWidth = 0;
iaot::u32 g_launcherIconHeight = 0;
const iaot::u32 *g_launcherAlternateIconPixels = nullptr;
iaot::u32 g_launcherAlternateIconWidth = 0;
iaot::u32 g_launcherAlternateIconHeight = 0;
id<MTLTexture> g_launcherTextures[2] = {nil, nil};
std::atomic<iaot::u32> g_launcherIconVariant{0};
std::atomic<iaot::u32> g_launcherBackgroundRgba{0x0B1020FF};
std::atomic<iaot::u32> g_launcherAccentRgba{0x64A5F0FF};
std::atomic<float> g_launcherBackgroundOpacity{0.82f};
char g_title[128]{};
iaot::OverlayOptions g_pendingOptions{};
char g_pendingTitle[128]{};
id g_sceneObserver = nil;
id g_applicationObserver = nil;
OverlayTicket g_retryGeneration = 0;
std::atomic_bool g_inputFrameUpdateScheduled{false};
// When a system Safari/authentication sheet (Google/Apple sign-in, etc.) is on
// screen, the overlay's Alert-level windows must step aside so the sheet is the
// frontmost interactive layer. These track that temporary hidden state and the
// lifetime of the main-queue watcher that drives it.
std::atomic_bool g_autoHiddenForSystemUI{false};
bool g_systemUiWatchActive = false; // main-thread only
unsigned int g_captureHideDepth = 0; // main-thread only

ImU32 PackedRgbaWithOpacity(iaot::u32 rgba, float opacity) {
  const int alpha = static_cast<int>((rgba & 0xFFu) * opacity);
  return IM_COL32((rgba >> 24) & 0xFFu, (rgba >> 16) & 0xFFu,
                  (rgba >> 8) & 0xFFu, alpha < 0 ? 0 : (alpha > 255 ? 255 : alpha));
}

void LoadSemiboldSystemFont() {
  ImGuiIO &io = ImGui::GetIO();
  CTFontRef base =
      CTFontCreateUIFontForLanguage(kCTFontUIFontSystem, 17.0, nullptr);
  CTFontRef semibold = base ? CTFontCreateCopyWithSymbolicTraits(
                                  base, 17.0, nullptr, kCTFontBoldTrait,
                                  kCTFontBoldTrait)
                            : nullptr;
  CTFontRef selected = semibold ? semibold : base;
  CFURLRef url =
      selected ? static_cast<CFURLRef>(
                     CTFontCopyAttribute(selected, kCTFontURLAttribute))
               : nullptr;
  NSString *path = url ? [(__bridge NSURL *)url path] : nil;
  if (path.length) {
    ImFontConfig config;
    config.RasterizerMultiply = 1.08f;
    config.OversampleH = 2;
    config.OversampleV = 2;
    io.FontDefault =
        io.Fonts->AddFontFromFileTTF(path.UTF8String, 17.0f, &config);
  }
  if (!io.FontDefault)
    io.FontDefault = io.Fonts->AddFontDefault();
  if (url)
    CFRelease(url);
  if (semibold)
    CFRelease(semibold);
  if (base)
    CFRelease(base);
}

id<MTLTexture> CreateRgbaTexture(id<MTLDevice> device,
                                 const iaot::u32 *pixels, iaot::u32 width,
                                 iaot::u32 height) {
  if (!device || !pixels || width == 0 || height == 0)
    return nil;
  MTLTextureDescriptor *descriptor = [MTLTextureDescriptor
      texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                   width:width
                                  height:height
                               mipmapped:NO];
  descriptor.usage = MTLTextureUsageShaderRead;
  id<MTLTexture> texture = [device newTextureWithDescriptor:descriptor];
  if (!texture)
    return nil;

  const size_t pixelCount = static_cast<size_t>(width) * height;
  std::vector<iaot::u8> rgba(pixelCount * 4);
  for (size_t index = 0; index < pixelCount; ++index) {
    const iaot::u32 packed = pixels[index];
    rgba[index * 4 + 0] = static_cast<iaot::u8>(packed >> 24);
    rgba[index * 4 + 1] = static_cast<iaot::u8>(packed >> 16);
    rgba[index * 4 + 2] = static_cast<iaot::u8>(packed >> 8);
    rgba[index * 4 + 3] = static_cast<iaot::u8>(packed);
  }
  [texture replaceRegion:MTLRegionMake2D(0, 0, width, height)
         mipmapLevel:0
           withBytes:rgba.data()
         bytesPerRow:static_cast<NSUInteger>(width) * 4];
  return texture;
}

void CreateLauncherTextures(id<MTLDevice> device) {
  g_launcherTextures[0] =
      CreateRgbaTexture(device, g_launcherIconPixels, g_launcherIconWidth,
                        g_launcherIconHeight);
  g_launcherTextures[1] = CreateRgbaTexture(
      device, g_launcherAlternateIconPixels, g_launcherAlternateIconWidth,
      g_launcherAlternateIconHeight);
}

id<MTLTexture> SelectedLauncherTexture() {
  const iaot::u32 selected =
      g_launcherIconVariant.load(std::memory_order_relaxed) == 1 ? 1 : 0;
  return g_launcherTextures[selected] ? g_launcherTextures[selected]
                                      : g_launcherTextures[0];
}

void UpdatePointer(UIView *view, NSSet<UITouch *> *touches, UIEvent *event) {
  if (!ImGui::GetCurrentContext())
    return;
  NSSet<UITouch *> *allTouches = event.allTouches;
  if (allTouches.count == 0)
    allTouches = touches;
  UITouch *anyTouch = [allTouches anyObject];
  if (!anyTouch)
    return;

  const CGPoint location = [anyTouch locationInView:view];
  ImGuiIO &io = ImGui::GetIO();
  io.AddMouseSourceEvent(ImGuiMouseSource_TouchScreen);
  io.AddMousePosEvent(static_cast<float>(location.x),
                      static_cast<float>(location.y));

  bool hasActiveTouch = false;
  for (UITouch *touch in allTouches) {
    if (touch.phase != UITouchPhaseEnded &&
        touch.phase != UITouchPhaseCancelled) {
      hasActiveTouch = true;
      break;
    }
  }
  io.AddMouseButtonEvent(0, hasActiveTouch);
}

bool LauncherContains(const ImVec2 &position,
                      const ImVec2 &launcherPosition) {
  const float radius = g_minimizedDiameter * 0.5f;
  const ImVec2 center(launcherPosition.x + radius,
                      launcherPosition.y + radius);
  const float dx = position.x - center.x;
  const float dy = position.y - center.y;
  return (dx * dx + dy * dy) <= (radius * radius);
}

CGRect CurrentInteractiveFrame() {
  std::lock_guard<std::mutex> lock(g_hitRegionMutex);
  if (g_minimized.load(std::memory_order_acquire)) {
    return CGRectMake(g_launcherHitPosition.x, g_launcherHitPosition.y,
                      g_minimizedDiameter, g_minimizedDiameter);
  }
  return g_menuHitRegion;
}

void ApplyInputWindowFrame() {
  if (!g_inputWindow || !g_window)
    return;
  CGRect frame = CurrentInteractiveFrame();
  if (CGRectIsNull(frame) || CGRectIsEmpty(frame))
    return;
  // Include the resize grip and antialiased edge pixels, then keep the input
  // window inside the landscape render coordinate space.
  frame = CGRectInset(frame, -4.0, -4.0);
  frame = CGRectIntersection(frame, g_window.bounds);
  if (CGRectIsNull(frame) || CGRectIsEmpty(frame))
    return;
  if (!CGRectEqualToRect(g_inputWindow.frame, frame))
    g_inputWindow.frame = frame;
}

void ScheduleInputWindowFrameUpdate() {
  if (g_inputFrameUpdateScheduled.exchange(true, std::memory_order_acq_rel))
    return;
  dispatch_async(dispatch_get_main_queue(), ^{
    ApplyInputWindowFrame();
    g_inputFrameUpdateScheduled.store(false, std::memory_order_release);
  });
}

void PublishLauncherHitRegion() {
  {
    std::lock_guard<std::mutex> lock(g_hitRegionMutex);
    g_launcherHitPosition = g_launcherPosition;
  }
  ScheduleInputWindowFrameUpdate();
}

void PublishMenuHitRegion(const ImVec2 &position, const ImVec2 &size) {
  {
    std::lock_guard<std::mutex> lock(g_hitRegionMutex);
    g_menuHitRegion = CGRectMake(position.x, position.y, size.x, size.y);
  }
  ScheduleInputWindowFrameUpdate();
}

bool InteractiveRegionContains(CGPoint point) {
  std::lock_guard<std::mutex> lock(g_hitRegionMutex);
  const ImVec2 position(static_cast<float>(point.x),
                        static_cast<float>(point.y));
  if (g_minimized.load(std::memory_order_acquire))
    return LauncherContains(position, g_launcherHitPosition);
  return !CGRectIsNull(g_menuHitRegion) &&
         CGRectContainsPoint(g_menuHitRegion, point);
}

// A system Safari/authentication sheet is rendered out-of-process and must be
// the frontmost interactive layer to receive touches. Our overlay windows live
// at UIWindowLevelAlert+1/+2, i.e. ABOVE such a sheet, which silently kills the
// sheet's touch handling even though the render window is non-interactive and
// the input window is only launcher-sized. Detect the sheet by class name on
// either a foreign visible window or the presented-view-controller chain of any
// window, so both the modally-presented SFSafariViewController case (same host
// window) and the ASWebAuthenticationSession case (its own window) are covered.
bool ClassNameLooksLikeSystemAuth(NSString *className) {
  if (className.length == 0)
    return false;
  return [className containsString:@"SFSafari"] ||
         [className containsString:@"Safari"] ||
         [className containsString:@"Authentication"] ||
         [className containsString:@"AuthKit"] ||
         [className containsString:@"WebAuthentication"];
}

bool PresentedChainIsSystemAuth(UIViewController *controller,
                                NSString **matchedOut) {
  int guard = 0;
  while (controller && guard++ < 8) {
    NSString *className = NSStringFromClass(controller.class);
    if (ClassNameLooksLikeSystemAuth(className)) {
      if (matchedOut)
        *matchedOut = className;
      return true;
    }
    controller = controller.presentedViewController;
  }
  return false;
}

bool ForeignSystemSheetPresent(NSString **matchedOut) {
  for (UIWindow *window in UIApplication.sharedApplication.windows) {
    if (window == g_window || window == g_inputWindow)
      continue;
    if (window.hidden || window.alpha <= 0.01f)
      continue;
    NSString *windowClass = NSStringFromClass(window.class);
    if (ClassNameLooksLikeSystemAuth(windowClass)) {
      if (matchedOut)
        *matchedOut = windowClass;
      return true;
    }
    if (PresentedChainIsSystemAuth(window.rootViewController, matchedOut))
      return true;
  }
  return false;
}

void PumpSystemUiWatch() {
  if (!g_window) {
    g_systemUiWatchActive = false;
    return;
  }
  NSString *matched = nil;
  const bool present = ForeignSystemSheetPresent(&matched);
  const bool hidden = g_autoHiddenForSystemUI.load(std::memory_order_acquire);
  if (present && !hidden) {
    g_autoHiddenForSystemUI.store(true, std::memory_order_release);
    g_window.hidden = YES;
    g_inputWindow.hidden = YES;
    iaot::Logf(OBF("IAOT overlay: stepped aside for system UI (%s)"),
               matched ? matched.UTF8String : "system-ui");
  } else if (!present && hidden) {
    g_autoHiddenForSystemUI.store(false, std::memory_order_release);
    if (g_visible.load(std::memory_order_acquire)) {
      g_window.hidden = NO;
      g_inputWindow.hidden = NO;
      ScheduleInputWindowFrameUpdate();
    }
    iaot::Log(OBF("IAOT overlay: restored after system UI dismissed"));
  }
  dispatch_after(
      dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)),
      dispatch_get_main_queue(), ^{
        PumpSystemUiWatch();
      });
}

void StartSystemUiWatch() {
  if (g_systemUiWatchActive)
    return;
  g_systemUiWatchActive = true;
  PumpSystemUiWatch();
}

// Aurora-style floating launcher: a draggable circular button that opens the
// menu on tap. This is the only thing visible while minimized, so the overlay
// stays out of the way until the user wants it (matching Aurora's launcher).
void DrawFloatingButton() {
  static bool draggedThisPress = false;
  const float d = g_minimizedDiameter;
  ImGui::SetNextWindowPos(g_launcherPosition, ImGuiCond_Always);
  ImGui::SetNextWindowSize(ImVec2(d, d));
  ImGui::SetNextWindowBgAlpha(0.0f);
  constexpr ImGuiWindowFlags flags =
      ImGuiWindowFlags_NoDecoration | ImGuiWindowFlags_NoResize |
      ImGuiWindowFlags_NoScrollbar | ImGuiWindowFlags_NoSavedSettings |
      ImGuiWindowFlags_NoBringToFrontOnFocus | ImGuiWindowFlags_NoNav |
      ImGuiWindowFlags_NoBackground;
  if (ImGui::Begin(OBF("##IAOTLauncher"), nullptr, flags)) {
    const ImVec2 wp = ImGui::GetWindowPos();
    const ImVec2 center(wp.x + d * 0.5f, wp.y + d * 0.5f);
    const float r = d * 0.5f;

    ImGui::SetCursorScreenPos(wp);
    ImGui::InvisibleButton(OBF("##IAOTLauncherHit"), ImVec2(d, d));
    const bool active = ImGui::IsItemActive();
    const bool hovered = ImGui::IsItemHovered();

    // Draw the button on the foreground list so it is never clipped by (or
    // hidden behind) other overlay windows.
    ImDrawList *dl = ImGui::GetForegroundDrawList();
    const float plateOpacity =
        g_launcherBackgroundOpacity.load(std::memory_order_relaxed);
    dl->AddCircleFilled(
        center, r,
        PackedRgbaWithOpacity(
            g_launcherBackgroundRgba.load(std::memory_order_relaxed),
            plateOpacity),
        64);
    dl->AddCircle(center, r - 1.0f,
                  PackedRgbaWithOpacity(
                      g_launcherAccentRgba.load(std::memory_order_relaxed),
                      hovered ? 0.90f : 0.55f),
                  64, hovered ? 2.0f : 1.2f);
    id<MTLTexture> launcherTexture = SelectedLauncherTexture();
    if (launcherTexture) {
      // Leave a small dark rim around the texture. Standard source-alpha
      // blending keeps every low-alpha glow pixel instead of thresholding it.
      const float inset = d * 0.075f;
      dl->AddImage(
          ImTextureRef(
              static_cast<ImTextureID>((intptr_t)launcherTexture)),
          ImVec2(wp.x + inset, wp.y + inset),
          ImVec2(wp.x + d - inset, wp.y + d - inset), ImVec2(0, 0),
          ImVec2(1, 1), IM_COL32_WHITE);
    } else {
      dl->AddText(ImVec2(center.x - 6.0f, center.y - 8.0f),
                  IM_COL32(190, 225, 255, 255), OBF("Q"));
    }

    if (ImGui::IsItemActivated())
      draggedThisPress = false;
    if (active && ImGui::IsMouseDragging(ImGuiMouseButton_Left)) {
      const ImVec2 delta = ImGui::GetIO().MouseDelta;
      g_launcherPosition.x += delta.x;
      g_launcherPosition.y += delta.y;
      draggedThisPress = true;
    }
    if (ImGui::IsItemDeactivated() && !draggedThisPress) {
      g_minimized.store(false,
                        std::memory_order_release); // tap opens the menu
      ScheduleInputWindowFrameUpdate();
    }
    PublishLauncherHitRegion();
  }
  ImGui::End();
}

void DrawOverlay() {
  // Non-interactive drawing (for example player-vision boxes) must continue
  // while the menu itself is minimized. Keep it outside any ImGui window so it
  // cannot affect touch hit-testing.
  if (const auto callback =
          g_backgroundDrawCallback.load(std::memory_order_acquire))
    callback();

  if (g_minimized.load(std::memory_order_acquire)) {
    DrawFloatingButton();
    return;
  }

  ImGui::SetNextWindowPos(ImVec2(24.0f, 90.0f), ImGuiCond_FirstUseEver);
  ImGui::SetNextWindowSize(ImVec2(480.0f, 310.0f), ImGuiCond_FirstUseEver);
  if (ImGui::Begin(g_title, nullptr,
                   ImGuiWindowFlags_NoCollapse |
                       ImGuiWindowFlags_NoSavedSettings)) {
    PublishMenuHitRegion(ImGui::GetWindowPos(), ImGui::GetWindowSize());
    id<MTLTexture> launcherTexture = SelectedLauncherTexture();
    if (launcherTexture) {
      // Keep the header icon a real ImGui item so a single tap minimizes the
      // menu without creating a second overlay hit region. ImageButton always
      // paints a themed ImGuiCol_Button frame behind the image regardless of
      // bg_col, which read as a small boxed-in icon; suppress that frame and
      // its padding so only the (now larger) icon itself is visible.
      ImGui::PushStyleColor(ImGuiCol_Button, ImVec4(0, 0, 0, 0));
      ImGui::PushStyleColor(ImGuiCol_ButtonHovered, ImVec4(0, 0, 0, 0));
      ImGui::PushStyleColor(ImGuiCol_ButtonActive, ImVec4(0, 0, 0, 0));
      ImGui::PushStyleVar(ImGuiStyleVar_FramePadding, ImVec2(0, 0));
      if (ImGui::ImageButton(
              OBF("##IAOTHeaderIcon"),
              ImTextureRef(static_cast<ImTextureID>((intptr_t)launcherTexture)),
              ImVec2(48.0f, 48.0f), ImVec2(0, 0), ImVec2(1, 1),
               ImVec4(0, 0, 0, 0), ImVec4(1, 1, 1, 1))) {
        g_minimized.store(true, std::memory_order_release);
        ScheduleInputWindowFrameUpdate();
      }
      ImGui::PopStyleVar();
      ImGui::PopStyleColor(3);
      ImGui::SameLine();
    }
    ImGui::BeginGroup();
    ImGui::TextUnformatted(OBF("QUASAR"));
    ImGui::TextDisabled(OBF("Chaos controls"));
    ImGui::EndGroup();
    ImGui::SameLine(ImGui::GetWindowContentRegionMax().x - 76.0f);
    if (ImGui::SmallButton(OBF("Minimize"))) {
      g_minimized.store(true, std::memory_order_release);
      ScheduleInputWindowFrameUpdate();
    }
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
  UpdatePointer(self, touches, event);
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  UpdatePointer(self, touches, event);
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  UpdatePointer(self, touches, event);
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches
               withEvent:(UIEvent *)event {
  UpdatePointer(self, touches, event);
}
@end

@interface IAOTOverlayInputView : UIView
@end

@implementation IAOTOverlayInputView
- (void)touchesBegan:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  UpdatePointer(g_renderView, touches, event);
}
- (void)touchesMoved:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  UpdatePointer(g_renderView, touches, event);
}
- (void)touchesEnded:(NSSet<UITouch *> *)touches withEvent:(UIEvent *)event {
  UpdatePointer(g_renderView, touches, event);
}
- (void)touchesCancelled:(NSSet<UITouch *> *)touches
               withEvent:(UIEvent *)event {
  UpdatePointer(g_renderView, touches, event);
}
@end

@interface IAOTOverlayWindow : UIWindow
@end

@implementation IAOTOverlayWindow
- (BOOL)canBecomeKeyWindow {
  return NO;
}
@end

@interface IAOTOverlayInputWindow : UIWindow
@end

@implementation IAOTOverlayInputWindow
- (BOOL)pointInside:(CGPoint)point withEvent:(UIEvent *)event {
  IAOT_UNUSED(event);
  CGPoint renderPoint = g_window ? [self convertPoint:point toWindow:g_window]
                                 : point;
  return InteractiveRegionContains(renderPoint) ? YES : NO;
}

- (UIView *)hitTest:(CGPoint)point withEvent:(UIEvent *)event {
  if (![self pointInside:point withEvent:event])
    return nil;
  return [super hitTest:point withEvent:event];
}

- (BOOL)canBecomeKeyWindow {
  return NO;
}
@end

@interface IAOTOverlayInputController : UIViewController
@end

@implementation IAOTOverlayInputController
- (BOOL)shouldAutorotate {
  return YES;
}
- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
  return UIInterfaceOrientationMaskLandscape;
}
- (UIInterfaceOrientation)preferredInterfaceOrientationForPresentation {
  return UIInterfaceOrientationLandscapeRight;
}
- (void)loadView {
  IAOTOverlayInputView *view = [[IAOTOverlayInputView alloc]
      initWithFrame:CGRectMake(0.0, 0.0, 1.0, 1.0)];
  view.backgroundColor = UIColor.clearColor;
  view.opaque = NO;
  view.userInteractionEnabled = YES;
  view.multipleTouchEnabled = YES;
  self.view = view;
}
@end

@interface IAOTOverlayController : UIViewController <MTKViewDelegate>
@property(nonatomic, strong) id<MTLDevice> metalDevice;
@property(nonatomic, strong) id<MTLCommandQueue> commandQueue;
@end

@implementation IAOTOverlayController
- (BOOL)shouldAutorotate {
  return YES;
}

- (UIInterfaceOrientationMask)supportedInterfaceOrientations {
  // The host game is landscape-only. Keeping the overlay on the same mask
  // prevents a portrait overlay window from intercepting or remapping game
  // touches when UIKit updates scene orientation.
  return UIInterfaceOrientationMaskLandscape;
}

- (UIInterfaceOrientation)preferredInterfaceOrientationForPresentation {
  return UIInterfaceOrientationLandscapeRight;
}

- (void)loadView {
  self.metalDevice = MTLCreateSystemDefaultDevice();
  IAOTOverlayView *view =
      [[IAOTOverlayView alloc] initWithFrame:UIScreen.mainScreen.bounds
                                      device:self.metalDevice];
  view.autoresizingMask =
      UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
  view.backgroundColor = UIColor.clearColor;
  view.opaque = NO;
  // Rendering and input use separate windows. This full-screen Metal surface
  // is permanently non-interactive, so it cannot consume a Google/Apple/native
  // authentication touch regardless of UIKit window ordering.
  view.userInteractionEnabled = NO;
  view.clearColor = MTLClearColorMake(0.0, 0.0, 0.0, 0.0);
  view.colorPixelFormat = MTLPixelFormatBGRA8Unorm;
  view.preferredFramesPerSecond = 60;
  view.delegate = self;
  g_renderView = view;
  self.view = view;
}

- (void)viewDidLoad {
  [super viewDidLoad];
  self.commandQueue = [self.metalDevice newCommandQueue];
  IMGUI_CHECKVERSION();
  ImGui::CreateContext();
  ImGui::StyleColorsDark();
  ImGui::GetIO().IniFilename = nullptr;
  LoadSemiboldSystemFont();
  CreateLauncherTextures(self.metalDevice);
  ImGui_ImplMetal_Init(self.metalDevice);
}

- (void)viewDidLayoutSubviews {
  [super viewDidLayoutSubviews];
  // A controller created for a scene-attached UIWindow can initially receive
  // a zero-sized view during scene activation. Keep the MTKView tied to the
  // real window bounds and force a valid drawable size before the first draw.
  IAOTOverlayView *view = (IAOTOverlayView *)self.view;
  if (view && self.view.window && !CGRectIsEmpty(self.view.window.bounds)) {
    view.frame = self.view.window.bounds;
    const CGFloat scale = self.view.window.screen.scale > 0.0
                              ? self.view.window.screen.scale
                              : UIScreen.mainScreen.scale;
    if (view.drawableSize.width <= 0.0 || view.drawableSize.height <= 0.0)
      view.drawableSize = CGSizeMake(view.bounds.size.width * scale,
                                     view.bounds.size.height * scale);
  }
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

  if (view.bounds.size.width <= 1.0 || view.bounds.size.height <= 1.0 ||
      view.drawableSize.width <= 1.0 || view.drawableSize.height <= 1.0) {
    iaot::Logf(OBF("IAOT overlay: skipped zero-sized frame (view %.0fx%.0f drawable %.0fx%.0f)"),
               static_cast<double>(view.bounds.size.width),
               static_cast<double>(view.bounds.size.height),
               static_cast<double>(view.drawableSize.width),
               static_cast<double>(view.drawableSize.height));
    return;
  }

  static bool loggedFirstFrame = false;
  if (!loggedFirstFrame) {
    loggedFirstFrame = true;
    iaot::Logf(OBF("IAOT overlay: first frame drawn (%.0fx%.0f)"),
               static_cast<double>(view.bounds.size.width),
               static_cast<double>(view.bounds.size.height));
  }

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
    g_launcherTextures[0] = nil;
    g_launcherTextures[1] = nil;
    ImGui::DestroyContext();
  }
}
@end

namespace iaot {
namespace {
bool StartOverlayOnMain(const OverlayOptions &options, OverlayTicket ticket);

void ClearLaunchObservers() {
  NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
  if (g_sceneObserver)
    [center removeObserver:g_sceneObserver];
  if (g_applicationObserver)
    [center removeObserver:g_applicationObserver];
  g_sceneObserver = nil;
  g_applicationObserver = nil;
}

void ScheduleWhenForegroundSceneExists(const OverlayOptions &options,
                                        OverlayTicket ticket) {
  g_pendingOptions = options;
  if (options.title != g_pendingTitle)
    std::snprintf(g_pendingTitle, sizeof(g_pendingTitle), "%s",
                options.title ? options.title : OBF("IAOT Diagnostic Overlay"));
  g_pendingOptions.title = g_pendingTitle;
  if (g_sceneObserver || g_applicationObserver)
    return;
  void (^retry)(NSNotification *) = ^(NSNotification *) {
    if (!g_lifecycle.IsCurrent(ticket))
      return;
    ClearLaunchObservers();
    StartOverlayOnMain(g_pendingOptions, ticket);
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

  // Notification-only startup has a small but real race: the app can become
  // active between the scene scan and observer registration. A short main
  // queue retry closes that gap and also covers Unity's delayed window setup.
  if (g_retryGeneration != ticket) {
    g_retryGeneration = ticket;
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.5 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
                     if (!g_lifecycle.IsCurrent(ticket))
                       return;
                     g_retryGeneration = 0;
                     if (!g_window)
                       StartOverlayOnMain(g_pendingOptions, ticket);
                   });
  }
}

bool StartOverlayOnMain(const OverlayOptions &options, OverlayTicket ticket) {
  if (!g_lifecycle.IsCurrent(ticket))
    return false;
  if (g_window) {
    g_lifecycle.Set(ticket, OverlayStatus::Ready);
    return true;
  }
  if (options.backend != OverlayBackend::Auto &&
      options.backend != OverlayBackend::Metal) {
    Log(OBF("IAOT overlay backend is unsupported on iOS"));
    g_lifecycle.Set(ticket, OverlayStatus::Failed);
    return false;
  }
  if (!MTLCreateSystemDefaultDevice()) {
    Log(OBF("IAOT overlay could not create a Metal device"));
    g_lifecycle.Set(ticket, OverlayStatus::Failed);
    return false;
  }

  g_minimized.store(options.startMinimized, std::memory_order_release);
  g_minimizedDiameter =
      options.minimizedDiameter > 32.0f ? options.minimizedDiameter : 58.0f;
  g_launcherIconPixels = options.launcherIconRgba;
  g_launcherIconWidth = options.launcherIconWidth;
  g_launcherIconHeight = options.launcherIconHeight;
  g_launcherAlternateIconPixels = options.launcherAlternateIconRgba;
  g_launcherAlternateIconWidth = options.launcherAlternateIconWidth;
  g_launcherAlternateIconHeight = options.launcherAlternateIconHeight;
  std::snprintf(g_title, sizeof(g_title), "%s",
                options.title ? options.title : OBF("IAOT Diagnostic Overlay"));

  IAOTOverlayWindow *window = nil;
  CGRect windowFrame = UIScreen.mainScreen.bounds;
  UIWindow *hostWindow = nil;
  if (@available(iOS 13.0, *)) {
    UIWindowScene *targetScene = nil;

    // This Unity build uses the legacy AppDelegate window and has no
    // UIApplicationSceneManifest. Prefer the already-visible Unity window's
    // scene; connectedScenes can be empty or still inactive during the early
    // constructor/hook phase. A UIWindow created without the host scene can
    // exist successfully yet never be composited by SpringBoard/UIKit.
    id appDelegate = UIApplication.sharedApplication.delegate;
    if ([appDelegate respondsToSelector:@selector(window)])
      hostWindow = [appDelegate window];
    if (!hostWindow) {
      for (UIWindow *candidate in UIApplication.sharedApplication.windows) {
        if (!candidate.hidden && candidate.windowScene) {
          hostWindow = candidate;
          break;
        }
      }
    }
    if (hostWindow.windowScene)
      targetScene = hostWindow.windowScene;

    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
      if ([scene isKindOfClass:UIWindowScene.class] &&
          scene.activationState == UISceneActivationStateForegroundActive) {
        if (!targetScene)
          targetScene = (UIWindowScene *)scene;
        break;
      }
    }
    if (targetScene) {
      window = [[IAOTOverlayWindow alloc] initWithWindowScene:targetScene];
      // A scene-attached UIWindow is created with a ZERO frame. Without this it
      // renders and hit-tests as a 0x0 window, so the overlay never appears.
      windowFrame = targetScene.coordinateSpace.bounds;
      if (CGRectIsEmpty(windowFrame))
        windowFrame = targetScene.screen.bounds;
      if (CGRectIsEmpty(windowFrame))
        windowFrame = UIScreen.mainScreen.bounds;
    } else {
      g_lifecycle.Set(ticket, OverlayStatus::WaitingForScene);
      ScheduleWhenForegroundSceneExists(options, ticket);
      Log(OBF("IAOT overlay: waiting for a host UIWindow/foreground scene"));
      return true;
    }
  } else {
    window = [[IAOTOverlayWindow alloc] initWithFrame:windowFrame];
  }
  ClearLaunchObservers();
  window.frame = windowFrame;
  if (CGRectIsEmpty(window.bounds)) {
    Log(OBF("IAOT overlay: refusing to show an empty UIWindow"));
    g_lifecycle.Set(ticket, OverlayStatus::Failed);
    return false;
  }
  window.backgroundColor = UIColor.clearColor;
  window.opaque = NO;
  window.alpha = 1.0;
  window.userInteractionEnabled = NO;
  window.windowLevel = UIWindowLevelAlert + 1.0;
  window.rootViewController = [IAOTOverlayController new];
  // makeKeyAndVisible is important for legacy Unity AppDelegate windows:
  // setting hidden=NO alone can leave a newly-created overlay outside the
  // active window hierarchy even though its frame and controller are valid.
  [window makeKeyAndVisible];
  // The overlay never needs key-window ownership. Leaving it key strands the
  // host app without first-responder focus and breaks native WKWebView content.
  if (hostWindow && hostWindow != window)
    [hostWindow makeKeyWindow];
  g_window = window;

  // Keep touch ownership out of the full-screen rendering window entirely.
  // A second transparent window is physically limited to the menu (or the
  // minimized launcher), so no hit-test behavior in this framework can block a
  // native authentication web view elsewhere on screen.
  IAOTOverlayInputWindow *inputWindow = nil;
  CGRect inputFrame =
      g_minimized.load(std::memory_order_acquire)
          ? CGRectMake(g_launcherPosition.x, g_launcherPosition.y,
                       g_minimizedDiameter, g_minimizedDiameter)
          : CGRectMake(24.0, 90.0, 480.0, 310.0);
  if (@available(iOS 13.0, *)) {
    if (window.windowScene)
      inputWindow =
          [[IAOTOverlayInputWindow alloc] initWithWindowScene:window.windowScene];
  }
  if (!inputWindow)
    inputWindow = [[IAOTOverlayInputWindow alloc] initWithFrame:inputFrame];
  inputWindow.frame = inputFrame;
  inputWindow.backgroundColor = UIColor.clearColor;
  inputWindow.opaque = NO;
  inputWindow.windowLevel = window.windowLevel + 1.0;
  inputWindow.rootViewController = [IAOTOverlayInputController new];
  inputWindow.hidden = NO;
  g_inputWindow = inputWindow;

  // Watch for system Safari/authentication sheets and step the overlay aside
  // for as long as one is up. Without this, the overlay's Alert-level windows
  // sit on top of a Google/Apple sign-in sheet and block every touch to it.
  StartSystemUiWatch();

  g_visible.store(true, std::memory_order_release);
  g_lifecycle.Set(ticket, OverlayStatus::Ready);
  Logf(OBF("IAOT overlay: render window shown (%.0fx%.0f) level %.0f "
           "with isolated input window minimized=%d"),
       static_cast<double>(windowFrame.size.width),
       static_cast<double>(windowFrame.size.height),
       static_cast<double>(UIWindowLevelAlert + 1.0),
       g_minimized.load(std::memory_order_acquire) ? 1 : 0);
  return true;
}
} // namespace

bool StartOverlay(const OverlayOptions &options) {
  // Copy caller-owned memory before any asynchronous work is queued.
  auto owned = std::make_shared<OwnedOverlayOptions>();
  owned->options = options;
  owned->title = options.title ? options.title : OBF("IAOT Diagnostic Overlay");
  const auto copyIcon = [](const u32 *pixels, u32 width, u32 height,
                           std::vector<u32> &destination) {
    if (!pixels)
      return width == 0 && height == 0;
    if (!width || !height || width > 1024 || height > 1024)
      return false;
    destination.assign(pixels, pixels + static_cast<std::size_t>(width) * height);
    return true;
  };
  if (!copyIcon(options.launcherIconRgba, options.launcherIconWidth,
                options.launcherIconHeight, owned->primary) ||
      !copyIcon(options.launcherAlternateIconRgba, options.launcherAlternateIconWidth,
                options.launcherAlternateIconHeight, owned->alternate)) {
    Log(OBF("IAOT overlay rejected invalid icon dimensions"));
    return false;
  }
  owned->options.title = owned->title.c_str();
  owned->options.launcherIconRgba =
      owned->primary.empty() ? nullptr : owned->primary.data();
  owned->options.launcherAlternateIconRgba =
      owned->alternate.empty() ? nullptr : owned->alternate.data();
  const auto ticket = g_lifecycle.Begin(OverlayStatus::Starting);
  bool (^start)() = ^bool {
    if (!g_lifecycle.IsCurrent(ticket))
      return false;
    ClearLaunchObservers();
    g_retryGeneration = 0;
    // An existing window continues to own its original artwork.
    if (!g_window)
      g_ownedOptions = owned;
    return StartOverlayOnMain(owned->options, ticket);
  };
  if (NSThread.isMainThread)
    return start();
  dispatch_async(dispatch_get_main_queue(), ^{
    start();
  });
  return true;
}

OverlayStatus GetOverlayStatus() { return g_lifecycle.Status(); }

void StopOverlay() {
  const auto ticket = g_lifecycle.Begin(OverlayStatus::Stopping);
  void (^stop)() = ^{
    if (!g_lifecycle.IsCurrent(ticket))
      return;
    ClearLaunchObservers();
    g_retryGeneration = 0;
    if ([g_window.rootViewController isKindOfClass:IAOTOverlayController.class])
      [(IAOTOverlayController *)g_window.rootViewController shutdown];
    g_inputWindow.hidden = YES;
    g_inputWindow = nil;
    g_window.hidden = YES;
    g_window = nil;
    g_renderView = nil;
    g_pendingOptions = {};
    g_pendingTitle[0] = '\0';
    g_launcherIconPixels = nullptr;
    g_launcherAlternateIconPixels = nullptr;
    g_ownedOptions.reset();
    // The watcher self-terminates on its next tick once g_window is nil.
    g_autoHiddenForSystemUI.store(false, std::memory_order_release);
    g_visible.store(false, std::memory_order_release);
    g_lifecycle.Set(ticket, OverlayStatus::Stopped);
    Log(OBF("IAOT overlay stopped; pending startup invalidated"));
  };
  if (NSThread.isMainThread)
    stop();
  else
    dispatch_async(dispatch_get_main_queue(), stop);
}
void SetOverlayBackgroundDrawCallback(OverlayDrawFn callback) {
  g_backgroundDrawCallback.store(callback, std::memory_order_release);
}
void SetOverlayDrawCallback(OverlayDrawFn callback) {
  g_drawCallback.store(callback, std::memory_order_release);
}
void SetOverlayLauncherAppearance(u32 backgroundRgba, u32 accentRgba,
                                  float opacity) {
  g_launcherBackgroundRgba.store(backgroundRgba, std::memory_order_relaxed);
  g_launcherAccentRgba.store(accentRgba, std::memory_order_relaxed);
  g_launcherBackgroundOpacity.store(
      opacity < 0.0f ? 0.0f : (opacity > 1.0f ? 1.0f : opacity),
      std::memory_order_relaxed);
}
void SetOverlayLauncherIconVariant(u32 variant) {
  g_launcherIconVariant.store(variant == 1 ? 1u : 0u,
                              std::memory_order_relaxed);
}
bool IsOverlayVisible() { return g_visible.load(std::memory_order_acquire); }
void SetOverlayVisible(bool visible) {
  dispatch_async(dispatch_get_main_queue(), ^{
    g_visible.store(visible && g_window != nil, std::memory_order_release);
    // While a system sheet is up the watcher owns window visibility; honor the
    // requested state only once the sheet is gone so we never pop back on top
    // of a Google/Apple sign-in view.
    if (!g_autoHiddenForSystemUI.load(std::memory_order_acquire)) {
      g_window.hidden = !visible;
      g_inputWindow.hidden = !visible;
    }
  });
}
void SetOverlayCaptureHidden(bool hidden) {
  void (^update)(void) = ^{
    if (hidden) {
      ++g_captureHideDepth;
      g_window.hidden = YES;
      g_inputWindow.hidden = YES;
      return;
    }
    if (g_captureHideDepth == 0)
      return;
    --g_captureHideDepth;
    if (g_captureHideDepth != 0)
      return;
    if (g_autoHiddenForSystemUI.load(std::memory_order_acquire))
      return;
    const bool visible = g_visible.load(std::memory_order_acquire);
    g_window.hidden = !visible;
    g_inputWindow.hidden = !visible;
  };
  if (NSThread.isMainThread)
    update();
  else
    dispatch_sync(dispatch_get_main_queue(), update);
}
bool IsOverlayCaptureWindow(const void *window) {
  if (!window)
    return false;
  id candidate = (__bridge id)window;
  return candidate == g_window || candidate == g_inputWindow;
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
