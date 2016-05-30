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
HOOK_DEFINE(BOOL, AbortPath, (HDC hdc))
//HOOK_DEFINE(BOOL, PlayEnhMetaFileRecord())
//HOOK_DEFINE(int, GdipCreateFontFamilyFromName, (const WCHAR *name, void *fontCollection, void **FontFamily))

//HOOK_DEFINE(HRESULT, ScriptItemize, (const WCHAR* pwcInChars,int cInChars,int cMaxItems,const SCRIPT_CONTROL* psControl,const SCRIPT_STATE* psState,SCRIPT_ITEM* pItems,int* pcItems))

/*
HOOK_DEFINE(HRESULT, ScriptShape, (\
			HDC hdc, \
			SCRIPT_CACHE* psc, \
			const WCHAR* pwcChars, \
			int cChars, \
			int cMaxGlyphs, \
			SCRIPT_ANALYSIS* psa, \
			WORD* pwOutGlyphs, \
			WORD* pwLogClust, \
			SCRIPT_VISATTR* psva, \
			int* pcGlyphs \
			))*/


/*
HOOK_DEFINE(HRESULT, ScriptTextOut, (\
			const HDC hdc, \
			SCRIPT_CACHE* psc, \
			int x, \
			int y, \
			UINT fuOptions, \
			const RECT* lprc, \
			const SCRIPT_ANALYSIS* psa, \
			const WCHAR* pwcReserved, \
			int iReserved, \
			const WORD* pwGlyphs, \
			int cGlyphs, \
			const int* piAdvance, \
			const int* piJustify, \
			const GOFFSET* pGoffset \
			))

HOOK_DEFINE(HRESULT, ScriptStringOut, (\
			__in  SCRIPT_STRING_ANALYSIS ssa,\
			__in  int iX, \
			__in  int iY, \
			__in  UINT uOptions, \
			__in  const RECT *prc, \
			__in  int iMinSel, \
			__in  int iMaxSel, \
			__in  BOOL fDisabled \
			))*/

//HOOK_DEFINE(int, GetTextFace, (HDC hdc, int c,  LPWSTR lpName))
//HOOK_DEFINE(HRESULT, ScriptIsComplex, (const WCHAR *pwcInChars,int cInChars,DWORD dwFlags))
/*

HOOK_MANUALLY(LONG, LdrLoadDll, (IN PWCHAR               PathToFile OPTIONAL,
			   IN ULONG                Flags OPTIONAL, 
			   IN UNICODE_STRING2*      ModuleFileName, 
			   OUT HANDLE*             ModuleHandle ))
*/

HOOK_MANUALLY(void, DrawGlyphRun, (
			   ID2D1RenderTarget* self,
			   D2D1_POINT_2F baselineOrigin,
			   const DWRITE_GLYPH_RUN *glyphRun,
			   ID2D1Brush *foregroundBrush,
			   DWRITE_MEASURING_MODE measuringMode))

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


HOOK_MANUALLY(GpStatus, GdipDrawString, (
			  GpGraphics               *graphics,
			  GDIPCONST WCHAR          *string,
			  INT                       length,
			  GDIPCONST GpFont         *font,
			  GDIPCONST RectF          *layoutRect,
			  GDIPCONST GpStringFormat *stringFormat,
			  GDIPCONST GpBrush        *brush
			  ))
//EOF
