#pragma once

template <int BUFSIZE, bool ignoreCase>
class StringHashT
{
private:
	DWORD	m_dwHash;
	TCHAR	m_szBuffer[BUFSIZE];

	void UpdateHash()
	{
		DWORD dw = 0;
		LPWSTR p, end = m_szBuffer + BUFSIZE;
		for (p = m_szBuffer; *p && p < end; p++) {
			dw <<= 3;
			if (ignoreCase) {
				dw ^= _totlower(*p);
			} else {
				dw ^= *p;
			}
		}
		m_dwHash = dw;
	}

public:
	StringHashT()
		: m_dwHash(0)
	{
		ZeroMemory(m_szBuffer, sizeof(m_szBuffer));
	}
	StringHashT(LPCTSTR psz)
	{
		StringHashT();
		_tcsncpy(m_szBuffer, psz, BUFSIZE - 1);
		UpdateHash();
	}

	bool operator ==(const StringHashT<BUFSIZE, ignoreCase>& x) const
	{
		if (ignoreCase) {
			return !(m_dwHash != x.m_dwHash || _tcsicmp(m_szBuffer, x.m_szBuffer));
		} else {
			return !(m_dwHash != x.m_dwHash || _tcscmp(m_szBuffer, x.m_szBuffer));
		}
	}

	DWORD Hash() const
	{
		return m_dwHash;
	}
	LPCTSTR c_str() const
	{
		return m_szBuffer;
	}
};

typedef StringHashT<LF_FACESIZE,true>	StringHashFont;
typedef StringHashT<MAX_PATH,true>		StringHashModule;


//COLORREF(RR GG BB 00) を DIB32(BB GG RR 00) に変換
#define RGB2DIB(rgb)	RGB(GetBValue(rgb), GetGValue(rgb), GetRValue(rgb))
#define DIB2RGB(dib)	RGB2DIB(dib)

// ExtTextOutWのビットマップキャッシュ
class CBitmapCache
{
private:
	HBRUSH	m_brush;
	HDC		m_hdc;
	HDC		m_exthdc;
	HBITMAP	m_hbmp;
	BYTE*	m_lpPixels;
	SIZE	m_dibSize;
	int		m_counter;
	DWORD*	m_CurrentPixel;
	COLORREF m_bkColor;

	NOCOPY(CBitmapCache);

public:
	CBitmapCache()
		: m_hdc(NULL)
		, m_hbmp(NULL)
		, m_lpPixels(NULL)
		, m_counter(0)
		, m_CurrentPixel(NULL)
		, m_exthdc(NULL)
		, m_brush(NULL)
		, m_bkColor(NULL)
	{
		m_dibSize.cx = m_dibSize.cy = 0;
	}

	~CBitmapCache()
	{
		if (m_hdc) {
			DeleteDC(m_hdc);
		}
		if (m_hbmp)	{
			DeleteBitmap(m_hbmp);
		}
		if (m_brush) 
			DeleteObject(m_brush);
		m_hdc = NULL;
		m_hbmp = NULL;
		m_brush = NULL;
		m_dibSize.cx = 0;
		m_dibSize.cy = 0;
	}

	const SIZE& Size() const
	{
		return m_dibSize;
	}
	BYTE* GetPixels()
	{
		Assert(m_lpPixels != NULL);
		return m_lpPixels;
	}

	DWORD GetPixel(int X, int Y) {
		if ((unsigned)X >= (unsigned)m_dibSize.cx || (unsigned)Y >= (unsigned)m_dibSize.cy) {
			return CLR_INVALID;
		}
		DWORD* lpPixels = (DWORD*)m_lpPixels;
		m_CurrentPixel = &lpPixels[Y * m_dibSize.cx + X];
		DWORD dib = *m_CurrentPixel;
		return DIB2RGB(dib);
	}

	void SetPixelV(int X, int Y, COLORREF rgb) {
		// if ((unsigned)X >= (unsigned)m_dibSize.cx || (unsigned)Y >= (unsigned)m_dibSize.cy) {
		// 	return;
		// }
		DWORD* lpPixels = (DWORD*)m_lpPixels;
		m_CurrentPixel = &lpPixels[Y * m_dibSize.cx + X];
		SetCurrentPixel(rgb);
	}

	void SetCurrentPixel(COLORREF rgb) {
		*m_CurrentPixel = RGB2DIB(rgb);
	}

	//本体はcache.cpp
	HDC CreateDC(HDC dc);
	HBITMAP CreateDIB(int width, int height, BYTE** lplpPixels);
	void FillSolidRect(COLORREF rgb, const RECT* lprc);
	void DrawHorizontalLine(int X1, int Y1, int X2, COLORREF rgb, int width);

};
