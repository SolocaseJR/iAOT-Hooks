#include <IAOT/AppFunction.hpp>
#include <cassert>
#include <string>
static int FortyTwo() { return 42; }
static void Store(int *p) { *p = 9; }
static std::string Text() { return "owned demo"; }
static int Read(void *p) { return *static_cast<int *>(p); }
int main() {
  iaot::AppFunction<int()> a = iaot::AppFunction<int()>::FromRuntime(
      reinterpret_cast<std::uintptr_t>(&FortyTwo));
  assert(a.Call() == 42);
  int x = 0;
  iaot::AppFunction<void(int *)>::FromRuntime(
      reinterpret_cast<std::uintptr_t>(&Store))
      .Call(&x);
  assert(x == 9);
  assert(iaot::AppFunction<std::string()>::FromRuntime(
             reinterpret_cast<std::uintptr_t>(&Text))
             .Call() == "owned demo");
  assert(iaot::AppFunction<int(void *)>::FromRuntime(
             reinterpret_cast<std::uintptr_t>(&Read))
             .Call(&x) == 9);
  return 0;
}
