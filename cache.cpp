#include "override.h"

//CreateDIBè®¡æ•°ï¼Œå°†åœ¨ç»˜åˆ¶ä¸‹åˆ—æ¬¡æ•°åŽæ›´æ–°DIBåŒº
#define BITMAP_REDUCE_COUNTER	256//Ä¬ÈÏ1024


HDC CBitmapCache::CreateDC(HDC dc)
{
	if(!m_hdc) {
		m_hdc = CreateCompatibleDC(dc);
		m_exthdc = dc;
	}
	return m_hdc;
}

HBITMAP CBitmapCache::CreateDIB(int width, int height, BYTE** lplpPixels)
{
	SIZE& dibSize = m_dibSize;
	width  = (width + 3) & ~3;

	if (dibSize.cx >= width && dibSize.cy >= height) {
		if (++m_counter < BITMAP_REDUCE_COUNTER) {
			*lplpPixels = m_lpPixels;
			return m_hbmp;
		}
		//ƒJƒEƒ“ƒ^’´‰ß
		//‚½‚¾‚µƒTƒCƒY‚ª‘S‚­“¯‚¶‚È‚çÄ¶¬‚µ‚È‚¢
		if (dibSize.cx == width && dibSize.cy == height) {
			m_counter   = 0;
			*lplpPixels = m_lpPixels;
			return m_hbmp;
		}
	} else {
		if (dibSize.cx > width) {
			width  = dibSize.cx;
		}
		if (dibSize.cy > height) {
			height = dibSize.cy;
		}
	}

	BITMAPINFOHEADER bmiHeader = { sizeof(BITMAPINFOHEADER), width, -height, 1, 32, BI_RGB };
	HBITMAP hbmpNew = CreateDIBSection(CreateDC(m_exthdc), (BITMAPINFO*)&bmiHeader, DIB_RGB_COLORS, (LPVOID*)lplpPixels, NULL, 0);
	if (!hbmpNew) {
		return NULL;
	}
	TRACE(_T("width=%d, height=%d\n"), width, height);

	//ƒƒ‚ƒŠ•s‘«“™‚ÅhbmpNew==NULL‚Ìê‡‚ð‘z’è‚µA
	//¬Œ÷‚µ‚½‚Æ‚«‚Ì‚ÝƒLƒƒƒbƒVƒ…‚ðXV
	if (m_hbmp) {
		DeleteBitmap(m_hbmp);
	}

	m_hbmp		= hbmpNew;
	dibSize.cx	= width;
	dibSize.cy	= height;
	//CreateDIBSection‚Í‘½•ªƒy[ƒW‹«ŠE‚©ƒZƒOƒƒ“ƒg‹«ŠE
	m_lpPixels	= *lplpPixels;
	m_counter	= 0;
	return m_hbmp;
}

void CBitmapCache::FillSolidRect(COLORREF rgb, const RECT* lprc)
{

	if (!m_brush || rgb!=m_bkColor)
	{
		if (m_brush)
			DeleteObject(m_brush);
		m_brush = CreateSolidBrush(rgb);
		m_bkColor = rgb;
	}
	FillRect(m_hdc, lprc, m_brush);


	//DrawHorizontalLine(lprc->left, lprc->top, lprc->right, rgb, lprc->bottom - lprc->top);
/*	LPBYTE lpPixels = m_lpPixels;
	const DWORD dwBmpBytes		= m_dibSize.cx * m_dibSize.cy;
	rgb = RGB2DIB(rgb);

	//TODO: MMX or SSE‰»
	__asm {
		mov edi, dword ptr [lpPixels]
		mov ecx, dword ptr [dwBmpBytes]
		mov eax, dword ptr [rgb]
		cld
		rep stosd
	}*/
//	DWORD* p = (DWORD*)m_lpPixels;
//	DWORD* const pend = p + dwBmpBytes;
//	while (p < pend) {
//		*p++ = rgb;
//	}
}

//…•½ü‚ðˆø‚­
//(X1,Y1)           (X2,Y1)
//   +-----------------+   ^
//   |       rgb       |   | width
//   +-----------------+   v
void CBitmapCache::DrawHorizontalLine(int X1, int Y1, int X2, COLORREF rgb, int width)
{
	if (!m_dibSize.cx || !m_dibSize.cy) {
		return;
	}

	if (X1 > X2) {
		const int xx = X1;
		X1 = X2;
		X2 = xx;
	}

	//ƒNƒŠƒbƒsƒ“ƒO
	const int xSize = m_dibSize.cx;
	const int ySize = m_dibSize.cy;
	X1 = Bound(X1, 0, xSize);
	X2 = Bound(X2, 0, xSize);
	Y1 = Bound(Y1, 0, ySize);
	width = Max(width, 1);
	const int Y2 = Bound(Y1 + width, 0, ySize);

	rgb = RGB2DIB(rgb);

	DWORD* lpPixels = (DWORD*)m_lpPixels + (Y1 * xSize + X1);
	const int Xd = X2 - X1;
	const int Yd = Y2 - Y1;
/*	for (int yy=Y1; yy<Y2; yy++) {
		for (int xx=X1; xx<X2; xx++) {
			_SetPixelV(xx, yy, rgb);
		}
	}

	for (int yy=Y1; yy<Y2; yy++, lpPixels += xSize) {
		__asm {
			mov edi, dword ptr [lpPixels]
			mov ecx, dword ptr [Xd]
			mov eax, dword ptr [rgb]
			cld
			rep stosd
		}
	}*/

/*#ifdef _M_IX86
	//–³ˆÓ–¡‚ÉƒAƒZƒ“ƒuƒŠ‰»
	__asm {
		mov ebx, dword ptr [Yd]
		mov edx, dword ptr [lpPixels]
		mov esi, dword ptr [xSize]
		cld
L1:
		mov edi, edx
		mov ecx, dword ptr [Xd]
		mov eax, dword ptr [rgb]
		rep stosd
		lea edx, dword ptr [edx+esi*4]
		dec ebx
		jnz L1
	}
#else*/	//å¯¹äºŽ64ä½ç³»ç»Ÿï¼Œä½¿ç”¨Cè¯­è¨€
	for (int yy=Y1; yy<Y2; yy++) {
		for (int xx=X1; xx<X2; xx++) {
			*( (DWORD*)m_lpPixels + (yy * xSize + xx) ) = rgb;
		}
	}
//#endif

}
