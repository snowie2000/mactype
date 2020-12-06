#ifndef _GDIPLUSFLAT_H
#define _GDIPLUSFLAT_H

#include <Windows.h>
#include "GdiPlusTypes2.h"
#include <GdiPlusEnums.h>
#include <GdiplusGpStubs.h>
#include <GdiPlusPixelFormats.h>

#define GDIPCONST const
#define WINGDIPAPI __stdcall
typedef Status GpStatus;

typedef GpStatus (WINGDIPAPI*
GdipDrawString)(
			   GpGraphics               *graphics,
			   GDIPCONST WCHAR          *string,
			   INT                       length,
			   GDIPCONST GpFont         *font,
			   GDIPCONST RectF          *layoutRect,
			   GDIPCONST GpStringFormat *stringFormat,
			   GDIPCONST GpBrush        *brush
			   );//绘制文字的函数

typedef GpStatus (WINGDIPAPI*
GdipGetLogFontW)(GpFont * font, GpGraphics *graphics, LOGFONTW * logfontW);//��GPFont��ȡLogfont

typedef GpStatus (WINGDIPAPI*
GdipGetDC)(GpGraphics* graphics, HDC * hdc);	//��GPGraphic��ȡHDC

typedef GpStatus (WINGDIPAPI*
GdipReleaseDC)(GpGraphics* graphics, HDC hdc);

typedef GpStatus (WINGDIPAPI*
GdipGetStringFormatAlign)(GDIPCONST GpStringFormat *format, 
						 StringAlignment *align);

typedef GpStatus (WINGDIPAPI*
GdipGetStringFormatTrimming)(
							GDIPCONST GpStringFormat *format,
							StringTrimming       *trimming
							);	//获取字符串的缩略方式。当字符串长度超过矩形长度时，将使用设置的方式删除中间的部分文字。
								//ת����GDI��Ӧʹ��DrawText��ģ�⡣
typedef GpStatus (WINGDIPAPI*
GdipGetStringFormatHotkeyPrefix)(GDIPCONST GpStringFormat *format, 
								INT *hotkeyPrefix);	//获取&符号的显示方式，隐藏，显示下划线或者不处理&。由DrawText模拟。

typedef GpStatus (WINGDIPAPI*
GdipGetBrushType)(GpBrush *brush, GpBrushType *type);

typedef GpStatus (WINGDIPAPI*
GdipGetSolidFillColor)(GpSolidFill *brush, ARGB *color);	//��ȡ��ɫBrush����ɫ

extern GdipDrawString pfnGdipDrawString ;
extern GdipGetBrushType pfnGdipGetBrushType ;
extern GdipGetDC pfnGdipGetDC ;
extern GdipGetLogFontW pfnGdipGetLogFontW ;
extern GdipGetSolidFillColor pfnGdipGetSolidFillColor ;
extern GdipGetStringFormatAlign pfnGdipGetStringFormatAlign ;
extern GdipGetStringFormatHotkeyPrefix pfnGdipGetStringFormatHotkeyPrefix ;
extern GdipGetStringFormatTrimming pfnGdipGetStringFormatTrimming ;
extern GdipReleaseDC pfnGdipReleaseDC ;

bool InitGdiplusFuncs();

#endif
