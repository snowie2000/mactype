#include "directwrite.h"
#include "settings.h"
#include "dynCodeHelper.h"

void MyDebug(const TCHAR * sz, ...)
{
#ifdef DEBUG
	TCHAR szData[512] = { 0 };

	va_list args;
	va_start(args, sz);
	StringCchVPrintf(szData, sizeof(szData)-1, sz, args);
	va_end(args);

	OutputDebugString(szData);
#endif
}

#define SET_VAL(x, y) *(DWORD_PTR*)&(x) = *(DWORD_PTR*)&(y)
// To hook a method, add HOOK_MANUALLY() in hooklist.h and use this.
#define HOOK(obj, name, index) { \
	if (!HOOK_##name.Link) {  \
		AutoEnableDynamicCodeGen dynHelper(true);  \
		SET_VAL(ORIG_##name, (*reinterpret_cast<void***>(obj.p))[index]);  \
		hook_demand_##name(false);  \
		if (!HOOK_##name.Link) { MyDebug(L"##name hook failed"); }  \
	}  \
};

struct ComMethodHooker {
	// The target function if it has been hooked
	void*(*lpGetTargetFunc)();
	// The method the vftable refers to
	void*(*lpGetMethod)(IUnknown* obj);
	// Hook the method
	void(*lpHookFunc)(IUnknown* obj);
};

#define COM_METHOD_HOOKER(type, name, index) ComMethodHooker { \
	[]() -> void* { \
		if (!HOOK_##name.Link) \
			return NULL; \
		return HOOK_##name.Link->TargetProc; \
	}, \
	[](IUnknown* obj) -> void* { \
		return (*reinterpret_cast<void***>(obj))[index]; \
	}, \
	[](IUnknown* obj) -> void { \
		CComPtr<type> ptr = (type*)obj; \
		HOOK(ptr, name, index); \
	} \
}

#define COM_METHOD_HOOKER_EMPTY() ComMethodHooker { \
	[]() -> void* { \
		return NULL; \
	}, \
	[](IUnknown* obj) -> void* { \
		return NULL; \
	}, \
	[](IUnknown* obj) -> void { \
		return; \
	} \
}

struct Params {
	D2D1_TEXT_ANTIALIAS_MODE AntialiasMode;
	// Don't access directly. Use Get(D2D|DW)RenderingParams().
	IDWriteRenderingParams *RenderingParams;

	FLOAT Gamma;
	FLOAT EnhancedContrast;
	FLOAT ClearTypeLevel;
	DWRITE_PIXEL_GEOMETRY PixelGeometry;
	// RenderingMode=6 is invalid for DWrite interface
	DWRITE_RENDERING_MODE RenderingMode;
	FLOAT GrayscaleEnhancedContrast;
	DWRITE_GRID_FIT_MODE GridFitMode;
	DWRITE_RENDERING_MODE1 RenderingMode1;

	Params();
	void CreateParams(IDWriteFactory* dw_factory);
};

//IDWriteFactory* g_pDWriteFactory = NULL;
CComPtr<IDWriteGdiInterop> g_pGdiInterop = NULL;

enum D2D1RenderTargetCategory {
	D2D1_RENDER_TARGET_CATEGORY = 1,
	// ID2D1DCRenderTarget, ID2D1HwndRenderTarget, ID2D1BitmapRenderTarget
	D2D1_RENDER_TARGET1_CATEGORY,
	D2D1_DEVICE_CONTEXT_CATEGORY
};

template<typename Intf>
inline HRESULT IfSupport(IUnknown* pUnknown, void(*lpFunc)(Intf*)) {
	CComPtr<Intf> comObject;
	HRESULT hr = pUnknown->QueryInterface(&comObject);
	if (SUCCEEDED(hr)) {
		lpFunc(comObject);
	}
	return hr;
}

void Params::CreateParams(IDWriteFactory *dw_factory)
{
	IDWriteFactory3* dw3 = NULL;
	IDWriteFactory2* dw2 = NULL;
	IDWriteFactory1* dw1 = NULL;
	IDWriteRenderingParams3* r3 = NULL;
	IDWriteRenderingParams2* r2 = NULL;
	IDWriteRenderingParams1* r1 = NULL;
	IDWriteRenderingParams* r0 = NULL;

	CComPtr<IDWriteFactory> pDWriteFactory;
	if (NULL == dw_factory) {
		ORIG_DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,
			__uuidof(IDWriteFactory),
			reinterpret_cast<IUnknown**>(&pDWriteFactory));
		dw_factory = pDWriteFactory;
	}

	HRESULT hr = dw_factory->QueryInterface(&dw3);
	if SUCCEEDED(hr) {
		hr = dw3->CreateCustomRenderingParams(
			this->Gamma,
			this->EnhancedContrast,
			this->GrayscaleEnhancedContrast,
			this->ClearTypeLevel,
			this->PixelGeometry,
			this->RenderingMode1,
			this->GridFitMode,
			&r3);
		dw3->Release();
		if SUCCEEDED(hr) {
			RenderingParams = r3;
			return;
		}
	}

	hr = dw_factory->QueryInterface(&dw2);
	if SUCCEEDED(hr) {
		hr = dw2->CreateCustomRenderingParams(
			this->Gamma,
			this->EnhancedContrast,
			this->GrayscaleEnhancedContrast,
			this->ClearTypeLevel,
			this->PixelGeometry,
			this->RenderingMode,
			this->GridFitMode,
			&r2);
		dw2->Release();
		if SUCCEEDED(hr) {
			RenderingParams = r2;
			return;
		}
	}

	hr = dw_factory->QueryInterface(&dw1);
	if SUCCEEDED(hr) {
		hr = dw1->CreateCustomRenderingParams(
			this->Gamma,
			this->EnhancedContrast,
			this->GrayscaleEnhancedContrast,
			this->ClearTypeLevel,
			this->PixelGeometry,
			this->RenderingMode,
			&r1);
		dw1->Release();
		if SUCCEEDED(hr) {
			RenderingParams = r1;
			return;
		}
	}

	hr = dw_factory->CreateCustomRenderingParams(
		this->Gamma,
		this->EnhancedContrast,
		this->ClearTypeLevel,
		this->PixelGeometry,
		this->RenderingMode,
		&r0);
	if (SUCCEEDED(hr)) {
		RenderingParams = r0;
		return;
	}

	RenderingParams = NULL;
}

Params::Params() {
	//MessageBox(NULL, L"MakeParam", NULL, MB_OK);
	const CGdippSettings* pSettings = CGdippSettings::GetInstanceNoInit();
	//
	Gamma = pSettings->GammaValueForDW();	//user defined value preferred.
	//if (Gamma == 0)
	//	Gamma = pSettings->GammaValue()*pSettings->GammaValue() > 1.3 ? pSettings->GammaValue()*pSettings->GammaValue() / 2 : 0.7f;
	EnhancedContrast = pSettings->ContrastForDW();
	ClearTypeLevel = pSettings->ClearTypeLevelForDW();
	switch (pSettings->GetFontSettings().GetAntiAliasMode())
	{
		case 2:
		case 4:
			PixelGeometry = DWRITE_PIXEL_GEOMETRY_RGB;
			break;
		case 3:
		case 5:
			PixelGeometry = DWRITE_PIXEL_GEOMETRY_BGR;
			break;
		default:
			PixelGeometry = DWRITE_PIXEL_GEOMETRY_FLAT;
	}

	AntialiasMode = (D2D1_TEXT_ANTIALIAS_MODE)D2D1_TEXT_ANTIALIAS_MODE_DEFAULT;
	RenderingMode = (DWRITE_RENDERING_MODE)pSettings->RenderingModeForDW();
	GrayscaleEnhancedContrast = pSettings->ContrastForDW();
	switch (pSettings->GetFontSettings().GetHintingMode())
	{
		case 0: GridFitMode = DWRITE_GRID_FIT_MODE_DEFAULT;
			break;
		case 1: GridFitMode = DWRITE_GRID_FIT_MODE_DISABLED;
			break;
		default:
			GridFitMode = DWRITE_GRID_FIT_MODE_ENABLED;
			break;
	}
	RenderingMode1 = (DWRITE_RENDERING_MODE1)pSettings->RenderingModeForDW();
	RenderingParams = NULL;
}

Params* GetD2DParams() {
	static Params d2dParams;
	return &d2dParams;
}

Params* GetDWParams() {
	static Params dwParams = [] {
		const CGdippSettings* pSettings = CGdippSettings::GetInstanceNoInit();

		Params p;
		if (pSettings->RenderingModeForDW() == 6) {	//DW rendering in mode6 is horrible
			p.RenderingMode = DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC;
			p.RenderingMode1 = DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC;
		}
		//g_DWParams.Gamma = powf(g_D2DParams.Gamma, 1.0 / 3.0);
		return p;
	}();
	return &dwParams;
}

IDWriteRenderingParams* GetD2DRenderingParams(IDWriteRenderingParams* default) {
	Params* params = GetD2DParams();
	static bool inited = [&] {
		params->CreateParams(NULL);
		return true;
	}();

	if (params->RenderingParams)
		return params->RenderingParams;
	else
		return default;
}

IDWriteRenderingParams* GetDWRenderingParams(IDWriteRenderingParams* default) {
	Params* params = GetDWParams();
	static bool inited = [&] {
		params->CreateParams(NULL);
		return true;
	}();

	if (params->RenderingParams)
		return params->RenderingParams;
	else
		return default;
}

// Hook the implementation rather than an interface.
// There are many versions but inside the DLL there's only one implementation
// that supports the latest available one. Older versions are just upcasts,
// they share the same vftable.
void HookFactory(ID2D1Factory* pD2D1Factory) {
	static bool loaded = [&] {
		HRESULT hr;
		CComPtr<ID2D1Factory> ptr = pD2D1Factory;
		// index is the index in the vftable. Their order is the same as
		// methods declared in the header.
		HOOK(ptr, CreateWicBitmapRenderTarget, 13);
		HOOK(ptr, CreateHwndRenderTarget, 14);
		HOOK(ptr, CreateDxgiSurfaceRenderTarget, 15);
		HOOK(ptr, CreateDCRenderTarget, 16);
		MyDebug(L"ID2D1Factory hooked");

		CComPtr<ID2D1Factory1> ptr1;
		hr = pD2D1Factory->QueryInterface(&ptr1);
		if (SUCCEEDED(hr)){
			HOOK(ptr1, CreateDevice1, 17);
			MyDebug(L"ID2D1Factory1 hooked");
		}

		CComPtr<ID2D1Factory2> ptr2;
		hr = pD2D1Factory->QueryInterface(&ptr2);
		if (SUCCEEDED(hr)){
			HOOK(ptr2, CreateDevice2, 27);
			MyDebug(L"ID2D1Factory2 hooked");
		}

		CComPtr<ID2D1Factory3> ptr3;
		hr = pD2D1Factory->QueryInterface(&ptr3);
		if (SUCCEEDED(hr)){
			HOOK(ptr3, CreateDevice3, 28);
			MyDebug(L"ID2D1Factory3 hooked");
		}

		CComPtr<ID2D1Factory4> ptr4;
		hr = pD2D1Factory->QueryInterface(&ptr4);
		if (SUCCEEDED(hr)){
			HOOK(ptr4, CreateDevice4, 29);
			MyDebug(L"ID2D1Factory4 hooked");
		}

		CComPtr<ID2D1Factory5> ptr5;
		hr = pD2D1Factory->QueryInterface(&ptr5);
		if (SUCCEEDED(hr)){
			HOOK(ptr5, CreateDevice5, 30);
			MyDebug(L"ID2D1Factory5 hooked");
		}

		CComPtr<ID2D1Factory6> ptr6;
		hr = pD2D1Factory->QueryInterface(&ptr6);
		if (SUCCEEDED(hr)){
			HOOK(ptr6, CreateDevice6, 31);
			MyDebug(L"ID2D1Factory6 hooked");
		}

		CComPtr<ID2D1Factory7> ptr7;
		hr = pD2D1Factory->QueryInterface(&ptr7);
		if (SUCCEEDED(hr)){
			HOOK(ptr7, CreateDevice7, 32);
			MyDebug(L"ID2D1Factory7 hooked");
		}
		return true;
	}();
}

void HookDevice(ID2D1Device* d2dDevice){
	static bool loaded = [&] {
		CComPtr<ID2D1Device> ptr = d2dDevice;
		HOOK(ptr, CreateDeviceContext, 4);
		MyDebug(L"ID2D1Device hooked");

		CComPtr<ID2D1Device1> ptr2;
		HRESULT hr = (d2dDevice)->QueryInterface(&ptr2);
		if SUCCEEDED(hr) {
			HOOK(ptr2, CreateDeviceContext2, 11);
			MyDebug(L"ID2D1Device1 hooked");
		}
		CComPtr<ID2D1Device2> ptr3;
		hr = (d2dDevice)->QueryInterface(&ptr3);
		if SUCCEEDED(hr) {
			HOOK(ptr3, CreateDeviceContext3, 12);
			MyDebug(L"ID2D1Device2 hooked");
		}
		CComPtr<ID2D1Device3> ptr4;
		hr = (d2dDevice)->QueryInterface(&ptr4);
		if SUCCEEDED(hr) {
			HOOK(ptr4, CreateDeviceContext4, 15);
			MyDebug(L"ID2D1Device3 hooked");
		}
		CComPtr<ID2D1Device4> ptr5;
		hr = (d2dDevice)->QueryInterface(&ptr5);
		if SUCCEEDED(hr) {
			HOOK(ptr5, CreateDeviceContext5, 16);
			MyDebug(L"ID2D1Device4 hooked");
		}
		CComPtr<ID2D1Device5> ptr6;
		hr = (d2dDevice)->QueryInterface(&ptr6);
		if SUCCEEDED(hr) {
			HOOK(ptr6, CreateDeviceContext6, 17);
			MyDebug(L"ID2D1Device5 hooked");
		}
		CComPtr<ID2D1Device6> ptr7;
		hr = (d2dDevice)->QueryInterface(&ptr7);
		if SUCCEEDED(hr) {
			HOOK(ptr7, CreateDeviceContext7, 18);
			MyDebug(L"ID2D1Device6 hooked");
		}
		return true;
	}();
}

// Hook the method if it has not been hooked. It's not thread safe.
void HookRenderTargetMethod(
	ID2D1RenderTarget* pD2D1RenderTarget,
	D2D1RenderTargetCategory hookCategory,
	ComMethodHooker methodHookers[]
	) {
	void* method = methodHookers[hookCategory].lpGetMethod(pD2D1RenderTarget);

	if (D2D1_RENDER_TARGET_CATEGORY != hookCategory) {
		void* target = methodHookers[D2D1_RENDER_TARGET_CATEGORY].lpGetTargetFunc();
		if (target != NULL && target == method) {
			return;
		}
	}
	if (D2D1_RENDER_TARGET1_CATEGORY != hookCategory) {
		void* target = methodHookers[D2D1_RENDER_TARGET1_CATEGORY].lpGetTargetFunc();
		if (target != NULL && target == method) {
			return;
		}
	}
	if (D2D1_DEVICE_CONTEXT_CATEGORY != hookCategory) {
		void* target = methodHookers[D2D1_DEVICE_CONTEXT_CATEGORY].lpGetTargetFunc();
		if (target != NULL && target == method) {
			return;
		}
	}

	methodHookers[hookCategory].lpHookFunc(pD2D1RenderTarget);
}

void HookRenderTarget(
	ID2D1RenderTarget* pD2D1RenderTarget,
	D2D1RenderTargetCategory hookCategory
	){
	MyDebug(L"HookRenderTarget %d", hookCategory);
	static bool loaded = [&] {
		CComPtr<ID2D1RenderTarget> ptr = pD2D1RenderTarget;
		HOOK(ptr, CreateCompatibleRenderTarget, 12);
		HOOK(ptr, D2D1RenderTarget_DrawTextLayout, 28);

		ID2D1Factory* pD2D1Factory;
		pD2D1RenderTarget->GetFactory(&pD2D1Factory);
		if (pD2D1Factory)
			HookFactory(pD2D1Factory);

		// Actually it always implements ID2D1DeviceContext regardless of
		// hookCategory.
		CComPtr<ID2D1DeviceContext> ptr1;
		HRESULT hr = pD2D1RenderTarget->QueryInterface(&ptr1);
		if (SUCCEEDED(hr)) {
			ID2D1Device* pD2D1Device;
			ptr1->GetDevice(&pD2D1Device);
			if (pD2D1Device)
				HookDevice(pD2D1Device);
		}
		return true;
	}();

	// Some methods are duplicated across interfaces. Hook them whenever they're
	// available from an instance. Make sure don't hook the same function
	// multiple times.
	//
	// Up to two instances of the same function
	static ComMethodHooker hookDrawText[] = {
		COM_METHOD_HOOKER_EMPTY(),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_DrawText, 27),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_DrawText, 27),
		COM_METHOD_HOOKER(ID2D1DeviceContext, D2D1DeviceContext_DrawText, 27)
	};
	// Up to three instances of the same function
	static ComMethodHooker hookDrawGlyphRun[] = {
		COM_METHOD_HOOKER_EMPTY(),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_DrawGlyphRun, 29),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget1_DrawGlyphRun, 29),
		COM_METHOD_HOOKER(ID2D1DeviceContext, D2D1DeviceContext_DrawGlyphRun, 29)
	};
	static ComMethodHooker hookSetTextAntialiasMode[] = {
		COM_METHOD_HOOKER_EMPTY(),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_SetTextAntialiasMode, 34),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_SetTextAntialiasMode, 34),
		COM_METHOD_HOOKER(ID2D1DeviceContext, D2D1DeviceContext_SetTextAntialiasMode, 34)
	};
	static ComMethodHooker hookSetTextRenderingParams[] = {
		COM_METHOD_HOOKER_EMPTY(),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_SetTextRenderingParams, 36),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_SetTextRenderingParams, 36),
		COM_METHOD_HOOKER(ID2D1DeviceContext, D2D1DeviceContext_SetTextRenderingParams, 36)
	};
	// Note that there's a branch in the inheritance hierarchy in
	// ID2D1RenderTarget. But the implementation always supports
	// ID2D1DeviceContext, thus multiple inheritance should take place. The
	// vftable for ID2D1DeviceContext is different if hookCategory is not
	// D2D1_DEVICE_CONTEXT_CATEGORY. It consists of thunks each of which adjusts
	// the this pointer and then jumps to the corresponding method for
	// ID2D1RenderTarget (So we don't have to hook them as well), and the other
	// functions for ID2D1DeviceContext.
	static ComMethodHooker hookDrawGlyphRun1[] = {
		COM_METHOD_HOOKER_EMPTY(),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget_DrawGlyphRun1, 82),
		COM_METHOD_HOOKER(ID2D1RenderTarget, D2D1RenderTarget1_DrawGlyphRun1, 82),
		COM_METHOD_HOOKER(ID2D1DeviceContext, D2D1DeviceContext_DrawGlyphRun1, 82)
	};

	if (hookCategory == D2D1_RENDER_TARGET_CATEGORY) {
		static bool loaded1 = [&] {
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_DWRITE);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawText);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawGlyphRun);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextAntialiasMode);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextRenderingParams);

			CComPtr<ID2D1DeviceContext> ptr;
			HRESULT hr = pD2D1RenderTarget->QueryInterface(&ptr);
			if (SUCCEEDED(hr)) {
				HookRenderTargetMethod(ptr, hookCategory, hookDrawGlyphRun1);
			}
			return true;
		}();
	} else if (hookCategory == D2D1_RENDER_TARGET1_CATEGORY) {
		static bool loaded2 = [&] {
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_DWRITE);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawText);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawGlyphRun);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextAntialiasMode);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextRenderingParams);

			CComPtr<ID2D1DeviceContext> ptr;
			HRESULT hr = pD2D1RenderTarget->QueryInterface(&ptr);
			if (SUCCEEDED(hr)) {
				HookRenderTargetMethod(ptr, hookCategory, hookDrawGlyphRun1);
			}
			return true;
		}();
	} else if (hookCategory == D2D1_DEVICE_CONTEXT_CATEGORY) {
		static bool loaded3 = [&] {
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_DWRITE);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawText);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawGlyphRun);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextAntialiasMode);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookSetTextRenderingParams);
			HookRenderTargetMethod(pD2D1RenderTarget, hookCategory, hookDrawGlyphRun1);
			return true;
		}();
	}

	pD2D1RenderTarget->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_DEFAULT);
	if (GetD2DRenderingParams(NULL)) {
		pD2D1RenderTarget->SetTextRenderingParams(GetD2DRenderingParams(NULL));
	}
}


//DWrite hooks
HRESULT WINAPI IMPL_CreateGlyphRunAnalysis(
	IDWriteFactory* This,
	DWRITE_GLYPH_RUN const* glyphRun,
	FLOAT pixelsPerDip,
	DWRITE_MATRIX const* transform,
	DWRITE_RENDERING_MODE renderingMode,
	DWRITE_MEASURING_MODE measuringMode,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	IDWriteGlyphRunAnalysis** glyphRunAnalysis)
{
	HRESULT hr = E_FAIL;
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE_ALIASED) {
		MyDebug(L"Try DW2");
		IDWriteFactory2* f;
		hr = This->QueryInterface(&f);
		if (SUCCEEDED(hr)) {
			DWRITE_MATRIX m = {};
			if (transform) {
				m = *transform;
				m.m11 *= pixelsPerDip;
				m.m12 *= pixelsPerDip;
				m.m21 *= pixelsPerDip;
				m.m22 *= pixelsPerDip;
				m.dx *= pixelsPerDip;
				m.dy *= pixelsPerDip;
			}
			else {
				m.m11 = pixelsPerDip;
				m.m22 = pixelsPerDip;
			}
			hr = f->CreateGlyphRunAnalysis(
				glyphRun,
				&m,
				renderingMode,
				measuringMode,
				DWRITE_GRID_FIT_MODE_DEFAULT,
				DWRITE_TEXT_ANTIALIAS_MODE_CLEARTYPE,
				baselineOriginX,
				baselineOriginY,
				glyphRunAnalysis
				);
			f->Release();
		}
	}

	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE_ALIASED) {
		MyDebug(L"Try DW1");
		Params* dwParams = GetDWParams();
		DWRITE_MATRIX m;
		DWRITE_MATRIX const* pm = transform;
		if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
			if (transform) {
				m = *transform;
				m.m12 += 1.0f / 0xFFFF;
				m.m21 += 1.0f / 0xFFFF;
			}
			else {
				m = { 1, 1.0f / 0xFFFF, 1.0f / 0xFFFF, 1 };
			}
			pm = &m;
		}
		hr = ORIG_CreateGlyphRunAnalysis(
			This,
			glyphRun,
			pixelsPerDip,
			pm,
			dwParams->RenderingMode,
			measuringMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		MyDebug(L"Try Original Params");
		hr = ORIG_CreateGlyphRunAnalysis(
			This,
			glyphRun,
			pixelsPerDip,
			transform,
			renderingMode,
			measuringMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (SUCCEEDED(hr)) {
		MyDebug(L"CreateGlyphRunAnalysis hooked");
		static bool loaded = [&] {
			CComPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
			return true;
		}();
	}
	return hr;
}

HRESULT WINAPI IMPL_GetGdiInterop(
	IDWriteFactory* This,
	IDWriteGdiInterop** gdiInterop
	) 
{
	HRESULT hr = ORIG_GetGdiInterop(This, gdiInterop);
	static bool loaded = [&] {
		CComPtr<IDWriteGdiInterop> gdip = *gdiInterop;
		HOOK(gdip, CreateBitmapRenderTarget, 7);
		return true;
	}();
	MyDebug(L"IMPL_GetGdiInterop hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateBitmapRenderTarget(
	IDWriteGdiInterop* This,
	HDC hdc,
	UINT32 width,
	UINT32 height,
	IDWriteBitmapRenderTarget** renderTarget
	)
{
	HRESULT hr = ORIG_CreateBitmapRenderTarget(
		This,
		hdc,
		width,
		height,
		renderTarget
		);
	if (SUCCEEDED(hr)) {
		static bool loaded = [&] {
			CComPtr<IDWriteBitmapRenderTarget> ptr = *renderTarget;
			HOOK(ptr, BitmapRenderTarget_DrawGlyphRun, 3);
			return true;
		}();
	}
	MyDebug(L"CreateBitmapRenderTarget hooked");
	return hr;
}

HRESULT WINAPI IMPL_GetAlphaBlendParams(
	IDWriteGlyphRunAnalysis* This,
	IDWriteRenderingParams* renderingParams,
	FLOAT* blendGamma,
	FLOAT* blendEnhancedContrast,
	FLOAT* blendClearTypeLevel
	)
{
	HRESULT hr = E_FAIL;
	if (FAILED(hr)) {
		hr = ORIG_GetAlphaBlendParams(
			This,
			GetDWRenderingParams(renderingParams),
			blendGamma,
			blendEnhancedContrast,
			blendClearTypeLevel
			);
	}
	if (FAILED(hr)) {
		hr = ORIG_GetAlphaBlendParams(
			This,
			renderingParams,
			blendGamma,
			blendEnhancedContrast,
			blendClearTypeLevel
			);
	}
	MyDebug(L"GetAlphaBlendParams hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateGlyphRunAnalysis2(
	IDWriteFactory2* This,
	DWRITE_GLYPH_RUN const* glyphRun,
	DWRITE_MATRIX const* transform,
	DWRITE_RENDERING_MODE renderingMode,
	DWRITE_MEASURING_MODE measuringMode,
	DWRITE_GRID_FIT_MODE gridFitMode,
	DWRITE_TEXT_ANTIALIAS_MODE antialiasMode,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	IDWriteGlyphRunAnalysis** glyphRunAnalysis
	) 
{
	HRESULT hr = E_FAIL;
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE_ALIASED) {
		IDWriteFactory3* f;
		hr = This->QueryInterface(&f);
		if (SUCCEEDED(hr)) {
			hr = f->CreateGlyphRunAnalysis(
				glyphRun,
				transform,
				(DWRITE_RENDERING_MODE1)renderingMode,
				measuringMode,
				gridFitMode,
				antialiasMode,
				baselineOriginX,
				baselineOriginY,
				glyphRunAnalysis
				);
			f->Release();
		}
	}

	Params* dwParams = GetDWParams();
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE_ALIASED) {
		hr = ORIG_CreateGlyphRunAnalysis2(
			This,
			glyphRun,
			transform,
			dwParams->RenderingMode,
			measuringMode,
			dwParams->GridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		DWRITE_MATRIX m = {};
		DWRITE_MATRIX const* pm = transform;
		if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
			if (transform) {
				m = *transform;
				m.m12 += 1.0f / 0xFFFF;
				m.m21 += 1.0f / 0xFFFF;
			}
			else {
				m = { 1, 1.0f / 0xFFFF, 1.0f / 0xFFFF, 1 };
			}
			pm = &m;
		}
		hr = ORIG_CreateGlyphRunAnalysis2(
			This,
			glyphRun,
			pm,
			renderingMode,
			measuringMode,
			gridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		hr = ORIG_CreateGlyphRunAnalysis2(
			This,
			glyphRun,
			transform,
			renderingMode,
			measuringMode,
			gridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (SUCCEEDED(hr)) {
		MyDebug(L"CreateGlyphRunAnalysis2 hooked");
		static bool loaded = [&] {
			CComPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
			return true;
		}();
	}
	return hr;
}

HRESULT WINAPI IMPL_CreateGlyphRunAnalysis3(
	IDWriteFactory3* This,
	DWRITE_GLYPH_RUN const* glyphRun,
	DWRITE_MATRIX const* transform,
	DWRITE_RENDERING_MODE1 renderingMode,
	DWRITE_MEASURING_MODE measuringMode,
	DWRITE_GRID_FIT_MODE gridFitMode,
	DWRITE_TEXT_ANTIALIAS_MODE antialiasMode,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	IDWriteGlyphRunAnalysis** glyphRunAnalysis
	)
{
	MyDebug(L"CreateGlyphRunAnalysis3 hooked");
	Params* dwParams = GetDWParams();
	HRESULT hr = E_FAIL;
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE1_ALIASED) {
		hr = ORIG_CreateGlyphRunAnalysis3(
			This,
			glyphRun,
			transform,
			dwParams->RenderingMode1,
			measuringMode,
			dwParams->GridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		MyDebug(L"try again with only transformation");
		DWRITE_MATRIX m = {};
		DWRITE_MATRIX const* pm = transform;
		if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
			if (transform) {
				m = *transform;
				m.m12 += 1.0f / 0xFFFF;
				m.m21 += 1.0f / 0xFFFF;
			}
			else {
				m = { 1, 1.0f / 0xFFFF, 1.0f / 0xFFFF, 1 };
			}
			pm = &m;
		}
		hr = ORIG_CreateGlyphRunAnalysis3(
			This,
			glyphRun,
			pm,
			renderingMode,
			measuringMode,
			gridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		MyDebug(L"fallback to original params");
		hr = ORIG_CreateGlyphRunAnalysis3(
			This,
			glyphRun,
			transform,
			renderingMode,
			measuringMode,
			gridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (SUCCEEDED(hr)) {
		MyDebug(L"CreateGlyphRunAnalysis3 hooked");
		static bool loaded = [&] {
			CComPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
			return true;
		}();
	}
	return hr;
}


//d2d1 hooks
HRESULT WINAPI IMPL_D2D1CreateDevice(
	IDXGIDevice* dxgiDevice,
	CONST D2D1_CREATION_PROPERTIES* creationProperties,
	ID2D1Device** d2dDevice) {
	HRESULT hr = ORIG_D2D1CreateDevice(
		dxgiDevice,
		creationProperties,
		d2dDevice
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice);
	}
	MyDebug(L"IMPL_D2D1CreateDevice hooked");
	return hr;
}

HRESULT WINAPI IMPL_D2D1CreateDeviceContext(
	IDXGISurface* dxgiSurface,
	CONST D2D1_CREATION_PROPERTIES* creationProperties,
	ID2D1DeviceContext** d2dDeviceContext){
	HRESULT hr = ORIG_D2D1CreateDeviceContext(
		dxgiSurface,
		creationProperties,
		d2dDeviceContext
		);
	if SUCCEEDED(hr) {
		HookRenderTarget(*d2dDeviceContext, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_D2D1CreateDeviceContext hooked");
	return hr;
}

HRESULT WINAPI IMPL_D2D1CreateFactory(
	D2D1_FACTORY_TYPE factoryType,
	REFIID riid,
	const D2D1_FACTORY_OPTIONS* pFactoryOptions,
	void** ppIFactory){
	HRESULT hr = ORIG_D2D1CreateFactory(factoryType, riid, pFactoryOptions, ppIFactory);
	if (SUCCEEDED(hr)) {
		auto pUnknown = reinterpret_cast<IUnknown*>(*ppIFactory);
		ID2D1Factory* pD2D1Factory;
		HRESULT hr2 = pUnknown->QueryInterface(&pD2D1Factory);
		if (SUCCEEDED(hr2)) {
			HookFactory(pD2D1Factory);
			pD2D1Factory->Release();
		}
	}
	return hr;
}

HRESULT WINAPI IMPL_CreateWicBitmapRenderTarget(
	ID2D1Factory* This,
	IWICBitmap* target,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1RenderTarget** renderTarget 
	) {
	HRESULT hr = ORIG_CreateWicBitmapRenderTarget(
		This,
		target,
		renderTargetProperties,
		renderTarget
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*renderTarget, D2D1_RENDER_TARGET_CATEGORY);
	}
	MyDebug(L"IMPL_CreateWicBitmapRenderTarget hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateHwndRenderTarget(
	ID2D1Factory* This,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	const D2D1_HWND_RENDER_TARGET_PROPERTIES* hwndRenderTargetProperties,
	ID2D1HwndRenderTarget** hwndRenderTarget
	) {
	HRESULT hr = ORIG_CreateHwndRenderTarget(
		This,
		renderTargetProperties,
		hwndRenderTargetProperties,
		hwndRenderTarget
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*hwndRenderTarget, D2D1_RENDER_TARGET1_CATEGORY);
	}
	MyDebug(L"IMPL_CreateHwndRenderTarget hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDxgiSurfaceRenderTarget(
	ID2D1Factory* This,
	IDXGISurface* dxgiSurface,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1RenderTarget** renderTarget
	) {
	HRESULT hr = ORIG_CreateDxgiSurfaceRenderTarget(
		This,
		dxgiSurface,
		renderTargetProperties,
		renderTarget
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*renderTarget, D2D1_RENDER_TARGET_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDxgiSurfaceRenderTarget hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDCRenderTarget(
	ID2D1Factory* This,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1DCRenderTarget** dcRenderTarget
	) {
	HRESULT hr = ORIG_CreateDCRenderTarget(
		This,
		renderTargetProperties,
		dcRenderTarget
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*dcRenderTarget, D2D1_RENDER_TARGET1_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDCRenderTarget hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateCompatibleRenderTarget(
	ID2D1RenderTarget* This,
	CONST D2D1_SIZE_F* desiredSize,
	CONST D2D1_SIZE_U* desiredPixelSize,
	CONST D2D1_PIXEL_FORMAT* desiredFormat,
	D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS options,
	ID2D1BitmapRenderTarget** bitmapRenderTarget
	) {
	HRESULT hr = ORIG_CreateCompatibleRenderTarget(
		This,
		desiredSize,
		desiredPixelSize,
		desiredFormat,
		options,
		bitmapRenderTarget
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*bitmapRenderTarget, D2D1_RENDER_TARGET1_CATEGORY);
	}
	MyDebug(L"IMPL_CreateCompatibleRenderTarget hooked");
	return hr;
}

void WINAPI IMPL_D2D1RenderTarget_SetTextAntialiasMode(
	ID2D1RenderTarget* This,
	D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode
	) {
	MyDebug(L"IMPL_D2D1RenderTarget_SetTextAntialiasMode hooked");
	ORIG_D2D1RenderTarget_SetTextAntialiasMode(This, D2D1_TEXT_ANTIALIAS_MODE_DEFAULT);
}

void WINAPI IMPL_D2D1DeviceContext_SetTextAntialiasMode(
	ID2D1DeviceContext* This,
	D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode
	) {
	MyDebug(L"IMPL_D2D1DeviceContext_SetTextAntialiasMode hooked");
	ORIG_D2D1DeviceContext_SetTextAntialiasMode(This, D2D1_TEXT_ANTIALIAS_MODE_DEFAULT);
}

void WINAPI IMPL_D2D1RenderTarget_SetTextRenderingParams(
	ID2D1RenderTarget* This,
	_In_opt_ IDWriteRenderingParams* textRenderingParams
	) {
	MyDebug(L"IMPL_D2D1RenderTarget_SetTextRenderingParams hooked");
	ORIG_D2D1RenderTarget_SetTextRenderingParams(This, GetD2DRenderingParams(textRenderingParams));
}

void WINAPI IMPL_D2D1DeviceContext_SetTextRenderingParams(
	ID2D1DeviceContext* This,
	_In_opt_ IDWriteRenderingParams* textRenderingParams
	) {
	MyDebug(L"IMPL_D2D1DeviceContext_SetTextRenderingParams hooked");
	ORIG_D2D1DeviceContext_SetTextRenderingParams(This, GetD2DRenderingParams(textRenderingParams));
}

HRESULT WINAPI IMPL_CreateDeviceContext(
	ID2D1Device* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext** deviceContext
	) {
	HRESULT hr = ORIG_CreateDeviceContext(
		This,
		options,
		deviceContext
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext2(
	ID2D1Device1* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext1** deviceContext1
	) {
	HRESULT hr = ORIG_CreateDeviceContext2(
		This,
		options,
		deviceContext1
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext1, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext2 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext3(
	ID2D1Device2* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext2** deviceContext2
	) {
	HRESULT hr = ORIG_CreateDeviceContext3(
		This,
		options,
		deviceContext2
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext2, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext3 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext4(
	ID2D1Device3* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext3** deviceContext2
	) {
	HRESULT hr = ORIG_CreateDeviceContext4(
		This,
		options,
		deviceContext2
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext2, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext4 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext5(
	ID2D1Device4* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext4** deviceContext
	) {
	HRESULT hr = ORIG_CreateDeviceContext5(
		This,
		options,
		deviceContext
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext5 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext6(
	ID2D1Device5* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext5** deviceContext
	) {
	HRESULT hr = ORIG_CreateDeviceContext6(
		This,
		options,
		deviceContext
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext6 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDeviceContext7(
	ID2D1Device6* This,
	D2D1_DEVICE_CONTEXT_OPTIONS options,
	ID2D1DeviceContext6** deviceContext
	) {
	HRESULT hr = ORIG_CreateDeviceContext7(
		This,
		options,
		deviceContext
		);
	if (SUCCEEDED(hr)) {
		HookRenderTarget(*deviceContext, D2D1_DEVICE_CONTEXT_CATEGORY);
	}
	MyDebug(L"IMPL_CreateDeviceContext7 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice1(
	ID2D1Factory1* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device** d2dDevice
	) {
	HRESULT hr = ORIG_CreateDevice1(
		This,
		dxgiDevice,
		d2dDevice
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice);
	}
	MyDebug(L"IMPL_CreateDevice1 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice2(
	ID2D1Factory2* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device1** d2dDevice1
	){
	HRESULT hr = ORIG_CreateDevice2(
		This,
		dxgiDevice,
		d2dDevice1
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice1);
	}
	MyDebug(L"IMPL_CreateDevice2 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice3(
	ID2D1Factory3* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device2** d2dDevice2
	){
	HRESULT hr = ORIG_CreateDevice3(
		This,
		dxgiDevice,
		d2dDevice2
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice2);
	}
	MyDebug(L"IMPL_CreateDevice3 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice4(
	ID2D1Factory4* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device3** d2dDevice3
	){
	HRESULT hr = ORIG_CreateDevice4(
		This,
		dxgiDevice,
		d2dDevice3
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice3);
	}
	MyDebug(L"IMPL_CreateDevice4 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice5(
	ID2D1Factory5* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device4** d2dDevice4
	){
	HRESULT hr = ORIG_CreateDevice5(
		This,
		dxgiDevice,
		d2dDevice4
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice4);
	}
	MyDebug(L"IMPL_CreateDevice5 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice6(
	ID2D1Factory6* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device5** d2dDevice5
	){
	HRESULT hr = ORIG_CreateDevice6(
		This,
		dxgiDevice,
		d2dDevice5
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice5);
	}
	MyDebug(L"IMPL_CreateDevice6 hooked");
	return hr;
}

HRESULT WINAPI IMPL_CreateDevice7(
	ID2D1Factory7* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device6** d2dDevice6
	){
	HRESULT hr = ORIG_CreateDevice7(
		This,
		dxgiDevice,
		d2dDevice6
		);
	if (SUCCEEDED(hr)) {
		HookDevice(*d2dDevice6);
	}
	MyDebug(L"IMPL_CreateDevice7 hooked");
	return hr;
}


/*
bool CreateFontFace(IDWriteGdiInterop* gdi, IDWriteFont*** dfont, LOGFONT* lf)
{
__try
{
gdi->CreateFontFromLOGFONT(lf, *dfont);
return true;
}
__except(EXCEPTION_EXECUTE_HANDLER)
{
return false;
}
}*/

/*
void WINAPI IMPL_SetTextRenderingParams(ID2D1RenderTarget* self, __in_opt IDWriteRenderingParams *textRenderingParams = NULL)
{
return ORIG_SetTextRenderingParams(self, g_D2DParamsLarge.RenderingParams);
}

void WINAPI IMPL_SetTextAntialiasMode(ID2D1RenderTarget* self,  D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode)
{
return ORIG_SetTextAntialiasMode(self, g_D2DParamsLarge.AntialiasMode);
}*/

bool hookD2D1() {
	//MessageBox(NULL, L"HookD2D1", NULL, MB_OK);
	static bool loaded = [&] {
		return true;
	}();
	return loaded;
}

#define FAILEXIT { /*CoUninitialize();*/ return false;}
bool hookFontCreation(CComPtr<IDWriteFactory>& pDWriteFactory) {
	if (FAILED(pDWriteFactory->GetGdiInterop(&g_pGdiInterop))) FAILEXIT;	//判断不正确

/*
	HDC dc = CreateCompatibleDC(0);
	CComQIPtr<IDWriteBitmapRenderTarget> rt;
	g_pGdiInterop->CreateBitmapRenderTarget(dc, 1, 1, &rt);	//used to trigger CreateBitmapRenderTarget->DrawGlyphRun hook
	rt.Release();
	DeleteDC(dc);*/

	HOOK(pDWriteFactory, CreateTextFormat, 15);
	CComPtr<IDWriteFont> dfont = NULL;
	CComPtr<IDWriteFontCollection> fontcollection = NULL;
	CComPtr<IDWriteFontFamily> ffamily = NULL;
	if (FAILED(pDWriteFactory->GetSystemFontCollection(&fontcollection, false))) FAILEXIT;
	if (FAILED(fontcollection->GetFontFamily(0, &ffamily))) FAILEXIT;
	if (FAILED(ffamily->GetFont(0, &dfont))) FAILEXIT;

	CComPtr<IDWriteFont3> dfont3 = NULL;
	HRESULT hr = dfont->QueryInterface(&dfont3);
	if (FAILED(hr)) {
		HOOK(dfont, CreateFontFace, 13);
	} else {
		// IDWriteFont::CreateFontFace just wraps this
		HOOK(dfont3, CreateFontFace, 19);
	}
	return true;
}

bool hookDirectWrite(IUnknown ** factory)	//此函数需要改进以判断是否成功hook
{
	//CoInitialize(NULL);
#ifdef DEBUG
	//MessageBox(NULL, L"HookDW", NULL, MB_OK);
#endif
	static bool loaded = [&] {
		CComPtr<IDWriteFactory> pDWriteFactory;
		HRESULT hr1 = (*factory)->QueryInterface(&pDWriteFactory);
		if (FAILED(hr1)) FAILEXIT;
		HOOK(pDWriteFactory, CreateGlyphRunAnalysis, 23);
		HOOK(pDWriteFactory, GetGdiInterop, 17);
		const CGdippSettings* pSettings = CGdippSettings::GetInstance();
		// Windows8/8.1 is too buggy, GDIinterpo doesn't work correctly
		if (!pSettings->IsWindows81() && !pSettings->IsWindows8() && pSettings->DelayedInited() && pSettings->GetFontSubstitutesInfo().GetSize())
			hookFontCreation(pDWriteFactory);
		MyDebug(L"DW1 hooked");

		CComPtr<IDWriteFactory2> pDWriteFactory2;
		HRESULT hr2 = (*factory)->QueryInterface(&pDWriteFactory2);
		if (FAILED(hr2)) FAILEXIT;
		HOOK(pDWriteFactory2, CreateGlyphRunAnalysis2, 30);
		MyDebug(L"DW2 hooked");

		CComPtr<IDWriteFactory3> pDWriteFactory3;
		HRESULT hr3 = (*factory)->QueryInterface(&pDWriteFactory3);
		if (FAILED(hr3)) FAILEXIT;
		HOOK(pDWriteFactory3, CreateGlyphRunAnalysis3, 31);
		MyDebug(L"DW3 hooked");
		return true;
	}();
	return loaded;
}

#undef FAILEXIT
#define FAILEXIT {return;}
void TriggerHook(ID2D1Factory* d2d_factory) {
	const D2D1_PIXEL_FORMAT format = D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED);
	const D2D1_RENDER_TARGET_PROPERTIES properties =
		D2D1::RenderTargetProperties(D2D1_RENDER_TARGET_TYPE_DEFAULT, format, 0.0f, 0.0f, D2D1_RENDER_TARGET_USAGE_NONE);
	CComPtr<ID2D1DCRenderTarget>target;
	if (FAILED(d2d_factory->CreateDCRenderTarget(&properties, &target))) FAILEXIT;
}

// The entry point of DirectWrite hooking

// D2D1CreateFactory
// 	HookFactory
// 		ID2D1Factory
// 			CreateWicBitmapRenderTarget
// 			CreateHwndRenderTarget
// 			CreateDxgiSurfaceRenderTarget
// 			CreateDCRenderTarget
// 				HookRenderTarget ->
// 		ID2D1Factory<N>
// 			CreateDevice<N>
// 				HookDevice ->

// D2D1CreateDevice
// 	HookDevice
// 		ID2D1Device<N>
// 			CreateDeviceContext<N>
// 				HookRenderTarget ->

// D2D1CreateDeviceContext
// 	HookRenderTarget
// 		ID2D1RenderTarget
// 			CreateCompatibleRenderTarget
// 				HookRenderTarget ->
// 			DrawText
// 			DrawTextLayout
// 			DrawGlyphRun
// 			SetTextAntialiasMode
// 			SetTextRenderingParams
// 			GetFactory()
// 				HookFactory ->

// 		ID2D1DeviceContext
// 			DrawGlyphRun1
// 			GetDevice()
// 				HookDevice ->

// DWriteCreateFactory
// 	hookDirectWrite
// 		IDWriteFactory
// 			GetGdiInterop
// 				CreateBitmapRenderTarget
// 					IDWriteBitmapRenderTarget
// 						DrawGlyphRun

// 			CreateGlyphRunAnalysis
// 				IDWriteGlyphRunAnalysis
// 					GetAlphaBlendParams

// 			hookFontCreation
// 				CreateTextFormat
// 					IDWriteTextFormat
// 				IDWriteFont
// 					CreateFontFace
// 						IDWriteFontFace
// 				IDWriteFont3
// 					   CreateFontFace
// 					GetFontFaceReference()
// 						IDWriteFontFaceReference
// 							CreateFontFace
// 							CreateFontFaceWithSimulations

// 		IDWriteFactory<N>
// 			CreateGlyphRunAnalysis<N>
// 				IDWriteGlyphRunAnalysis
// 					GetAlphaBlendParams
void HookD2DDll()
{
	typedef HRESULT (WINAPI *PFN_DWriteCreateFactory)(
		_In_ DWRITE_FACTORY_TYPE factoryType,
		_In_ REFIID iid,
		_COM_Outptr_ IUnknown **factory
		);

	typedef HRESULT (WINAPI *PFN_D2D1CreateFactory)(
		D2D1_FACTORY_TYPE factoryType,
		REFIID riid,
		const D2D1_FACTORY_OPTIONS* pFactoryOptions,
		void** ppIFactory
		);
	//Sleep(30 * 1000);
#ifdef DEBUG
	MessageBox(0, L"HookD2DDll", NULL, MB_OK);
#endif
	HMODULE d2d1 = GetModuleHandle(_T("d2d1.dll"));
	if (!d2d1)
		d2d1 = LoadLibrary(_T("d2d1.dll"));
	HMODULE dw = GetModuleHandle(_T("dwrite.dll"));
	if (!dw)
		dw = LoadLibrary(_T("dwrite.dll"));
	void* D2D1Factory = GetProcAddress(d2d1, "D2D1CreateFactory");
	void* D2D1Device = GetProcAddress(d2d1, "D2D1CreateDevice");
	void* D2D1Context = GetProcAddress(d2d1, "D2D1CreateDeviceContext");
	void* DWFactory = GetProcAddress(dw, "DWriteCreateFactory");
	*(DWORD_PTR*)&ORIG_D2D1CreateFactory = (DWORD_PTR)D2D1Factory;
	*(DWORD_PTR*)&ORIG_D2D1CreateDevice = (DWORD_PTR)D2D1Device;
	*(DWORD_PTR*)&ORIG_D2D1CreateDeviceContext = (DWORD_PTR)D2D1Context;
	*(DWORD_PTR*)&ORIG_DWriteCreateFactory = (DWORD_PTR)DWFactory;
	if (DWFactory) {
		hook_demand_DWriteCreateFactory();
	}
	if (D2D1Factory){
		hook_demand_D2D1CreateFactory();
	}
	if (D2D1Device) {
		hook_demand_D2D1CreateDevice();
	}
	if (D2D1Context) {
		hook_demand_D2D1CreateDeviceContext();
	}
}

/*
void HookGdiplus()
{
InitGdiplusFuncs();
//*(DWORD_PTR*)&ORIG_D2D1CreateFactory = (DWORD_PTR)D2D1Factory;
*(DWORD_PTR*)&ORIG_GdipDrawString = (DWORD_PTR)pfnGdipDrawString;
hook_demand_GdipDrawString();
}

GpStatus WINAPI IMPL_GdipDrawString(
GpGraphics               *graphics,
GDIPCONST WCHAR          *string,
INT                       length,
GDIPCONST GpFont         *font,
GDIPCONST RectF          *layoutRect,
GDIPCONST GpStringFormat *stringFormat,
GDIPCONST GpBrush        *brush
)
{
#define GDIPEXEC ORIG_GdipDrawString(graphics, string, length, font, layoutRect, stringFormat, brush)
#define GDIPCHECK(x) if ((x)!=Ok) return GDIPEXEC
if (string)
{
HDC dc = NULL;
LOGFONTW lf = {0};
GpBrushType bt;
ARGB FontColor=0 ,bkColor = 0;
//GDIPLUS to gdi32 data preparation
GDIPCHECK(pfnGdipGetLogFontW((GpFont*)font, graphics, &lf));
GDIPCHECK(pfnGdipGetBrushType((GpBrush*)brush, &bt));
if (bt!=BrushTypeSolidColor) return GDIPEXEC; //only solid brush is supported by GDI32
GDIPCHECK(pfnGdipGetSolidFillColor((GpSolidFill*)brush, &FontColor));
if (FontColor>>24!=0xFF) return GDIPEXEC;	//only transparent and Opaque is supported.
GDIPCHECK(pfnGdipGetDC(graphics, &dc));
HFONT ft = CreateFontIndirectW(&lf);
HFONT oldfont = SelectFont(dc, ft);

SetTextColor(dc, FontColor & 0x00FFFFFF);
SetBkMode(dc, TRANSPARENT);
RECT gdiRect = {ROUND(layoutRect->X), ROUND(layoutRect->Y), ROUND(layoutRect->X+layoutRect->Width), ROUND(layoutRect->Y+layoutRect->Height)};
DrawText(dc, string, length, &gdiRect, DT_WORDBREAK);
//ExtTextOutW(dc, gdiRect.left, gdiRect.top, 0, &gdiRect, string, wcslen(string), NULL);

SelectObject(dc, oldfont);
DeleteObject(ft);
pfnGdipReleaseDC(graphics, dc);
return Ok;
}
else
return ORIG_GdipDrawString(graphics, string, length, font, layoutRect, stringFormat, brush);
#undef GDIPCHECK
#undef GDIPEXEC
}
*/
HRESULT WINAPI IMPL_DWriteCreateFactory(__in DWRITE_FACTORY_TYPE factoryType,
	__in REFIID iid,
	__out IUnknown **factory)
{
	HRESULT ret = ORIG_DWriteCreateFactory(factoryType, iid, factory); 
	if (SUCCEEDED(ret))
		hookDirectWrite(factory);
	return ret;
}

HRESULT WINAPI IMPL_CreateFontFace(IDWriteFont* self,
	__out IDWriteFontFace** fontFace)
{
	HRESULT ret = ORIG_CreateFontFace(self, fontFace);
	if (ret == S_OK)
	{
		/*static bool loaded = [&] {
			CComPtr<IDWriteFontFace3> dfont3 = NULL;
			HRESULT hr = self->QueryInterface(&dfont3);
			if (SUCCEEDED(hr)) {
				CComPtr<IDWriteFontFaceReference> ffref;
				if (SUCCEEDED(dfont3->GetFontFaceReference(&ffref)) && ffref) {
					// Same as IDWriteFontFaceReference1::CreateFontFace
					HOOK(ffref, DWriteFontFaceReference_CreateFontFace, 3);
					HOOK(ffref, DWriteFontFaceReference_CreateFontFaceWithSimulations, 4);
				}
			}
			return true;
		}();*/

		LOGFONT lf = { 0 };
		if (FAILED(g_pGdiInterop->ConvertFontFaceToLOGFONT(*fontFace, &lf)))
			return ret;
		const CGdippSettings* pSettings = CGdippSettings::GetInstance();
		if (pSettings->CopyForceFont(lf, lf))
		{
			IDWriteFont* writefont = NULL;
			if (FAILED(g_pGdiInterop->CreateFontFromLOGFONT(&lf, &writefont)))
				return ret;
			(*fontFace)->Release();
			ORIG_CreateFontFace(writefont, fontFace);
			writefont->Release();
		}
	}
	return ret;
}

bool SubstituteDWriteFont3(__out IDWriteFontFace3** fontFace3)
{
	LOGFONT lf = { 0 };
	if (FAILED(g_pGdiInterop->ConvertFontFaceToLOGFONT(*fontFace3, &lf)))
		return false;
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	if (pSettings->CopyForceFont(lf, lf))
	{
		CComPtr<IDWriteFont> writefont;
		if (FAILED(g_pGdiInterop->CreateFontFromLOGFONT(&lf, &writefont)))
			return false;

		CComPtr<IDWriteFontFace> fontFaceOut;
		ORIG_CreateFontFace(writefont, &fontFaceOut);
		IDWriteFontFace3* fontFace3Out;
		if (FAILED(fontFaceOut->QueryInterface(&fontFace3Out)))
			return false;
		
		(*fontFace3)->Release();
		*fontFace3 = fontFace3Out;
	}
	return true;
}

HRESULT WINAPI IMPL_DWriteFontFaceReference_CreateFontFace(
	IDWriteFontFaceReference* self,
	__out IDWriteFontFace3** fontFace)
{
	HRESULT ret = ORIG_DWriteFontFaceReference_CreateFontFace(self, fontFace);
	if (ret == S_OK) {
		SubstituteDWriteFont3(fontFace);
	}
	return ret;
}

HRESULT WINAPI IMPL_DWriteFontFaceReference_CreateFontFaceWithSimulations(
	IDWriteFontFaceReference* self,
	DWRITE_FONT_SIMULATIONS fontFaceSimulationFlags,
	__out IDWriteFontFace3** fontFace)
{
	HRESULT ret = ORIG_DWriteFontFaceReference_CreateFontFaceWithSimulations(self,
		fontFaceSimulationFlags, fontFace);
	if (ret == S_OK) {
		SubstituteDWriteFont3(fontFace);
	}
	return ret;
}

HRESULT  WINAPI IMPL_CreateTextFormat(IDWriteFactory* self,
	__in_z WCHAR const* fontFamilyName,
	__maybenull IDWriteFontCollection* fontCollection,
	DWRITE_FONT_WEIGHT fontWeight,
	DWRITE_FONT_STYLE fontStyle,
	DWRITE_FONT_STRETCH fontStretch,
	FLOAT fontSize,
	__in_z WCHAR const* localeName,
	__out IDWriteTextFormat** textFormat)
{
	LOGFONT lf = { 0 };
	StringCchCopy(lf.lfFaceName, LF_FACESIZE, fontFamilyName);
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	if (pSettings->CopyForceFont(lf, lf))
		return ORIG_CreateTextFormat(self, lf.lfFaceName, fontCollection, fontWeight, fontStyle, fontStretch, fontSize, localeName, textFormat);
	else
		return ORIG_CreateTextFormat(self, fontFamilyName, fontCollection, fontWeight, fontStyle, fontStretch, fontSize, localeName, textFormat);
}

void WINAPI IMPL_D2D1RenderTarget_DrawGlyphRun1(
	ID2D1DeviceContext *This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	CONST DWRITE_GLYPH_RUN_DESCRIPTION *glyphRunDescription,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1RenderTarget1_DrawGlyphRun1(
	ID2D1DeviceContext *This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	CONST DWRITE_GLYPH_RUN_DESCRIPTION *glyphRunDescription,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget1_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget1_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1DeviceContext_DrawGlyphRun1(
	ID2D1DeviceContext *This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	CONST DWRITE_GLYPH_RUN_DESCRIPTION *glyphRunDescription,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1DeviceContext_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1DeviceContext_DrawGlyphRun1(
			This,
			baselineOrigin,
			glyphRun,
			glyphRunDescription,
			foregroundBrush,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1RenderTarget_DrawGlyphRun(
	ID2D1RenderTarget* This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1RenderTarget1_DrawGlyphRun(
	ID2D1RenderTarget* This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget1_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget1_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1DeviceContext_DrawGlyphRun(
	ID2D1DeviceContext* This,
	D2D1_POINT_2F baselineOrigin,
	CONST DWRITE_GLYPH_RUN *glyphRun,
	ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1DeviceContext_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1DeviceContext_DrawGlyphRun(
			This,
			baselineOrigin,
			glyphRun,
			foregroundBrush,
			measuringMode
			);
	}
}

HRESULT WINAPI IMPL_BitmapRenderTarget_DrawGlyphRun(
	IDWriteBitmapRenderTarget* This,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	DWRITE_MEASURING_MODE measuringMode,
	DWRITE_GLYPH_RUN const* glyphRun,
	IDWriteRenderingParams* renderingParams,
	COLORREF textColor,
	RECT* blackBoxRect)
{
	Params* dwParams = GetDWParams();
	HRESULT hr = E_FAIL;
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		DWRITE_MATRIX prev;
		hr = This->GetCurrentTransform(&prev);
		if (SUCCEEDED(hr)) {
			DWRITE_MATRIX rotate = prev;
			rotate.m12 += 1.0f / 0xFFFF;
			rotate.m21 += 1.0f / 0xFFFF;
			hr = This->SetCurrentTransform(&rotate);
			if (SUCCEEDED(hr)) {
				hr = ORIG_BitmapRenderTarget_DrawGlyphRun(
					This,
					baselineOriginX,
					baselineOriginY,
					measuringMode,
					glyphRun,
					GetDWRenderingParams(renderingParams),
					textColor,
					blackBoxRect
					);
				This->SetCurrentTransform(&prev);
			}
		}
	}
	if (FAILED(hr)) {
		hr = ORIG_BitmapRenderTarget_DrawGlyphRun(
			This,
			baselineOriginX,
			baselineOriginY,
			measuringMode,
			glyphRun,
			GetDWRenderingParams(renderingParams),
			textColor,
			blackBoxRect
			);
		if SUCCEEDED(hr) {
			MyDebug(L"DrawGlyphRun hooked");
		}
	}
	if (FAILED(hr)) {
		hr = ORIG_BitmapRenderTarget_DrawGlyphRun(
			This,
			baselineOriginX,
			baselineOriginY,
			measuringMode,
			glyphRun,
			renderingParams,
			textColor,
			blackBoxRect
			);
		if SUCCEEDED(hr) {
			MyDebug(L"DrawGlyphRun failed");
		}
	}
	MyDebug(L"DrawGlyphRun hooked");
	return hr;
}


void WINAPI IMPL_D2D1RenderTarget_DrawText(
	ID2D1RenderTarget* This,
	CONST WCHAR *string,
	UINT32 stringLength,
	IDWriteTextFormat *textFormat,
	CONST D2D1_RECT_F *layoutRect,
	ID2D1Brush *defaultForegroundBrush,
	D2D1_DRAW_TEXT_OPTIONS options,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget_DrawText(
			This,
			string,
			stringLength,
			textFormat,
			layoutRect,
			defaultForegroundBrush,
			options,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget_DrawText(
			This,
			string,
			stringLength,
			textFormat,
			layoutRect,
			defaultForegroundBrush,
			options,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1DeviceContext_DrawText(
	ID2D1DeviceContext* This,
	CONST WCHAR *string,
	UINT32 stringLength,
	IDWriteTextFormat *textFormat,
	CONST D2D1_RECT_F *layoutRect,
	ID2D1Brush *defaultForegroundBrush,
	D2D1_DRAW_TEXT_OPTIONS options,
	DWRITE_MEASURING_MODE measuringMode
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1DeviceContext_DrawText(
			This,
			string,
			stringLength,
			textFormat,
			layoutRect,
			defaultForegroundBrush,
			options,
			measuringMode
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1DeviceContext_DrawText(
			This,
			string,
			stringLength,
			textFormat,
			layoutRect,
			defaultForegroundBrush,
			options,
			measuringMode
			);
	}
}

void WINAPI IMPL_D2D1RenderTarget_DrawTextLayout(
	ID2D1RenderTarget* This,
	D2D1_POINT_2F origin,
	IDWriteTextLayout *textLayout,
	ID2D1Brush *defaultForegroundBrush,
	D2D1_DRAW_TEXT_OPTIONS options
	) {
	Params* dwParams = GetDWParams();
	if (dwParams->GridFitMode == DWRITE_GRID_FIT_MODE_DISABLED) {
		D2D1_MATRIX_3X2_F prev;
		This->GetTransform(&prev);
		D2D1_MATRIX_3X2_F rotate = prev;
		rotate.m12 += 1.0f / 0xFFFF;
		rotate.m21 += 1.0f / 0xFFFF;
		This->SetTransform(&rotate);
		ORIG_D2D1RenderTarget_DrawTextLayout(
			This,
			origin,
			textLayout,
			defaultForegroundBrush,
			options
			);
		This->SetTransform(&prev);
	}
	else {
		ORIG_D2D1RenderTarget_DrawTextLayout(
			This,
			origin,
			textLayout,
			defaultForegroundBrush,
			options
			);
	}
}