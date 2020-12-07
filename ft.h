#ifndef FT_H
#define FT_H

#define FTO_MONO		0x0001
#define FTO_SIZE_ONLY	0x0002

#define CACHE_SIZE 20
#define MAX_CACHE_SIZE 1024
#define CNTRL_UNICODE_PLANE 2
#define CNTRL_COMPLEX_TEXT 1
#define CNTRL_ZERO_WIDTH 4
typedef char (*COLORCACHE)[3][256][256];

#include "fteng.h"

enum FT_DRAW_STATE{
	FT_DRAW_NORMAL = 0,
	FT_DRAW_EMBEDDED_BITMAP = 1,
	FT_DRAW_NOTFOUND = 2
};

BOOL FontLInit(void);
void FontLFree(void);

COLORREF GetPaletteColor(HDC hdc, UINT paletteindex);

#define ROUND(x) ((x)>0? int(x+0.5):int(x-0.5))	// a special round method used by windows
class CDCTransformer {
private:
	float fXZoomFactor, fYZoomFactor;
	XFORM m_xfm;
	bool bZoomMode;
	bool bMirrorX, bMirrorY;
public:
	CDCTransformer():bMirrorY(false), bMirrorX(false) {};
	void init(XFORM& xfm)
	{
		memcpy(&m_xfm, &xfm, sizeof(XFORM));
		if (xfm.eM11<0)
		{
			bMirrorX = true;
			m_xfm.eM11 = -m_xfm.eM11;
		}
		if (xfm.eM22<0)
		{
			bMirrorY = true;
			m_xfm.eM11 = -m_xfm.eM22;
		}
		fXZoomFactor = m_xfm.eM11;
		fYZoomFactor = m_xfm.eM22;
		m_xfm.eDx=0;
		m_xfm.eDy=0;
		bZoomMode = fXZoomFactor==fYZoomFactor;
	}
	void SetSourceOffset(int X, int Y)
	{
		float temp = float(X)*m_xfm.eM11+m_xfm.eDx;
		m_xfm.eDx = temp-(int)temp;
		if (ROUND(m_xfm.eDx)<0)	// change negive offset to positive
			m_xfm.eDx+=1;
		temp = float(Y)*m_xfm.eM22+m_xfm.eDy;
		m_xfm.eDy = temp-(int)temp;
		if (ROUND(m_xfm.eDy)<0)
			m_xfm.eDy+=1;
	}
	int XInit() {return ROUND(m_xfm.eDx);}
	int YInit() {return ROUND(m_xfm.eDy);}
	int GetXCeilingIntAB(int X)
	{
		return ROUND(ROUND((X-m_xfm.eDx)/m_xfm.eM11)*m_xfm.eM11+m_xfm.eDx);
	}
	int GetYCeilingIntAB(int Y)
	{
		return ROUND(ROUND((Y-m_xfm.eDy)/m_xfm.eM22)*m_xfm.eM22+m_xfm.eDy);
	}
	~CDCTransformer() {};
	void TransformRectAB(const RECT* lpInRect, PRECT lpOutRect)
	{
		lpOutRect->bottom = TransformYCoordinateAB(lpInRect->bottom);
		lpOutRect->left = TransformXCoordinateAB(lpInRect->left);
		lpOutRect->right = TransformXCoordinateAB(lpInRect->right);
		lpOutRect->top = TransformYCoordinateAB(lpInRect->top);
	}
	void TransformRectBA(const RECT* lpInRect, PRECT lpOutRect)
	{
		lpOutRect->bottom = ROUND(lpInRect->bottom/fYZoomFactor);
		lpOutRect->left = ROUND(lpInRect->left/fXZoomFactor);
		lpOutRect->right = ROUND(lpInRect->right/fXZoomFactor);
		lpOutRect->top = ROUND(lpInRect->top/fYZoomFactor);
	}
	int TransformXAB(int nX)
	{
		return ROUND(nX*fXZoomFactor);
	}
	int TransformXCoordinateAB(int nX)
	{
		return ROUND((float(nX))*fXZoomFactor/*+m_xfm.eDx*/);
	}
	int TransformXBA(int nX)
	{
		return ROUND(nX/fXZoomFactor);
	}
	int TransformXCoordinateBA(int nX)
	{
		return ROUND((float(nX)/*-m_xfm.eDx*/)/fXZoomFactor);
	}
	int TransformYAB(int nY)
	{
		return ROUND(nY*fYZoomFactor);
	}
	int TransformYCoordinateAB(int nY)
	{
		return ROUND((float(nY))*fYZoomFactor/*+m_xfm.eDy*/);
	}
	int TransformYBA(int nY)
	{
		return ROUND(nY/fYZoomFactor);
	}
	int TransformYCoordinateBA(int nY)
	{
		return ROUND((float(nY)/*-m_xfm.eDy*/)/fYZoomFactor);
	}
	void TransformlpDx(const int* lpDx, int* outlpDx, int szDx)
	{
		int LastPos = 0, LPCurPos = 0;
		double fDPLastPos = 0, fDPCurPos = 0;
		for (;szDx>0;--szDx)
		{
			LPCurPos += *lpDx++;						// the logical position the letter belongs to
			fDPCurPos = LPCurPos*fXZoomFactor;			// the device position the letter belongs to
			*outlpDx = ROUND(fDPCurPos-fDPLastPos);		// the device coord
			fDPLastPos += *outlpDx++;					// the coord after this letter is painted. In order to calculate the pos of the next letter.
		}
	}
	bool TransformMode() { return bZoomMode; }
	XFORM* GetTransform() { return &m_xfm; }
	bool MirrorX() { return bMirrorX; }
	bool MirrorY() { return bMirrorY; }
};

class ControlIder
{
private:
	char* unicode;
public:
	ControlIder()
	{
		unicode = new char[0xffff];
		memset(unicode, 0, sizeof(char)*0xffff);	// non-control char by default
		//memset(unicode, 2, sizeof(char)*32);
		for (int i=0;i<0x3000;i++)
			unicode[i]=!!iswcntrl(i);
		for (int i=0xa000;i<0xffff;i++)
			unicode[i]=!!iswcntrl(i);			// Chinese
		memset(&unicode[0xd800],CNTRL_UNICODE_PLANE,sizeof(char)*(0xdfff-0xd800+1));		//unicode plane
		memset(&unicode[0x0590],CNTRL_COMPLEX_TEXT,sizeof(char)*(0x05FF-0x0590+1));		//hebrew
		memset(&unicode[0x0600],CNTRL_COMPLEX_TEXT,sizeof(char)*(0x06FF-0x0600+1));		//arabic
		memset(&unicode[0xFB50],CNTRL_COMPLEX_TEXT,sizeof(char)*(0xFDFF-0xFB50+1));		//Arabic Presentation Forms-A
		memset(&unicode[0xFE70],CNTRL_COMPLEX_TEXT,sizeof(char)*(0xFEFF-0xFE70+1));		//Arabic Presentation Forms-B
		memset(&unicode[0x0E00],CNTRL_COMPLEX_TEXT,sizeof(char)*(0x0E7F-0x0E00+1));		//thai
//		unicode[0xa] = 0;
// 		unicode[0xd] = 0;
// 		unicode[0x9] = 0;
		// Set width for some special control chars. They have zero width, but GetCharABCWidth gives wrong results for them.
	}
	~ControlIder()
	{
		delete unicode;
	}
	void setcntrlAttribute(WCHAR wch, int cnType)
	{
		unicode[wch] = cnType;
	}
	char myiswcntrl(WCHAR str)
	{
		return unicode[str];
	}
	bool myiscomplexscript(LPCWSTR lpString, int cbString)
	{
		for (;cbString>0;++lpString,--cbString)
		{
			if (unicode[*lpString]==CNTRL_COMPLEX_TEXT)
				return true;
		}
		return false;
	}
};

struct FREETYPE_PARAMS
{
	UINT etoOptions;
	UINT ftOptions;
	int charExtra;
	COLORREF color;
	int alpha;
	BYTE alphatuner;
	LOGFONTW* lplf;
	OUTLINETEXTMETRIC* otm;
	wstring strFamilyName, strFullName;

	FREETYPE_PARAMS()
	{
		ZeroMemory(this, sizeof(*this));
	}

	//FreeTypeTextOut用 (サイズ計算＋文字描画)
	FREETYPE_PARAMS(UINT eto, HDC hdc, LOGFONTW* p, OUTLINETEXTMETRIC* lpotm = NULL)
		: etoOptions(eto)
		, ftOptions(0)
		, charExtra(GetTextCharacterExtra(hdc))
		, color(GetTextColor(hdc))
		, alpha(0)
		, lplf(p)
		, otm(lpotm)
		, alphatuner(1)
	{
		if ((color>>24)%2 || (color>>28)%2)
		{
			color = GetPaletteColor(hdc, color);
		}
		if (otm)
		{
			strFamilyName = (LPWSTR)((DWORD_PTR)otm + (DWORD_PTR)otm->otmpFamilyName);
			strFullName = wstring((LPWSTR)((DWORD_PTR)otm + (DWORD_PTR)otm->otmpFullName));
			std::wstring strStyleName = wstring((LPWSTR)((DWORD_PTR)otm + (DWORD_PTR)otm->otmpStyleName));

			strFullName = MakeUniqueFontName(strFullName, strFamilyName, strStyleName);
			if (strFamilyName.size() > 0 && strFamilyName.c_str()[0] == L'@')
				strFullName = L'@' + strFullName;
		}
	}

	bool IsMono() const
	{
		return (ftOptions & FTO_MONO);
	}
};

class CGGOKerning : public CMap<DWORD, int>
{
private:
	DWORD makekey(WORD first, WORD second) {
		return ((DWORD)first << 16) | second;
	}
public:
	void init(HDC hdc)
	{
		DWORD rc;
		rc = GetKerningPairs(hdc, 0, NULL);
		if (rc <= 0) return;
		DWORD kpcnt = rc;
		LPKERNINGPAIR kpp = (LPKERNINGPAIR)calloc(kpcnt, sizeof *kpp);
		if (!kpp) return;
		rc = GetKerningPairs(hdc, kpcnt, kpp);
		for (DWORD i = 0; i < rc; ++i) {
			Add(makekey(kpp[i].wFirst, kpp[i].wSecond), kpp[i].iKernAmount);
		}
		free(kpp);
	}
	int get(WORD first, WORD second) {
		DWORD key = makekey(first, second);
		int x = FindKey(key);
		return (x >= 0) ? GetValueAt(x) : 0;
	}
};


//fteng.cpp variables
// forward declaration
extern FT_Library     freetype_library;
extern FTC_Manager    cache_man;
extern FTC_CMapCache  cmap_cache;
extern FTC_ImageCache image_cache;

struct FreeTypeDrawInfo
{
	FT_FaceRec_ dummy_freetype_face;

	//FreeTypePrepareが設定する
	int sx,sy;
	FT_Face freetype_face;
	FT_Int cmap_index;
	FT_Bool useKerning;
	FT_Render_Mode render_mode;
	FTC_ImageTypeRec font_type;
	FTC_ScalerRec scaler;
	FreeTypeFontInfo* pfi;
	const CFontSettings* pfs;
	FreeTypeFontCache* pftCache;
	FTC_FaceID* face_id_list;
	HFONT* ggo_font_list;	// for faster GGO fontlinking
	FTC_FaceID face_id_simsun;
	FT_Face freetype_face_list[CFontLinkInfo::FONTMAX * 2 + 1];	// in order to solve italic issues.
	int face_id_list_num;
	int* Dx;
	int* Dy;

	//呼び出し前に自分で設定する
	HDC hdc;
	int xBase;
	int y;//coord height, calculated by ETO_PDY, 0 if not provided
	int x;//coord width, calculated by win32 API
	int px;	//x of paint, the real text width
	int yBase;
	int yTop;

	int height; //original width
	int width;

	const int* lpDx;
	CBitmapCache* pCache;
	FREETYPE_PARAMS* params;
	int* AAModes;


	FreeTypeDrawInfo(FREETYPE_PARAMS& fp, HDC dc, LOGFONTW* lf = NULL, CBitmapCache* ca = NULL, const int* dx = NULL, int cbString =0, int xs=0, int ys = 0)
		: freetype_face(&dummy_freetype_face), cmap_index(0), useKerning(0)
		, pfi(NULL), pfs(NULL), pftCache(NULL), face_id_list_num(0), ggo_font_list(NULL)
		, hdc(dc), x(0), y(0), yBase(0), yTop(0), face_id_simsun(NULL), px(0), xBase(0)
	{
		render_mode = FT_RENDER_MODE_NORMAL;
		ZeroMemory(&scaler, sizeof(scaler));
		ZeroMemory(&font_type, sizeof(font_type));
		ZeroMemory(&face_id_list, sizeof face_id_list);
		// init face list
		ZeroMemory(&freetype_face_list, sizeof freetype_face_list);
		lpDx   = dx;
		pCache = ca;
		params = &fp;
		height = 0;
		width = 0;
		sx = xs;
		sy = ys;
		if(lf) params->lplf = lf;
		memset(&dummy_freetype_face, 0, sizeof dummy_freetype_face);
		Dx = new int[cbString];
		Dy = new int[cbString];
		AAModes = new int[cbString];
		scaler.height = 12;
		scaler.width = 12;
		scaler.pixel = 1;
	}
	~FreeTypeDrawInfo()
	{
		delete Dx;
		delete Dy;
		delete[] AAModes;
	}

	const LOGFONTW& LogFont() const { return *params->lplf; }
	COLORREF Color() const { return params->color; }
	UINT GetETO() const { return params->etoOptions; }
	bool IsGlyphIndex() const { return !!(params->etoOptions & ETO_GLYPH_INDEX); }
	bool IsMono() const { return !!(params->ftOptions & FTO_MONO); }
	bool IsSizeOnly() const { return !!(params->ftOptions & FTO_SIZE_ONLY); }
	CGGOKerning ggokerning;

	FT_Face GetFace(int index)	// get face list
	{
		if (!freetype_face_list[index])
		{
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_MANAGER);
			FT_Size font_size;
			//FTC_ScalerRec fscaler={face_id_list[index], 1,1,1,0,0};
			scaler.face_id = face_id_list[index];
			if (FTC_Manager_LookupSize(cache_man, &scaler, &font_size))
			//if (FTC_Manager_LookupFace(cache_man, face_id_list[index], &freetype_face_list[index]))
				freetype_face_list[index] = NULL;
			else {
				if (scaler.height == 0)
					return font_size->face;	// return without save, because the scaler is not prepared yet.
				freetype_face_list[index] = font_size->face;
			}

		}
		return freetype_face_list[index];
	}

#ifdef _DEBUG
	void Validate()
	{
		Assert(params->lplf);
	};
#endif
};

BOOL FreeTypeTextOut(
	const HDC hdc,
	CBitmapCache& cache,
	LPCWSTR lpString,
	int cbString,
	FreeTypeDrawInfo& FTInfo,
	FT_Referenced_Glyph * Glyphs,
	FT_DRAW_STATE* drState
	);


BOOL FreeTypeGetGlyph(	// Get all the glyphs and widths needed.
					  FreeTypeDrawInfo& FTInfo,
					  LPCWSTR lpString,  
					  int cbString,     
					  int& width,
					  FT_Referenced_Glyph* Glyphs,
					  FT_DRAW_STATE* drState
					  );
void RefreshAlphaTable();
void UpdateLcdFilter();

#endif
