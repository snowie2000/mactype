#include "legacy_control_center.h"

#include <atomic>

namespace {

class FakeControlCenter final : public mactype::IControlCenter {
 public:
  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID, void** object) override {
    *object = this;
    AddRef();
    return S_OK;
  }
  ULONG STDMETHODCALLTYPE AddRef() override { return ++references_; }
  ULONG STDMETHODCALLTYPE Release() override {
    const ULONG remaining = --references_;
    if (remaining == 0) delete this;
    return remaining;
  }
  ULONG WINAPI GetVersion() override { return 0x0002'0001; }
  BOOL WINAPI SetIntAttribute(int, int) override { return TRUE; }
  BOOL WINAPI SetFloatAttribute(int, float) override { return TRUE; }
  int WINAPI GetIntAttribute(int) override { return 0; }
  float WINAPI GetFloatAttribute(int) override { return 0.0F; }
  BOOL WINAPI RefreshSetting() override { return TRUE; }
  BOOL WINAPI EnableRender(BOOL) override { return TRUE; }
  BOOL WINAPI EnableCache(BOOL) override { return TRUE; }
  BOOL WINAPI ClearIndividual() override { return TRUE; }
  BOOL WINAPI AddIndividual(WCHAR*) override { return TRUE; }
  BOOL WINAPI DelIndividual(WCHAR*) override { return TRUE; }
  void WINAPI LoadSetting(const WCHAR*) override {}
  HWND WINAPI CreateMessageWnd() override { return HWND_MESSAGE; }
  void WINAPI DestroyMessageWnd() override {}

 private:
  std::atomic<ULONG> references_{1};
};

}  // namespace

extern "C" void WINAPI CreateControlCenter(mactype::IControlCenter** result) {
  *result = new FakeControlCenter();
}

extern "C" HRESULT WINAPI DllGetVersion(DLLVERSIONINFO* version) {
  if (!version || version->cbSize < sizeof(DLLVERSIONINFO)) return E_INVALIDARG;
  version->dwMajorVersion = 2;
  version->dwMinorVersion = 1;
  version->dwBuildNumber = 0;
  version->dwPlatformID = DLLVER_PLATFORM_WINDOWS;
  return S_OK;
}
