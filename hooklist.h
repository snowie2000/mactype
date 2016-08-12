// ここにフックするAPIを列挙していく。
// ORIG_foo を使いつつ、IMPL_foo を実装すること。

/*
HOOK_DEFINE(DWORD, GetTabbedTextExtentA, (HDC hdc, LPCSTR lpString, int nCount, int nTabPositions, CONST LPINT lpnTabStopPositions))
HOOK_DEFINE(DWORD, GetTabbedTextExtentW, (HDC hdc, LPCWSTR lpString, int nCount, int nTabPositions, CONST LPINT lpnTabStopPositions))

HOOK_DEFINE(BOOL, GetTextExtentExPointA, (HDC hdc, LPCSTR lpszStr, int cchString, int nMaxExtent, LPINT lpnFit, LPINT lpDx, LPSIZE lpSize))
HOOK_DEFINE(BOOL, GetTextExtentExPointW, (HDC hdc, LPCWSTR lpszStr, int cchString, int nMaxExtent, LPINT lpnFit, LPINT lpDx, LPSIZE lpSize))
HOOK_DEFINE(BOOL, GetTextExtentExPointI, (HDC hdc, LPWORD pgiIn, int cgi, int nMaxExtent, LPINT lpnFit, LPINT lpDx, LPSIZE lpSize))

HOOK_DEFINE(BOOL, GetTextExtentPointI, (HDC hdc, LPWORD pgiIn, int cgi, LPSIZE lpSize))
*/

// HOOK_DEFINE(BOOL, nCreateProcessA, (LPCSTR lpApp, LPSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCSTR lpDir, LPSTARTUPINFOA psi, LPPROCESS_INFORMATION ppi))
// HOOK_DEFINE(BOOL, nCreateProcessW, (LPCWSTR lpApp, LPWSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCWSTR lpDir, LPSTARTUPINFOW psi, LPPROCESS_INFORMATION ppi))
// HOOK_DEFINE(BOOL, CreateProcessAsUserA, (HANDLE hToken, LPCSTR lpApp, LPSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCSTR lpDir, LPSTARTUPINFOA psi, LPPROCESS_INFORMATION ppi))
// HOOK_DEFINE(BOOL, CreateProcessAsUserW, (HANDLE hToken, LPCWSTR lpApp, LPWSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCWSTR lpDir, LPSTARTUPINFOW psi, LPPROCESS_INFORMATION ppi))

HOOK_DEFINE(BOOL, CreateProcessInternalW, (\
			HANDLE hToken, \
			LPCTSTR lpApplicationName,  \
			LPTSTR lpCommandLine, \
			LPSECURITY_ATTRIBUTES lpProcessAttributes, \
			LPSECURITY_ATTRIBUTES lpThreadAttributes, \
			BOOL bInheritHandles, \
			DWORD dwCreationFlags, \
			LPVOID lpEnvironment, \
			LPCTSTR lpCurrentDirectory, \
			LPSTARTUPINFO lpStartupInfo, \
			LPPROCESS_INFORMATION lpProcessInformation , \
			PHANDLE hNewToken \
			))
//HOOK_DEFINE(BOOL, DrawStateA, (HDC hdc, HBRUSH hbr, DRAWSTATEPROC lpOutputFunc, LPARAM lData, WPARAM wData, int x, int y, int cx, int cy, UINT uFlags))
//HOOK_DEFINE(BOOL, DrawStateW, (HDC hdc, HBRUSH hbr, DRAWSTATEPROC lpOutputFunc, LPARAM lData, WPARAM wData, int x, int y, int cx, int cy, UINT uFlags))

// HOOK_DEFINE(BOOL, GetTextExtentPointA, (HDC hdc, LPCSTR lpString, int cbString, LPSIZE lpSize))
// HOOK_DEFINE(BOOL, GetTextExtentPointW, (HDC hdc, LPCWSTR lpString, int cbString, LPSIZE lpSize))
// HOOK_DEFINE(BOOL, GetTextExtentPoint32A, (HDC hdc, LPCSTR lpString, int cbString, LPSIZE lpSize))
// HOOK_DEFINE(BOOL, GetTextExtentPoint32W, (HDC hdc, LPCWSTR lpString, int cbString, LPSIZE lpSize))
HOOK_DEFINE(int, GetObjectW,  (__in HANDLE h, __in int c, __out_bcount_opt(c) LPVOID pv))
HOOK_DEFINE(int, GetObjectA,  (__in HANDLE h, __in int c, __out_bcount_opt(c) LPVOID pv))
HOOK_DEFINE(BOOL, GetTextFaceAliasW, (HDC hdc, int nLen, LPWSTR lpAliasW))
HOOK_DEFINE(BOOL, DeleteObject, ( HGDIOBJ hObject))
HOOK_DEFINE(int, GetTextFaceW, ( __in HDC hdc, __in int c, __out_ecount_part_opt(c, return)  LPWSTR lpName))
HOOK_DEFINE(int, GetTextFaceA, ( __in HDC hdc, __in int c, __out_ecount_part_opt(c, return)  LPSTR lpName))
HOOK_DEFINE(DWORD, GetGlyphOutlineW, (    __in HDC hdc, \
			__in UINT uChar, \
			__in UINT fuFormat, \
			__out LPGLYPHMETRICS lpgm, \
			__in DWORD cjBuffer, \
			__out_bcount_opt(cjBuffer) LPVOID pvBuffer, \
			__in CONST MAT2 *lpmat2 \
			))
HOOK_DEFINE(DWORD, GetGlyphOutlineA, (__in HDC hdc, \
			__in UINT uChar, \
			__in UINT fuFormat, \
			__out LPGLYPHMETRICS lpgm, \
			__in DWORD cjBuffer, \
			__out_bcount_opt(cjBuffer) LPVOID pvBuffer, \
			__in CONST MAT2 *lpmat2))


// DrawTextA,W
// DrawTextExA,W
// TabbedTextOutA,W
// TextOutA,W
// ExtTextOutA
// は内部で ExtTextOutWを呼んでるから ExtTextOutW だけ実装すればOK。←XPの場合
// Windows 2000 でも動くようにその他のAPIもフックを掛けておく。
/*
HOOK_DEFINE(HFONT, CreateFontA, (int nHeight, int nWidth, int nEscapement, int nOrientation, int fnWeight, DWORD fdwItalic, DWORD fdwUnderline, DWORD fdwStrikeOut, DWORD fdwCharSet, DWORD fdwOutputPrecision, DWORD fdwClipPrecision, DWORD fdwQuality, DWORD fdwPitchAndFamily, LPCSTR  lpszFace))
HOOK_DEFINE(HFONT, CreateFontW, (int nHeight, int nWidth, int nEscapement, int nOrientation, int fnWeight, DWORD fdwItalic, DWORD fdwUnderline, DWORD fdwStrikeOut, DWORD fdwCharSet, DWORD fdwOutputPrecision, DWORD fdwClipPrecision, DWORD fdwQuality, DWORD fdwPitchAndFamily, LPCWSTR lpszFace))
HOOK_DEFINE(HFONT, CreateFontIndirectA, (CONST LOGFONTA *lplf))*/
HOOK_DEFINE(HFONT, CreateFontIndirectW, (CONST LOGFONTW *lplf))
HOOK_DEFINE(HFONT, CreateFontIndirectExW, (CONST ENUMLOGFONTEXDV *penumlfex))

// HOOK_DEFINE(BOOL, GetCharWidthW, (HDC hdc, UINT iFirstChar, UINT iLastChar, LPINT lpBuffer))
// HOOK_DEFINE(BOOL, GetCharWidth32W, (HDC hdc, UINT iFirstChar, UINT iLastChar, LPINT lpBuffer))


HOOK_DEFINE(BOOL, TextOutA, (HDC hdc, int nXStart, int nYStart, LPCSTR  lpString, int cbString))
HOOK_DEFINE(BOOL, TextOutW, (HDC hdc, int nXStart, int nYStart, LPCWSTR lpString, int cbString))

HOOK_DEFINE(BOOL, ExtTextOutA, (HDC hdc, int nXStart, int nYStart, UINT fuOptions, CONST RECT *lprc, LPCSTR  lpString, UINT cbString, CONST INT *lpDx))

HOOK_DEFINE(BOOL, ExtTextOutW, (HDC hdc, int nXStart, int nYStart, UINT fuOptions, CONST RECT *lprc, LPCWSTR lpString, UINT cbString, CONST INT *lpDx))
HOOK_DEFINE(BOOL, RemoveFontResourceExW, (__in LPCWSTR name, __in DWORD fl, __reserved PVOID pdv))
//HOOK_DEFINE(BOOL, RemoveFontResourceW, (__in LPCWSTR lpFileName))
HOOK_DEFINE(HGDIOBJ, GetStockObject, (__in int i))
HOOK_DEFINE(BOOL, BeginPath, (HDC hdc))
HOOK_DEFINE(BOOL, EndPath, (HDC hdc))
HOOK_DEFINE(BOOL, AbortPath, (HDC hdc));

/*

HOOK_MANUALLY(LONG, LdrLoadDll, (IN PWCHAR               PathToFile OPTIONAL,
			   IN ULONG                Flags OPTIONAL, 
			   IN UNICODE_STRING2*      ModuleFileName, 
			   OUT HANDLE*             ModuleHandle ))
*/

HOOK_MANUALLY(HRESULT, DrawGlyphRun, (
	IDWriteBitmapRenderTarget* This,
	FLOAT baselineOriginX,
	FLOAT baselineOriginY,
	DWRITE_MEASURING_MODE measuringMode,
	DWRITE_GLYPH_RUN const* glyphRun,
	IDWriteRenderingParams* renderingParams,
	COLORREF textColor,
	RECT* blackBoxRect))

/*
HOOK_MANUALLY(void, SetTextAntialiasMode, (
			   ID2D1RenderTarget* self,
			   D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode))

HOOK_MANUALLY(void, SetTextRenderingParams, (
			   ID2D1RenderTarget* self,
			   __in_opt IDWriteRenderingParams *textRenderingParams ))*/

HOOK_MANUALLY(HRESULT, DWriteCreateFactory,  (
			  __in DWRITE_FACTORY_TYPE factoryType,
			  __in REFIID iid,
			  __out IUnknown **factory))

HOOK_MANUALLY(HRESULT, D2D1CreateDevice, (
			  IDXGIDevice* dxgiDevice,
			  CONST D2D1_CREATION_PROPERTIES* creationProperties,
			  ID2D1Device** d2dDevice))

HOOK_MANUALLY(HRESULT, D2D1CreateDeviceContext, (
			  IDXGISurface* dxgiSurface,
			  CONST D2D1_CREATION_PROPERTIES* creationProperties,
			  ID2D1DeviceContext** d2dDeviceContext))

HOOK_MANUALLY(HRESULT, D2D1CreateFactory, (
				D2D1_FACTORY_TYPE factoryType,
				REFIID riid,
				const D2D1_FACTORY_OPTIONS* pFactoryOptions,
				void** ppIFactory))

HOOK_MANUALLY(HRESULT, GetGdiInterop, (
			  IDWriteFactory* This,
			  IDWriteGdiInterop** gdiInterop
			  ))

HOOK_MANUALLY(HRESULT, CreateGlyphRunAnalysis, (
			  IDWriteFactory* This,
			  DWRITE_GLYPH_RUN const* glyphRun,
			  FLOAT pixelsPerDip,
			  DWRITE_MATRIX const* transform,
			  DWRITE_RENDERING_MODE renderingMode,
			  DWRITE_MEASURING_MODE measuringMode,
			  FLOAT baselineOriginX,
			  FLOAT baselineOriginY,
			  IDWriteGlyphRunAnalysis** glyphRunAnalysis
			  ))

HOOK_MANUALLY(HRESULT, CreateGlyphRunAnalysis2, (
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
			  ))

HOOK_MANUALLY(HRESULT, CreateGlyphRunAnalysis3, (
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
			  ))

HOOK_MANUALLY(HRESULT, GetAlphaBlendParams, (
			  IDWriteGlyphRunAnalysis* This,
			  IDWriteRenderingParams* renderingParams,
			  FLOAT* blendGamma,
			  FLOAT* blendEnhancedContrast,
			  FLOAT* blendClearTypeLevel
			  ))
			  
HOOK_MANUALLY(HRESULT, CreateDeviceContext, (
			  ID2D1Device* This,
			  D2D1_DEVICE_CONTEXT_OPTIONS options,
			  ID2D1DeviceContext** deviceContext
			  ))

HOOK_MANUALLY(HRESULT, CreateDeviceContext2, (
			  ID2D1Device1* This,
			  D2D1_DEVICE_CONTEXT_OPTIONS options,
			  ID2D1DeviceContext1** deviceContext
			  ))

HOOK_MANUALLY(HRESULT, CreateDeviceContext3, (
			  ID2D1Device2* This,
			  D2D1_DEVICE_CONTEXT_OPTIONS options,
			  ID2D1DeviceContext2** deviceContext
			  ))

HOOK_MANUALLY(HRESULT, CreateTextFormat, (
			   IDWriteFactory* self,
			   __in_z WCHAR const* fontFamilyName,
			   __maybenull IDWriteFontCollection* fontCollection,
			   DWRITE_FONT_WEIGHT fontWeight,
			   DWRITE_FONT_STYLE fontStyle,
			   DWRITE_FONT_STRETCH fontStretch,
			   FLOAT fontSize,
			   __in_z WCHAR const* localeName,
			   __out IDWriteTextFormat** textFormat))

HOOK_MANUALLY(HRESULT, CreateFontFace, (
			  IDWriteFont* self,
			  __out IDWriteFontFace** fontFace
			  ))

HOOK_MANUALLY(HRESULT, CreateBitmapRenderTarget, (
			  IDWriteGdiInterop* This,
			  HDC hdc,
			  UINT32 width,
			  UINT32 height,
			  IDWriteBitmapRenderTarget** renderTarget
			  ))

HOOK_MANUALLY(HRESULT, CreateCompatibleRenderTarget, (
			  ID2D1RenderTarget* This,
			  CONST D2D1_SIZE_F* desiredSize,
			  CONST D2D1_SIZE_U* desiredPixelSize,
			  CONST D2D1_PIXEL_FORMAT* desiredFormat,
			  D2D1_COMPATIBLE_RENDER_TARGET_OPTIONS options,
			  ID2D1BitmapRenderTarget** bitmapRenderTarget
			  ))

HOOK_MANUALLY(void, SetTextAntialiasMode, (
			  ID2D1RenderTarget* This,
			  D2D1_TEXT_ANTIALIAS_MODE textAntialiasMode));

HOOK_MANUALLY(void, SetTextRenderingParams, (
				  ID2D1RenderTarget* This,
				  _In_opt_ IDWriteRenderingParams* textRenderingParams));

			  /*
HOOK_MANUALLY(GpStatus, GdipDrawString, (
			  GpGraphics               *graphics,
			  GDIPCONST WCHAR          *string,
			  INT                       length,
			  GDIPCONST GpFont         *font,
			  GDIPCONST RectF          *layoutRect,
			  GDIPCONST GpStringFormat *stringFormat,
			  GDIPCONST GpBrush        *brush
			  ))*/

HOOK_MANUALLY(HRESULT, CreateWicBitmapRenderTarget, (
	ID2D1Factory* This,
	IWICBitmap* target,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1RenderTarget** renderTarget));

HOOK_MANUALLY(HRESULT, CreateHwndRenderTarget, (
	ID2D1Factory* This,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	const D2D1_HWND_RENDER_TARGET_PROPERTIES* hwndRenderTargetProperties,
	ID2D1HwndRenderTarget** hwndRenderTarget));

HOOK_MANUALLY(HRESULT, CreateDxgiSurfaceRenderTarget, (
	ID2D1Factory* This,
	IDXGISurface* dxgiSurface,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1RenderTarget** renderTarget));

HOOK_MANUALLY(HRESULT, CreateDCRenderTarget, (
	ID2D1Factory* This,
	const D2D1_RENDER_TARGET_PROPERTIES* renderTargetProperties,
	ID2D1DCRenderTarget** dcRenderTarget));

HOOK_MANUALLY(HRESULT, CreateDevice1, (
	ID2D1Factory1* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device** d2dDevice));

HOOK_MANUALLY(HRESULT, CreateDevice2, (
	ID2D1Factory2* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device1** d2dDevice1
	));

HOOK_MANUALLY(HRESULT, CreateDevice3, (
	ID2D1Factory3* This,
	IDXGIDevice* dxgiDevice,
	ID2D1Device2** d2dDevice2
	));
//EOF
