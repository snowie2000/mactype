#pragma once

#include <ft2build.h>
#include <freetype/freetype.h>	/* FT_FREETYPE_H */
#include <freetype/ftcache.h>	/* FT_CACHE_H */
#include <freetype/tttables.h>
#include <freetype/tttags.h>	// FT_TRUETYPE_TAGS_H
#include <freetype/ftoutln.h>
#include <vector>
#include "ftref.h"
#include <math.h>
#include "undocAPI.h"

class FreeTypeFontEngine;
extern FreeTypeFontEngine* g_pFTEngine;
extern BOOL g_ccbCache;
extern BOOL g_ccbIndividual;
extern FTC_Manager    cache_man;

typedef set<CBitmapCache*> CTLSDCArray;
extern CTLSDCArray TLSDCArray;

LOGFONTW* GetFontNameFromFile(LPCTSTR Filename);
bool GetFontLocalName(TCHAR* pszFontName, __out TCHAR* pszNameOut);	//获得字体的本地化名称

struct CFontSetCache
{
	const CFontSettings** fontsetlist;
	int  fontsetsize;
	CFontSetCache()
		:fontsetsize(0)
	{
		fontsetsize=64;
		fontsetlist = (const CFontSettings**)malloc(fontsetsize * sizeof(void*));
	}
	~CFontSetCache()
	{
		free(fontsetlist);
	}
	void Set(FTC_FaceID faceid, const CFontSettings& fset)
	{
		while ((INT_PTR)faceid>=fontsetsize)
		{
			fontsetsize+=64;
			fontsetlist = (const CFontSettings**)realloc(fontsetlist, fontsetsize);
		}
		fontsetlist[(INT_PTR)faceid]=&fset;
	}
	const CFontSettings*& Get(FTC_FaceID faceid) const
	{
		return fontsetlist[(INT_PTR)faceid];
	}
};

struct myfont
{
	wstring name;
	int hash;
	bool operator < (const myfont mf) const {
		return name==mf.name? hash<mf.hash: name<mf.name;
	}
public:
	myfont(LPCWSTR lpFaceName, int nweight, int nitalic):
	name(lpFaceName),hash((nitalic<<31) | nweight)
	{}
};

enum FT_EngineConstants {
	FT_MAX_CHARS	= 65536,
};

/*
  FreeType偵暥帤暆丄懢帤丄幬懱傪僉儍僢僔儏偡傞婡峔偑柍偄偺偱偦傟傜傪曗偆

  1. 傑偢DllMain(DLL_PROCESS_ATTACH)偱FreeTypeFontEngine偺僀儞僗僞儞僗偑惗惉偝傟傞丅
     (弴斣偼CGdiPPSettings仺FontLInit(FreeType)仺FreeTypeFontEngine仺僼僢僋)
     ForceChangeFont傕偙偙偱張棟偡傞丅

  2. CreateFont偱FreeTypeFontEngine::AddFont偑屇傃弌偝傟丄FreeTypeFontInfo偲
     僼僅儞僩柤傪寢傃偮偗傞丅
     偮偄偱偵FreeTypeFontInfo偼Individual偺愝掕傪僐僺乕偟偰帩偮丅

  3. ExtTextOut傗GetTextExtent側偳偐傜FreeTypePrepare娭悢偑屇傃弌偝傟傞偲
     偝傜偵撪晹偱FreeTypeFontInfo::GetCache偑屇傃弌偝傟丄僼僅儞僩僒僀僘側偳偐傜
     FreeTypeFontCache傪摼傞丅柍偗傟偽惗惉偡傞丅
     FreeTypeFontCache偼撪晹偵FreeTypeCharData偺僥乕僽儖(UCS2側偺偱2^16屄)傪
     帩偭偰偄偰丄FreeTypeCharData偵偼暥帤枅偵僉儍僢僔儏僨乕僞傪曐娗偡傞丅

  4. FreeTypeFontCache偐傜丄暥帤傑偨偼僌儕僼斣崋傪尦偵FreeTypeCharData傪摼傞丅
     僉儍僢僔儏偑偁傟偽(儊儌儕拞偵巆偭偰偄傟偽)丄MRU僇僂儞僞傪僙僢僩偡傞丅
     柍偄応崌偼堦扷僗儖乕偟丄屻偱AddCharData偱僉儍僢僔儏傪捛壛偡傞丅

  5. 捛壛偟傑偔傞偲儊儌儕傪嬺傜偆偺偱丄捛壛偑堦掕悢(FREETYPE_REQCOUNTMAX)傪挻偊傞偲
     GC儌僪僉偱嵟嬤嶲徠偝傟偨僉儍僢僔儏僨乕僞傪FREETYPE_GC_COUNTER屄偩偗巆偟丄
     偦傟埲奜偺僨乕僞(FreeTypeCharData)偼奐曻偝傟傞丅
     偙偺2偮偺掕悢偼ini偱愝掕曄峏偱偒偨曽偑偄偄傛偆側婥傕偡傞丅

  6. 嵟屻偵丄DllMain(DLL_PROCESS_DETACH)偱FreeTypeFontEngine偺僀儞僗僞儞僗偑攋婞偝傟丄
     慡偰偺僉儍僢僔儏儊儌儕偑奐曻偝傟傞丅

 */

class FreeTypeGCCounter
{
private:
	int m_addcount;		//捛壛梡
	int m_mrucount;		//MRU梡

public:
	FreeTypeGCCounter()
		: m_addcount(0), m_mrucount(0)
	{
	}
	int AddIncrement() { return ++m_addcount; }
	int DecIncrement() { return --m_addcount; }
	int MruIncrement() { return ++m_mrucount; }

	void ResetGCCounter()
	{
		m_mrucount = 0;
		m_addcount = 0;
	}
};

class FreeTypeMruCounter
{
private:
	int m_mrucounter;	//GC梡

public:
	FreeTypeMruCounter(int n)
		: m_mrucounter(n)
	{
	}

	//GC梡MRU僇僂儞僞
	int GetMruCounter() const { return m_mrucounter; }
	void ResetMruCounter() { m_mrucounter = 0; }
	void SetMruCounter(FreeTypeGCCounter* p) { m_mrucounter = p->MruIncrement(); }
};

//暥帤暆丄(glyph index)丄FT_BitmapGlyph(懢帤丄幬懱偺傒)傪僉儍僢僔儏偡傞
class FreeTypeCharData : public FreeTypeMruCounter
{
private:
	typedef CValArray<FreeTypeCharData**>	CharDataArray;
	CharDataArray		m_arrSelfChar;	//帺暘帺恎偺曐懚尦(Char)
	FreeTypeCharData**	m_ppSelfGlyph;	//(Glyph)
	UINT				m_glyphindex;	//僌儕僼斣崋
	int					m_width;		//暥帤暆
	int					m_gdiWidth;		//使用GetCharWidth获得的GDI宽度
	FT_Referenced_BitmapGlyph		m_glyph;		//僇儔乕梡
	FT_Referenced_BitmapGlyph		m_glyphMono;	//儌僲僋儘梡
	int					m_bmpSize;		//價僢僩儅僢僾僒僀僘
	int					m_bmpMonoSize;	// 乂
	int					m_AAMode;
//	LONG				m_refcounter;	//嶲徠僇僂儞僞

#ifdef _DEBUG
	WCHAR				m_wch;			//UCS2暥帤
#endif
	NOCOPY(FreeTypeCharData);

	//FT_Bitmap::buffer偺僒僀僘傪曉偡
	static inline int FT_Bitmap_CalcSize(FT_BitmapGlyph gl)
	{
		return gl->bitmap.pitch * gl->bitmap.rows;
	}

public:
	FreeTypeCharData(FreeTypeCharData** ppCh, FreeTypeCharData** ppGl, WCHAR wch, UINT glyphindex, int width, int mru, int gdiWidth, int AAMode);
	~FreeTypeCharData();

#ifdef _DEBUG
	WCHAR GetChar() const { return m_wch; }
#else
	WCHAR GetChar() const { return L'?'; }
#endif
	UINT GetGlyphIndex() const { return m_glyphindex; }
	int GetWidth() const { return m_width; }
	void SetWidth(int width) { m_width = width; }
	void SetGDIWidth(int width) { m_gdiWidth = width; }
	int GetGDIWidth() const {return m_gdiWidth; }
	int GetAAMode() const {return m_AAMode; }
	void AddChar(FreeTypeCharData** ppCh)
	{
		if (ppCh)
			m_arrSelfChar.Add(ppCh);
	}
	FT_Referenced_BitmapGlyph GetGlyph(FT_Render_Mode render_mode) const
	{
		return (render_mode == FT_RENDER_MODE_MONO) ? m_glyphMono : m_glyph;
	}
	void SetGlyph(FT_Render_Mode render_mode, FT_Referenced_BitmapGlyph glyph);

	void Erase()
	{
		//delete this;
	}
};
static INT_PTR NULL_INT = NULL;
class FreeTypeFontCache : public FreeTypeMruCounter, public FreeTypeGCCounter
{
	typedef map<int, FreeTypeCharData*> GlyphCache;

private:
	int  m_px;
	int  m_weight;
	bool m_italic;
	bool m_active;
	TEXTMETRIC m_tm;

	//4亊65536亊2亖512KB偖傜偄偨偐偑抦傟偰傞偺偱屌掕攝楍偱栤戣柍偟
#ifdef _USE_ARRAY
	FreeTypeCharData*	m_chars[FT_MAX_CHARS];
	FreeTypeCharData*	m_glyphs[FT_MAX_CHARS];
#else
	GlyphCache m_GlyphCache;
#endif
	NOCOPY(FreeTypeFontCache);
	void Compact();

	FreeTypeCharData** _GetChar(WCHAR wch)
	{
#ifdef _USE_ARRAY
		return m_chars + wch;
#else
		GlyphCache::iterator it=m_GlyphCache.find(wch);
		return it==m_GlyphCache.end()? reinterpret_cast<FreeTypeCharData**>(&NULL_INT): &(it->second);
#endif
	}
	FreeTypeCharData** _GetGlyph(UINT glyph)
	{
#ifdef _USE_ARRAY
		return m_glyphs + glyph;
#else
		GlyphCache::iterator it=m_GlyphCache.find(-(int)glyph);
		return it == m_GlyphCache.end() ? reinterpret_cast<FreeTypeCharData**>(&NULL_INT) : &(it->second);
#endif
	}

public:
	FreeTypeFontCache(/*int px, int weight, bool italic, */int mru);
	~FreeTypeFontCache();

	const TEXTMETRIC& GetTextMetric(HDC hdc)
	{
		if (m_tm.tmHeight == 0) {
			::GetTextMetrics(hdc, &m_tm);
		}
		return m_tm;
	}

	bool Equals(int px, int weight, bool italic) const
	{
		return (m_px == px && m_weight == weight && m_italic == italic);
	}
	FreeTypeCharData* FindChar(WCHAR wch)
	{
		/*if (!g_ccbCache) return NULL;*/
		FreeTypeCharData* p = *_GetChar(wch);
		if(p) {
			p->SetMruCounter(this);
		}
		return p;
	}

	FreeTypeCharData* FindGlyphIndex(UINT glyph)
	{
		/*if (!g_ccbCache) return NULL;*/
		FreeTypeCharData* p = (glyph & 0xffff0000) ? NULL : *_GetGlyph(glyph);
		if(p) {
			p->SetMruCounter(this);
		}
		return p;
	}

	bool Activate()
	{
		if (!m_active) {
			m_active = true;
			return true;
		}
		return false;
	}

	void Erase();
	void Deactive() { m_active = false; };
	void AddCharData(WCHAR wch, UINT glyphindex, int width, int gdiWidth, FT_Referenced_BitmapGlyph glyph, FT_Render_Mode render_mode, int AAMode);
	void AddGlyphData(UINT glyphindex, int width, int gdiWidth, FT_Referenced_BitmapGlyph glyph, FT_Render_Mode render_mode, int AAMode);
};


// 僼僅儞僩柤偲FaceID(int傪巊偆偙偲偵偡傞)
//extern CFontSetCache g_fsetcache;
extern CHashedStringList FontNameCache;
class FreeTypeFontInfo : public FreeTypeMruCounter, public FreeTypeGCCounter
{
private:
	INT_PTR  m_id;
	int  m_weight;
	bool m_italic;
	char m_hashinting;
	int  m_ftWeight;
	int  m_os2Weight;
	int  m_nMaxSizes;
	int	 m_nFontFamily;
	HFONT m_ggoFont;
	TT_OS2* m_OS2Table;
	char m_ebmps[256];
	LONG volatile count;
	CFontSettings m_set;
	StringHashFont m_hash;
	wstring	m_fullname, m_familyname;
	typedef map<UINT, FreeTypeFontCache*>	CacheArray;
	CacheArray m_cache;
	//快速链接
	FTC_FaceID face_id_link[CFontLinkInfo::FONTMAX * 2 + 1];
	HFONT ggo_link[CFontLinkInfo::FONTMAX * 2 + 1];
	bool m_linkinited;
	int m_linknum;
	FTC_FaceID m_SimSunID;
	NOCOPY(FreeTypeFontInfo);
	void Compact();
	void Createlink();

public:
	void AddRef() {InterlockedIncrement(&count);};
	void Release() {
		if (InterlockedDecrement(&count)==0)
			delete this;
	}
	TT_OS2* GetOS2Table()
	{
		if (!m_OS2Table)
		{
			TT_OS2 * os2_table = NULL;
			FT_Face freetype_face;
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_MANAGER);
			if (FTC_Manager_LookupFace(cache_man, (FTC_FaceID)m_id, &freetype_face)) 
				return NULL;
			os2_table = (TT_OS2*) FT_Get_Sfnt_Table(freetype_face, ft_sfnt_os2);
			if (!os2_table) return NULL;
			m_OS2Table = new TT_OS2;
			memcpy(m_OS2Table, os2_table, sizeof(TT_OS2));
		}
		return m_OS2Table;
	}
	BOOL FontHasHinting()
	{
		if (m_hashinting==3)
		{
			FT_Face freetype_face;
			CCriticalSectionLock __lock(CCriticalSectionLock::CS_MANAGER);
			if (FTC_Manager_LookupFace(cache_man, (FTC_FaceID)m_id, &freetype_face))	//查询ft face
			{
				m_hashinting = false;
				return NULL;
			}
			FT_ULong length = 0;
			FT_Error err = FT_Load_Sfnt_Table(freetype_face, TTAG_fpgm, 0, NULL, &length);	//获取fpgm表长度
			if (!err && length>50)		//成功读取表，并且长度较长
				m_hashinting = true;		//字体存在hinting
			else
				m_hashinting = false;
		}
		return m_hashinting;
	}
	wstring GetFullName() {return m_fullname;};
	bool m_isSimSun;
	bool IsPixel;
	UINT getCacheHash(int px, int weight, bool italic, int width) {return ((px<<20)|(width<<8)|(weight<<1)|(int)italic); };	//计算一个hash值来定位cache
	FreeTypeFontInfo(int n, LPCTSTR name, int weight, bool italic, int mru, wstring fullname, wstring familyname)
		: m_id(n), m_weight(weight), m_italic(italic), m_OS2Table(NULL), IsPixel(false)
		, FreeTypeMruCounter(mru), m_isSimSun(false), m_ggoFont(NULL), m_linkinited(false), m_linknum(0), m_os2Weight(0)
		, m_SimSunID(0), count(1), m_fullname(fullname), m_familyname(familyname), m_hashinting(3), m_nFontFamily(0)
	{
		//m_set = set;
		memset(m_ebmps, 0xff, sizeof(m_ebmps));
		
		enum { FTC_MAX_SIZES_DEFAULT = 4 };
		const CGdippSettings* pSettings = CGdippSettings::GetInstance();
		m_nMaxSizes = pSettings->CacheMaxSizes();
		if (!m_nMaxSizes)
			m_nMaxSizes = FTC_MAX_SIZES_DEFAULT;
		//extern BOOL g_EngineCreateFont;
			if (pSettings->FontSubstitutes() < SETTING_FONTSUBSTITUTE_ALL)
				m_ggoFont = CreateFont(10,0,0,0,weight,italic,0,0,DEFAULT_CHARSET,0,FONT_MAGIC_NUMBER,0,0,name);	
					//use magic number to create unsubstitud font
			else
				m_ggoFont = CreateFont(10,0,0,0,weight,italic,0,0,DEFAULT_CHARSET,0,0,0,0,name);
			HDC hdc = CreateCompatibleDC(NULL);
			HFONT old = SelectFont(hdc, m_ggoFont);
			//获得字体的全称
		
			int nSize=GetOutlineTextMetrics(hdc, 0, NULL);
			if (nSize==0)
				m_fullname = L"";
			else
			//if (m_fullname.size()==0)	//构造函数中不提供，自己获取
			{
				LPOUTLINETEXTMETRIC otm = (LPOUTLINETEXTMETRIC)malloc(nSize);
				memset(otm, 0, nSize);
				otm->otmSize = nSize;
				GetOutlineTextMetrics(hdc, nSize, otm);
				m_fullname = (wstring)(LPWSTR)((DWORD_PTR)otm + (DWORD_PTR)otm->otmpFullName);
				TCHAR * localname = (LPWSTR)((DWORD_PTR)otm+(DWORD_PTR)otm->otmpFamilyName);
				if (m_fullname.length() == 0) {
					m_fullname = localname;
				}
				m_fullname += (wstring)(LPWSTR)((DWORD_PTR)otm + (DWORD_PTR)otm->otmpStyleName);
				TCHAR buff[LF_FACESIZE+1];				
				GetFontLocalName(localname, buff);
				m_nFontFamily = otm->otmTextMetrics.tmPitchAndFamily & 0xF0;	//获取字体家族，家族对应使用什么默认链接字体
				m_familyname = (wstring)buff;
				m_set = pSettings->FindIndividual(m_familyname.c_str());
				m_ftWeight = CalcBoldWeight(/*weight*/700);
				m_hash = StringHashFont(name);
				if (m_familyname.size()>0 && m_familyname.c_str()[0]==L'@')	//附加一个@
					m_fullname = L'@'+m_fullname;
				free(otm);
			}
			SelectFont(hdc, old);
			DeleteDC(hdc);
		
			//完成
//		g_EngineCreateFont = false;
		face_id_link[0]=(FTC_FaceID)NULL;
		ggo_link[0] = NULL;
		//g_fsetcache.Set((FTC_FaceID)n, set);
	}
	~FreeTypeFontInfo()
	{
		Erase();
		DeleteFont(m_ggoFont);
		if (m_OS2Table)
			delete m_OS2Table;
	}

	HFONT GetGGOFont(){return m_ggoFont;};
	int CalcNormalWeight() const
	{
		return m_set.GetNormalWeight();
	}
	int CalcBoldWeight(int weight) const
	{
//		return weight - FW_NORMAL) / 8;
//		return ((weight - FW_NORMAL) / 12) + (m_set.GetBoldWeight() << 2);
		weight = weight < FW_BOLD ? 0: /*(weight > FW_BOLD ?*/ 612;
		if (weight <= FW_NORMAL) {
			return 0;
		}
		return ((weight - FW_NORMAL) / 8) + (m_set.GetBoldWeight() << 2);
	}
	int CalcBoldWeight(const LOGFONT& lf) const
	{
		return CalcBoldWeight(lf.lfWeight);
	}
	void CalcItalicSlant(FT_Matrix& matrix) const
	{
		matrix.xx = 1 << 16;
//		matrix.xy = 0x5800;
		matrix.xy = (5 + m_set.GetItalicSlant()) << 12;
		matrix.yx = 0;
		matrix.yy = 1 << 16;
	}

	bool Equals(const StringHashFont& hash, int weight, bool italic) const
	{
		weight = CalcBoldWeight(weight);
		return (m_ftWeight == weight && m_italic == italic && m_hash == hash);
	}
	void UpdateFontSetting()
	{
		m_ftWeight = CalcBoldWeight(700/*m_weight*/);
		//清除字体链接
		face_id_link[0]=NULL;
		ggo_link[0]=NULL;
		m_linknum = 0;
		m_linkinited = false;
		m_SimSunID = 0;
	}
	int GetFTLink(FTC_FaceID** llplink)
	{
		CCriticalSectionLock __lock(CCriticalSectionLock::CS_FONTLINK);
		if (!*face_id_link)
			Createlink();
		*llplink = face_id_link;
		return m_linknum;
	}
	int GetGGOLink(HFONT** llplink)
	{
		CCriticalSectionLock __lock(CCriticalSectionLock::CS_FONTLINK);
		if (!*ggo_link)
			Createlink();
		*llplink = ggo_link;
		return m_linknum;
	}
	FTC_FaceID GetSimSunID() {return m_SimSunID;}

	INT_PTR GetId() const { return m_id; }
	LPCTSTR GetName() const { return m_hash.c_str(); }
	int GetFontWeight() const { return m_weight; }
	int GetExactBoldWeight() const {return m_set.GetBoldWeight(); }
	int GetFTWeight() const { return m_ftWeight; }
	bool IsItalic() const { return m_italic; }
	const StringHashFont& GetHash() const { return m_hash; }

	const CFontSettings& GetFontSettings() const { return m_set; }
	void SetFontSettings(const CFontSettings& set) { m_set = set;};
	bool operator ==(const FreeTypeFontInfo& x) const { return (m_hash == x.m_hash); }

	FreeTypeFontCache* GetCache(FTC_ScalerRec& scaler, const LOGFONT& lf);
	bool EmbeddedBmpExist(int px);
	void Erase()
	{
		CCriticalSectionLock __lock(CCriticalSectionLock::CS_FONTCACHE);
		CacheArray::iterator it = m_cache.begin();
		for (;it!=m_cache.end();++it)
		{
			delete it->second;
		}
		m_cache.clear();
	}
};

class FreeTypeFontEngine : public FreeTypeGCCounter
{
private:
	//typedef CArray<FreeTypeFontInfo*>	FontListArray;
	typedef map<myfont, FreeTypeFontInfo*> FontMap;
	typedef map<wstring, FreeTypeFontInfo*> FullNameMap;
	typedef vector<FreeTypeFontInfo*> FontList;
	//FontListArray	m_arrFontList;
	int				m_nMaxFaces;
	int				m_nMemUsed;
	bool			m_bAddOnFind;
	FontMap			m_mfontMap;
	FullNameMap		m_mfullMap;
	FontList		m_mfontList;
	FT_Face*		m_arrFace;
	int				m_nFaceCount;
	void Compact();
	int GrowFace()
	{
		FT_Face* a=(FT_Face*)malloc(m_nFaceCount*sizeof(FT_Face));
		memcpy(a, m_arrFace, sizeof(FT_Face)*m_nFaceCount);
		m_nFaceCount+=64;
		m_arrFace = (FT_Face*)realloc(m_arrFace, sizeof(FT_Face) * m_nFaceCount);
		memset(m_arrFace+m_nFaceCount-64, 0, sizeof(FT_Face)*64);
		for (int i=0;i<m_nFaceCount-64;i++)
			Assert(a[i]==m_arrFace[i]);
		free(a);
		return m_nFaceCount;
	}

public:
	FreeTypeFontEngine()
		: m_nMemUsed(0), m_nMaxFaces(0), m_bAddOnFind(false), m_nFaceCount(64)
	{
		enum { FTC_MAX_FACES_DEFAULT = 2 };
		const CGdippSettings* pSettings = CGdippSettings::GetInstanceNoInit();
		m_nMaxFaces = pSettings->CacheMaxFaces();
		if (m_nMaxFaces == 0)
			m_nMaxFaces = FTC_MAX_FACES_DEFAULT;
		//m_arrFace = (FT_Face*)malloc(m_nFaceCount*sizeof(FT_Face));
		//memset(m_arrFace, 0, sizeof(FT_Face)*m_nFaceCount);
	}
	~FreeTypeFontEngine()
	{
		TRACE(_T("MaxFaces: %d\n"), m_mfontMap.size());
		TRACE(_T("MemUsed : %d\n"), m_nMemUsed);
		//FontListArray& arr = m_arrFontList;
		FullNameMap::const_iterator iter=m_mfullMap.begin();
		for (;iter!=m_mfullMap.end();++iter)
			iter->second->Release();
		//free(m_arrFace);
	}
	int CalcBoldWeight(int weight) const
	{
		return weight < FW_BOLD ? 0: FW_BOLD;
	}
	FreeTypeFontInfo* AddFont(LPCTSTR lpFaceName, int weight, bool italic, BOOL* bIsFontLoaded = NULL);
	FreeTypeFontInfo* AddFont(void* lpparams);
	int  GetFontIdByName(LPCTSTR lpFaceName, int weight, bool italic);
//	LPCTSTR GetFontById(int faceid, int& weight, bool& italic);
	FreeTypeFontInfo* FindFont(LPCTSTR lpFaceName, int weight, bool italic, bool AddOnFind = true, BOOL* bIsFontLoaded=NULL);
	FreeTypeFontInfo* FindFont(int faceid);
	FreeTypeFontInfo* FindFont(void* lpparams);

	bool FontExists(LPCTSTR lpFaceName, int weight, bool italic)
	{
		return !!FindFont(lpFaceName, weight, italic);
	}
	BOOL RemoveFont(LPCWSTR FontName);
	BOOL RemoveFont(FreeTypeFontInfo* fontinfo);
	BOOL RemoveThisFont(FreeTypeFontInfo* fontinfo, LOGFONT* lg);
	//儊儌儕巊梡検僇僂儞僞
	void AddMemUsed(int x)
	{
		m_nMemUsed += x;
	}
	void SubMemUsed(int x)
	{
		m_nMemUsed -= x;
		if (m_nMemUsed < 0)
			m_nMemUsed = 0;
	}
	template <class T>
	void AddMemUsedObj(T* /*p*/)
	{
		AddMemUsed(sizeof(T));
	}
	template <class T>
	void SubMemUsedObj(T* /*p*/)
	{
		SubMemUsed(sizeof(T));
	}
	void ReloadAll()
	{
		//重新载入全部字体，即清空所有字体缓存
		COwnedCriticalSectionLock __olock(2);
		CCriticalSectionLock __lock;
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		
		FullNameMap::const_iterator iter=m_mfullMap.begin();
		for (;iter!=m_mfullMap.end();)
		{
			FreeTypeFontInfo* p =iter->second;
			if (p)
			{
				/*
								if (p->GetFullName()!=iter->first)	//是替换字体
																{
																	p->Release();	//释放掉多重引用
																	m_mfullMap.erase(iter++);
																	continue;
																}*/
		
				p->Erase();
				p->SetFontSettings(pSettings->FindIndividual(p->GetName()));
				p->UpdateFontSetting();
			}
			++iter;
		}
		//m_mfontMap.clear();
	}
};

//GetFontData偺儊儌儕僗僩儕乕儉
class FreeTypeSysFontData
{
private:
	HDC		m_hdc;
	HFONT	m_hOldFont;
	bool	m_isTTC;
	bool	m_locked;
	void*	m_pMapping;
	DWORD	m_dwSize;
	FT_Face	m_ftFace;
	wstring m_name;
	FT_StreamRec m_ftStream;

	FreeTypeSysFontData()
		: m_hdc(NULL)
		, m_hOldFont(NULL)
		, m_isTTC(false)
		, m_locked(false)
		, m_pMapping(NULL)
		, m_dwSize(0)
		, m_ftFace(NULL)
	{
		ZeroMemory(&m_ftStream, sizeof(FT_StreamRec));
	}

	static unsigned long IoFunc(FT_Stream stream, unsigned long offset, unsigned char* buffer, unsigned long count);
	static void CloseFunc(FT_Stream  stream);
	bool OpenFaceByIndex(int index);
	bool Init(LPCTSTR name, int weight, bool italic);

public:
	static FreeTypeSysFontData* CreateInstance(LPCTSTR name, int weight, bool italic);
	~FreeTypeSysFontData()
	{
		if (m_pMapping) {
			UnmapViewOfFile(m_pMapping);
		}
		if (m_hOldFont) {
			DeleteFont(SelectFont(m_hdc, m_hOldFont));
		}
		if (m_hdc) {
			DeleteDC(m_hdc);
		}
	}

	FT_Face GetFace()
	{
		FT_Face face = m_ftFace;
		m_ftFace = NULL;
		return face;
	}
};
