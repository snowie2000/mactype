#include "directwrite.h"
#include "settings.h"
#include "dynCodeHelper.h"

void MyDebug(const TCHAR * sz, ...)
{
#ifdef DEBUG
	TCHAR szData[512] = { 0 };

	va_list args;
	va_start(args, sz);
	StringCchPrintf(szData, sizeof(szData)-1, sz, args);
	va_end(args);

	OutputDebugString(szData);
#endif
}

#define SET_VAL(x, y) *(DWORD_PTR*)&(x) = *(DWORD_PTR*)&(y)
#define HOOK(obj, name, index) { \
	if (!HOOK_##name.Link) {  \
		AutoEnableDynamicCodeGen dynHelper(true);  \
		SET_VAL(ORIG_##name, (*reinterpret_cast<void***>(obj.p))[index]);  \
		hook_demand_##name(false);  \
		if (!HOOK_##name.Link) { MyDebug(L"##name hook failed"); }  \
	}  \
};



struct Params {
	D2D1_TEXT_ANTIALIAS_MODE AntialiasMode;
	IDWriteRenderingParams *RenderingParams;
	IDWriteRenderingParams *DWRenderingParams;	//RenderingMode=6 is invalid for DWrite interface

	FLOAT Gamma;
	FLOAT EnhancedContrast;
	FLOAT ClearTypeLevel;
	DWRITE_PIXEL_GEOMETRY PixelGeometry;
	DWRITE_RENDERING_MODE RenderingMode;
	FLOAT GrayscaleEnhancedContrast;
	DWRITE_GRID_FIT_MODE GridFitMode;
	DWRITE_RENDERING_MODE1 RenderingMode1;
};

Params g_D2DParams, g_D2DParamsLarge;
LONG bDWLoaded = false, bD2D1Loaded = false, bParamInited = false;
IDWriteFactory* g_pDWriteFactory = NULL;
IDWriteGdiInterop* g_pGdiInterop = NULL;

IDWriteRenderingParams* CreateParam(Params& d2dParams,  IDWriteFactory *dw_factory)
{
	IDWriteFactory3* dw3 = NULL;
	IDWriteFactory2* dw2 = NULL;
	IDWriteFactory1* dw1 = NULL;
	IDWriteRenderingParams3* r3 = NULL;
	IDWriteRenderingParams2* r2 = NULL;
	IDWriteRenderingParams1* r1 = NULL;
	IDWriteRenderingParams* r0 = NULL;

	HRESULT hr = dw_factory->QueryInterface(&dw3);
	if SUCCEEDED(hr) {
		hr = dw3->CreateCustomRenderingParams(
			d2dParams.Gamma,
			d2dParams.EnhancedContrast,
			d2dParams.GrayscaleEnhancedContrast,
			d2dParams.ClearTypeLevel,
			d2dParams.PixelGeometry,
			d2dParams.RenderingMode1,
			d2dParams.GridFitMode,
			&r3);
		dw3->Release();
		if SUCCEEDED(hr) {
			return r3;
		}
	}

	hr = dw_factory->QueryInterface(&dw2);
	if SUCCEEDED(hr) {
		hr = dw2->CreateCustomRenderingParams(
			d2dParams.Gamma,
			d2dParams.EnhancedContrast,
			d2dParams.GrayscaleEnhancedContrast,
			d2dParams.ClearTypeLevel,
			d2dParams.PixelGeometry,
			d2dParams.RenderingMode,
			d2dParams.GridFitMode,
			&r2);
		dw2->Release();
		if SUCCEEDED(hr) {
			return r2;
		}
	}

	hr = dw_factory->QueryInterface(&dw1);
	if SUCCEEDED(hr) {
		hr = dw1->CreateCustomRenderingParams(
			d2dParams.Gamma,
			d2dParams.EnhancedContrast,
			d2dParams.GrayscaleEnhancedContrast,
			d2dParams.ClearTypeLevel,
			d2dParams.PixelGeometry,
			d2dParams.RenderingMode,
			&r1);
		dw1->Release();
		if SUCCEEDED(hr) {
			return r1;
		}
	}

	if (SUCCEEDED(dw_factory->CreateCustomRenderingParams(
		d2dParams.Gamma,
		d2dParams.EnhancedContrast,
		d2dParams.ClearTypeLevel,
		d2dParams.PixelGeometry,
		d2dParams.RenderingMode,
		&r0)))
		return r0;

	return NULL;
}

bool MakeD2DParams(IDWriteFactory* dw_factory)
{
	//CComPtr<IDWriteRenderingParams> dwrpm;

	//if (FAILED(dw_factory->CreateRenderingParams(&dwrpm)))
	//	return false;
	if (InterlockedExchange((LONG*)&bParamInited, true)) return false;

	CComPtr<IDWriteFactory> pDWriteFactory;
	if (NULL == dw_factory) {
		ORIG_DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED,
			__uuidof(IDWriteFactory),
			reinterpret_cast<IUnknown**>(&pDWriteFactory));
		dw_factory = pDWriteFactory;
	}

	const CGdippSettings* pSettings = CGdippSettings::GetInstanceNoInit();
	//
	g_D2DParams.Gamma = pSettings->GammaValueForDW();	//user defined value preferred.
	//if (g_D2DParams.Gamma == 0)
	//	g_D2DParams.Gamma = pSettings->GammaValue()*pSettings->GammaValue() > 1.3 ? pSettings->GammaValue()*pSettings->GammaValue() / 2 : 0.7f;
	g_D2DParams.EnhancedContrast = pSettings->ContrastForDW();
	g_D2DParams.ClearTypeLevel = pSettings->ClearTypeLevelForDW();
	switch (pSettings->GetFontSettings().GetAntiAliasMode())
	{
	case 2:
	case 4:
		g_D2DParams.PixelGeometry = DWRITE_PIXEL_GEOMETRY_RGB;
		break;
	case 3:
	case 5:
		g_D2DParams.PixelGeometry = DWRITE_PIXEL_GEOMETRY_BGR;
		break;
	default:
		g_D2DParams.PixelGeometry = DWRITE_PIXEL_GEOMETRY_FLAT;
	}

	g_D2DParams.AntialiasMode = (D2D1_TEXT_ANTIALIAS_MODE)D2D1_TEXT_ANTIALIAS_MODE_DEFAULT;
	g_D2DParams.RenderingMode = (DWRITE_RENDERING_MODE)pSettings->RenderingModeForDW();
	g_D2DParams.GrayscaleEnhancedContrast = pSettings->ContrastForDW();
	g_D2DParams.GridFitMode = DWRITE_GRID_FIT_MODE_DISABLED;
	g_D2DParams.RenderingMode1 = (DWRITE_RENDERING_MODE1)pSettings->RenderingModeForDW();
	/*
	if (IsWindows8OrGreater()) {	//optimized for win8/win10
		g_D2DParams.GrayscaleEnhancedContrast = 1.0f;
		g_D2DParams.EnhancedContrast = 1.0f;
	}*/

	g_D2DParams.RenderingParams = CreateParam(g_D2DParams, dw_factory);
	if (pSettings->RenderingModeForDW() == 6) {	//DW rendering in mode6 is horrible
		g_D2DParams.RenderingMode = DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC;
		g_D2DParams.RenderingMode1 = DWRITE_RENDERING_MODE1_NATURAL_SYMMETRIC;
		g_D2DParams.DWRenderingParams = CreateParam(g_D2DParams, dw_factory);
	}
	else{
		g_D2DParams.DWRenderingParams = g_D2DParams.RenderingParams;	// otherwise, we just make them equal.
	}
	return g_D2DParams.DWRenderingParams != NULL && g_D2DParams.RenderingParams!=NULL;
}

void HookFactory(ID2D1Factory* pD2D1Factory) {
	if (!MakeD2DParams(NULL)) return;
	{//factory
		CComQIPtr<ID2D1Factory> ptr = pD2D1Factory;
		HOOK(ptr, CreateWicBitmapRenderTarget, 13);
		HOOK(ptr, CreateHwndRenderTarget, 14);
		HOOK(ptr, CreateDxgiSurfaceRenderTarget, 15);
		HOOK(ptr, CreateDCRenderTarget, 16);
	}
	{//factory1
	CComPtr<ID2D1Factory1> ptr;
	HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
	if (SUCCEEDED(hr)){
		HOOK(ptr, CreateDevice1, 17);
	}
}
	{//factory2
		CComPtr<ID2D1Factory2> ptr;
		HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
		if (SUCCEEDED(hr)){
			HOOK(ptr, CreateDevice2, 27);
		}
	}
	{//factory3
		CComPtr<ID2D1Factory3> ptr;
		HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
		if (SUCCEEDED(hr)){
			HOOK(ptr, CreateDevice3, 28);
		}
	}
	{//factory4
		CComPtr<ID2D1Factory4> ptr;
		HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
		if (SUCCEEDED(hr)){
			HOOK(ptr, CreateDevice4, 29);
		}
	}
	{//factory5
		CComPtr<ID2D1Factory5> ptr;
		HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
		if (SUCCEEDED(hr)){
			HOOK(ptr, CreateDevice5, 30);
		}
	}
	{//factory6
		CComPtr<ID2D1Factory6> ptr;
		HRESULT hr = pD2D1Factory->QueryInterface(&ptr);
		if (SUCCEEDED(hr)){
			HOOK(ptr, CreateDevice6, 31);
		}
	}
}

void HookRenderTarget(ID2D1RenderTarget* pD2D1RenderTarget) {
	static bool loaded = false;
	if (!loaded) {
		loaded = true;
		CComQIPtr<ID2D1RenderTarget> ptr = pD2D1RenderTarget;

		HOOK(ptr, CreateCompatibleRenderTarget, 12);
		//HOOK(ptr, DrawGlyphRun2, 29);
		HOOK(ptr, SetTextAntialiasMode, 34);
		HOOK(ptr, SetTextRenderingParams, 36);

		ID2D1Factory* pD2D1Factory;
		pD2D1RenderTarget->GetFactory(&pD2D1Factory);
		if (pD2D1Factory)
			HookFactory(pD2D1Factory);
	}
	pD2D1RenderTarget->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_DEFAULT);
	pD2D1RenderTarget->SetTextRenderingParams(g_D2DParams.RenderingParams);
}

void HookDevice(ID2D1Device* d2dDevice){
	CComQIPtr<ID2D1Device> ptr = d2dDevice;
	HOOK(ptr, CreateDeviceContext, 4);

	CComPtr<ID2D1Device1> ptr2;
	HRESULT hr = (d2dDevice)->QueryInterface(&ptr2);
	if SUCCEEDED(hr) {
		HOOK(ptr2, CreateDeviceContext2, 11);
	}
	CComPtr<ID2D1Device2> ptr3;
	hr = (d2dDevice)->QueryInterface(&ptr3);
	if SUCCEEDED(hr) {
		HOOK(ptr3, CreateDeviceContext3, 12);
	}
	CComPtr<ID2D1Device3> ptr4;
	hr = (d2dDevice)->QueryInterface(&ptr4);
	if SUCCEEDED(hr) {
		HOOK(ptr4, CreateDeviceContext4, 15);
	}
	CComPtr<ID2D1Device4> ptr5;
	hr = (d2dDevice)->QueryInterface(&ptr5);
	if SUCCEEDED(hr) {
		HOOK(ptr5, CreateDeviceContext5, 16);
	}
	CComPtr<ID2D1Device5> ptr6;
	hr = (d2dDevice)->QueryInterface(&ptr6);
	if SUCCEEDED(hr) {
		HOOK(ptr6, CreateDeviceContext6, 17);
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
			DWRITE_MATRIX m;
			DWRITE_MATRIX const* pm = transform;
			if (pm) {
				m = *transform;
				m.m11 *= pixelsPerDip;
				m.m22 *= pixelsPerDip;
				pm = &m;
			}
			hr = f->CreateGlyphRunAnalysis(
				glyphRun,
				pm,
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
		hr = ORIG_CreateGlyphRunAnalysis(
			This,
			glyphRun,
			pixelsPerDip,
			transform,
			g_D2DParams.RenderingMode,
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			CComQIPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
		}
	}
	return hr;
}

HRESULT WINAPI IMPL_GetGdiInterop(
	IDWriteFactory* This,
	IDWriteGdiInterop** gdiInterop
	) 
{
	static bool loaded = false;

	HRESULT hr = ORIG_GetGdiInterop(This, gdiInterop);
	if (!loaded) {
		loaded = true;
		CComQIPtr<IDWriteGdiInterop> gdip = *gdiInterop;
		HOOK(gdip, CreateBitmapRenderTarget, 7);
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			CComQIPtr<IDWriteBitmapRenderTarget> ptr = *renderTarget;
			HOOK(ptr, DrawGlyphRun, 3);
		}
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
			g_D2DParams.RenderingParams,
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
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE_ALIASED) {
		hr = ORIG_CreateGlyphRunAnalysis2(
			This,
			glyphRun,
			transform,
			g_D2DParams.RenderingMode,
			measuringMode,
			g_D2DParams.GridFitMode,
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			CComQIPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
		}
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
	HRESULT hr = E_FAIL;
	if (FAILED(hr) && renderingMode != DWRITE_RENDERING_MODE1_ALIASED) {
		hr = ORIG_CreateGlyphRunAnalysis3(
			This,
			glyphRun,
			transform,
			g_D2DParams.RenderingMode1,
			measuringMode,
			g_D2DParams.GridFitMode,
			antialiasMode,
			baselineOriginX,
			baselineOriginY,
			glyphRunAnalysis
			);
	}
	if (FAILED(hr)) {
		MyDebug(L"CreateGlyphRunAnalysis3 failed");
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			CComQIPtr<IDWriteGlyphRunAnalysis> ptr = *glyphRunAnalysis;
			HOOK(ptr, GetAlphaBlendParams, 5);
		}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice);
		}
	}
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
		HookRenderTarget(*d2dDeviceContext);
	}
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
		HookRenderTarget(*renderTarget);
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
		HookRenderTarget(*hwndRenderTarget);
	}
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
		HookRenderTarget(*renderTarget);
	}
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
		HookRenderTarget(*dcRenderTarget);
	}
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
		HookRenderTarget(*bitmapRenderTarget);
	}
	return hr;
}

void WINAPI IMPL_SetTextAntialiasMode(
	ID2D1RenderTarget* This,
	D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode
	) {
	MyDebug(L"IMPL_SetTextAntialiasMode hooked");
	ORIG_SetTextAntialiasMode(This, D2D1_TEXT_ANTIALIAS_MODE_DEFAULT);
}

void WINAPI IMPL_SetTextRenderingParams(
	ID2D1RenderTarget* This,
	_In_opt_ IDWriteRenderingParams* textRenderingParams
	) {
	MyDebug(L"IMPL_SetTextRenderingParams hooked");
	ORIG_SetTextRenderingParams(This, g_D2DParams.RenderingParams);
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
		HookRenderTarget(*deviceContext);
	}
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
		HookRenderTarget(*deviceContext1);
	}
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
		HookRenderTarget(*deviceContext2);
	}
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
		HookRenderTarget(*deviceContext2);
	}
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
		HookRenderTarget(*deviceContext);
	}
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
		HookRenderTarget(*deviceContext);
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice);
		}
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice1);
		}
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice2);
		}
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice3);
		}
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice4);
		}
	}
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
		static bool loaded = false;
		if (!loaded) {
			loaded = true;
			HookDevice(*d2dDevice5);
		}
	}
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
	if (InterlockedExchange((LONG*)&bD2D1Loaded, true)) return false;

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
	HOOK(dfont, CreateFontFace, 13);
	return true;
}

bool hookDirectWrite(IUnknown ** factory)	//此函数需要改进以判断是否成功hook
{
	//CoInitialize(NULL);
#ifdef DEBUG
	//MessageBox(NULL, L"HookDW", NULL, MB_OK);
#endif
	if (InterlockedExchange((LONG*)&bDWLoaded, true)) return false;

	//dwrite 1
	CComPtr<IDWriteFactory> pDWriteFactory;
	HRESULT hr1 = (*factory)->QueryInterface(&pDWriteFactory);
	if (FAILED(hr1)) FAILEXIT;
	HOOK(pDWriteFactory, CreateGlyphRunAnalysis, 23);
	HOOK(pDWriteFactory, GetGdiInterop, 17);
	hookFontCreation(pDWriteFactory);
	if (!MakeD2DParams(pDWriteFactory)) FAILEXIT;

	MyDebug(L"DW1 hooked");
	//dwrite2
	CComPtr<IDWriteFactory2> pDWriteFactory2;
	HRESULT hr2 = (*factory)->QueryInterface(&pDWriteFactory2);
	if (FAILED(hr2)) FAILEXIT;
	HOOK(pDWriteFactory2, CreateGlyphRunAnalysis2, 30);
	MyDebug(L"DW2 hooked");
	//dwrite3
	CComPtr<IDWriteFactory3> pDWriteFactory3;
	HRESULT hr3 = (*factory)->QueryInterface(&pDWriteFactory3);
	if (FAILED(hr3)) FAILEXIT;
	HOOK(pDWriteFactory3, CreateGlyphRunAnalysis3, 31);
	MyDebug(L"DW3 hooked");
	return true;

	/*
	if (FAILED(g_pDWriteFactory->GetGdiInterop(&g_pGdiInterop))) FAILEXIT;	//判断不正确

	if (!MakeD2DParams(g_pDWriteFactory)) FAILEXIT;	//准备创建渲染用的参数

	const D2D1_PIXEL_FORMAT format =D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,	D2D1_ALPHA_MODE_PREMULTIPLIED);
	const D2D1_RENDER_TARGET_PROPERTIES properties =
	D2D1::RenderTargetProperties(D2D1_RENDER_TARGET_TYPE_DEFAULT, format,	0.0f,0.0f,D2D1_RENDER_TARGET_USAGE_NONE);
	CComPtr<ID2D1DCRenderTarget>target;
	if (FAILED(d2d_factory->CreateDCRenderTarget(&properties, &target))) FAILEXIT;
	void* pDraw = (*reinterpret_cast<void***>(target.p))[29];
	//void* pRenderParam = (*reinterpret_cast<void***>(target.p))[36];
	//void* pAAParam = (*reinterpret_cast<void***>(target.p))[34];
	*(DWORD_PTR*)&ORIG_DrawGlyphRun= *(DWORD_PTR*)&pDraw;	//强制赋值，无视类型转换
	//*(DWORD_PTR*)&ORIG_SetTextRenderingParams= *(DWORD_PTR*)&pRenderParam;
	//*(DWORD_PTR*)&ORIG_SetTextAntialiasMode= *(DWORD_PTR*)&pAAParam;
	hook_demand_DrawGlyphRun();	//加载D2D hook
	//hook_demand_SetTextAntialiasMode();
	//hook_demand_SetTextRenderingParams();
	void* pTextFormat = (*reinterpret_cast<void***>(g_pDWriteFactory))[15];
	IDWriteFont * dfont = NULL;
	IDWriteFontCollection * fontcollection = NULL;
	IDWriteFontFamily* ffamily = NULL;
	if (FAILED(g_pDWriteFactory->GetSystemFontCollection(&fontcollection, false))) FAILEXIT;
	if (FAILED(fontcollection->GetFontFamily(0, &ffamily))) FAILEXIT;
	if (FAILED(ffamily->GetFont(0, &dfont))) FAILEXIT;
	ffamily->Release();
	fontcollection->Release();
	void* pCreateFontFace = (*reinterpret_cast<void***>(dfont))[13];
	*(DWORD_PTR*)&ORIG_CreateTextFormat= *(DWORD_PTR*)&pTextFormat;	//强制赋值，无视类型转换
	*(DWORD_PTR*)&ORIG_CreateFontFace= *(DWORD_PTR*)&pCreateFontFace;
	hook_demand_CreateTextFormat();
	hook_demand_CreateFontFace();
	dfont->Release();

	return true;*/
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
	//MessageBox(0, L"HookD2DDll", NULL, MB_OK);
#endif
	HMODULE d2d1 = LoadLibrary(_T("d2d1.dll"));
	HMODULE dw = LoadLibrary(_T("dwrite.dll"));
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
	/*if (dw) {
		MyDebug(L"DW hooked for PID %d", GetCurrentProcessId());
		CComPtr<IDWriteFactory> pDWriteFactory;
		(PFN_DWriteCreateFactory(DWFactory))(DWRITE_FACTORY_TYPE_ISOLATED,
			__uuidof(IDWriteFactory),
			reinterpret_cast<IUnknown**>(&pDWriteFactory));
		MakeD2DParams(pDWriteFactory);
	}*/
	/*if (d2d1) {
		CComQIPtr<ID2D1Factory> pD2dfactory;
		D2D1_FACTORY_OPTIONS d2d1Option = { D2D1_DEBUG_LEVEL_NONE };
		(PFN_D2D1CreateFactory(D2D1Factory))(D2D1_FACTORY_TYPE_SINGLE_THREADED, __uuidof(ID2D1Factory), &d2d1Option, reinterpret_cast<void**>(&pD2dfactory));
		TriggerHook(pD2dfactory);
	}*/
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
	if (!bDWLoaded && SUCCEEDED(ret))
		hookDirectWrite(factory);
	return ret;
}

HRESULT WINAPI IMPL_CreateFontFace(IDWriteFont* self,
	__out IDWriteFontFace** fontFace)
{
	HRESULT ret = ORIG_CreateFontFace(self, fontFace);
	if (ret == S_OK)
	{
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

HRESULT WINAPI IMPL_DrawGlyphRun(
	IDWriteBitmapRenderTarget* This,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	DWRITE_MEASURING_MODE measuringMode,
	DWRITE_GLYPH_RUN const* glyphRun,
	IDWriteRenderingParams* renderingParams,
	COLORREF textColor,
	RECT* blackBoxRect)
{
	HRESULT hr = E_FAIL;
	if (FAILED(hr)) {
		hr = ORIG_DrawGlyphRun(
			This,
			baselineOriginX,
			baselineOriginY,
			measuringMode,
			glyphRun,
			g_D2DParams.DWRenderingParams,
			textColor,
			blackBoxRect
			);
		if SUCCEEDED(hr) {
			MyDebug(L"DrawGlyphRun hooked");
		}
	}
	if (FAILED(hr)) {
		hr = ORIG_DrawGlyphRun(
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

/*
void WINAPI IMPL_DrawGlyphRun2(
	D2D1_POINT_2F baselineOrigin,
	_In_ CONST DWRITE_GLYPH_RUN *glyphRun,
	_In_ ID2D1Brush *foregroundBrush,
	DWRITE_MEASURING_MODE measuringMode = DWRITE_MEASURING_MODE_NATURAL) 
{
	return ORIG_DrawGlyphRun2(baselineOrigin, glyphRun, foregroundBrush, measuringMode);
}*/
