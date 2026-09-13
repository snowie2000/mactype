#pragma once

#include <Windows.h>
#include <Shlwapi.h>
#include <Unknwn.h>

namespace mactype {

struct IControlCenter {
  virtual HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** object) = 0;
  virtual ULONG STDMETHODCALLTYPE AddRef() = 0;
  virtual ULONG STDMETHODCALLTYPE Release() = 0;
  virtual ULONG WINAPI GetVersion() = 0;
  virtual BOOL WINAPI SetIntAttribute(int setting, int value) = 0;
  virtual BOOL WINAPI SetFloatAttribute(int setting, float value) = 0;
  virtual int WINAPI GetIntAttribute(int setting) = 0;
  virtual float WINAPI GetFloatAttribute(int setting) = 0;
  virtual BOOL WINAPI RefreshSetting() = 0;
  virtual BOOL WINAPI EnableRender(BOOL enabled) = 0;
  virtual BOOL WINAPI EnableCache(BOOL enabled) = 0;
  virtual BOOL WINAPI ClearIndividual() = 0;
  virtual BOOL WINAPI AddIndividual(WCHAR* setting) = 0;
  virtual BOOL WINAPI DelIndividual(WCHAR* face_name) = 0;
  virtual void WINAPI LoadSetting(const WCHAR* file_name) = 0;
  virtual HWND WINAPI CreateMessageWnd() = 0;
  virtual void WINAPI DestroyMessageWnd() = 0;
};

using CreateControlCenter = void(WINAPI*)(IControlCenter** control_center);
using DllGetVersion = HRESULT(WINAPI*)(DLLVERSIONINFO* version);

}  // namespace mactype
