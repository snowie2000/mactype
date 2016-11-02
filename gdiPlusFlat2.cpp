#include "gdiPlusFlat2.h"
#include <tchar.h>


GdipDrawString pfnGdipDrawString = NULL;
GdipGetBrushType pfnGdipGetBrushType = NULL;
GdipGetDC pfnGdipGetDC = NULL;
GdipGetLogFontW pfnGdipGetLogFontW = NULL;
GdipGetSolidFillColor pfnGdipGetSolidFillColor = NULL;
GdipGetStringFormatAlign pfnGdipGetStringFormatAlign = NULL;
GdipGetStringFormatHotkeyPrefix pfnGdipGetStringFormatHotkeyPrefix = NULL;
GdipGetStringFormatTrimming pfnGdipGetStringFormatTrimming = NULL;
GdipReleaseDC pfnGdipReleaseDC = NULL;

bool InitGdiplusFuncs(){
	static bool bInited = false;
	if (!bInited)
	{
		bInited = true;
		HMODULE	hGdiplusDll = GetModuleHandle(_T("Gdiplus.dll"));
		if (hGdiplusDll)
		{
			pfnGdipDrawString = (GdipDrawString)GetProcAddress(hGdiplusDll, "GdipDrawString");
			pfnGdipGetBrushType = (GdipGetBrushType)GetProcAddress(hGdiplusDll, "GdipGetBrushType");
			pfnGdipGetDC = (GdipGetDC)GetProcAddress(hGdiplusDll, "GdipGetDC");
			pfnGdipGetLogFontW = (GdipGetLogFontW)GetProcAddress(hGdiplusDll, "GdipGetLogFontW");
			pfnGdipGetSolidFillColor = (GdipGetSolidFillColor)GetProcAddress(hGdiplusDll, "GdipGetSolidFillColor");
			pfnGdipGetStringFormatAlign = (GdipGetStringFormatAlign)GetProcAddress(hGdiplusDll, "GdipGetStringFormatAlign");
			pfnGdipGetStringFormatHotkeyPrefix = (GdipGetStringFormatHotkeyPrefix)GetProcAddress(hGdiplusDll, "GdipGetStringFormatHotkeyPrefix");
			pfnGdipGetStringFormatTrimming = (GdipGetStringFormatTrimming)GetProcAddress(hGdiplusDll, "GdipGetStringFormatTrimming");
			pfnGdipReleaseDC = (GdipReleaseDC)GetProcAddress(hGdiplusDll, "GdipReleaseDC");
			return true;
		}
		else
			return false;
	}
	else
		return true;
}