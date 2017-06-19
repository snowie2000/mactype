#pragma once

#include "common.h"
#include "gdiPlusFlat2.h"
#include "cache.h"
#include "hash_list.h"
#include <VersionHelpers.h>
#include <IniParser/ParseIni.h>

#ifdef _WIN64
#ifdef DEBUG
#pragma comment (lib, "iniparser64_dbg.lib")
#else
#pragma comment (lib, "iniparser64.lib")
#endif
#else
#ifdef DEBUG
#pragma comment (lib, "iniparser_dbg.lib")
#else
#pragma comment (lib, "iniparser.lib")
#endif
#endif

#define MACTYPE_VERSION		20170614
#define MAX_FONT_SETTINGS	16
#define DEFINE_FS_MEMBER(name, param) \
	int  Get##name() const { return GetParam(param); } \
	void Set##name(int n)  { SetParam(param, n); }

#define HOOK_MANUALLY HOOK_DEFINE
#define HOOK_DEFINE(rettype, name, argtype) \
	extern rettype (WINAPI * ORIG_##name) argtype; \
	extern rettype WINAPI IMPL_##name argtype;
#include "hooklist.h"
#undef HOOK_DEFINE	//为了确保此文件也能直接使用正确的函数，需要申明。
#undef HOOK_MANUALLY

/*
struct CFontName  
{
	LPWSTR content;
public:
	bool operator < (const CFontName fn) const {
		return wcscmp(content, fn.content)>0;
	}
	CFontName(LPCWSTR fontname)
	{
		content = _wcsdup(fontname);
		wcsupr(content);
	}
	~CFontName()
	{
		free(content);
	}
};

struct CFontSubResult
{
public:
	LPWSTR lpRealName;
	LPWSTR lpGDIName;
	CFontSubResult(LPCWSTR RealName, LPCWSTR GDIName)
	{
		lpRealName = _wcsdup(RealName);
		lpGDIName = _wcsdup(GDIName);
	}
	~CFontSubResult()
	{
		free(lpRealName);
		free(lpGDIName);
	}
};

typedef map<CFontName,CFontSubResult> CFontNameCache;*/


class CFontSettings
{
private:
	int m_settings[MAX_FONT_SETTINGS];
	static const char m_bound[MAX_FONT_SETTINGS][2];

	enum _FontSettingsParams {
		FSP_HINTING			= 0,
		FSP_AAMODE			= 1,
		FSP_NORMAL_WEIGHT	= 2,
		FSP_BOLD_WEIGHT		= 3,
		FSP_ITALIC_SLANT	= 4,
		FSP_KERNING			= 5,
	};

public:
	CFontSettings()
	{
		Clear();
	}

	DEFINE_FS_MEMBER(HintingMode,	FSP_HINTING);
	DEFINE_FS_MEMBER(AntiAliasMode,	FSP_AAMODE);
	DEFINE_FS_MEMBER(NormalWeight,	FSP_NORMAL_WEIGHT);
	DEFINE_FS_MEMBER(BoldWeight,	FSP_BOLD_WEIGHT);
	DEFINE_FS_MEMBER(ItalicSlant,	FSP_ITALIC_SLANT);
	DEFINE_FS_MEMBER(Kerning,		FSP_KERNING);

	int GetParam(int x) const
	{
		Assert(0 <= x && x < MAX_FONT_SETTINGS);
		return m_settings[x];
	}
	void SetParam(int x, int n)
	{
		Assert(0 <= x && x < MAX_FONT_SETTINGS);
		m_settings[x] = Bound<int>(n, m_bound[x][0], m_bound[x][1]);
	}
	void Clear()
	{
		ZeroMemory(m_settings, sizeof(m_settings));
	}

	void SetSettings(const int* p, int count)
	{
		count = Min(count, MAX_FONT_SETTINGS);
		memcpy(m_settings, p, count * sizeof(int));
	}
};

#undef DEFINE_FS_MEMBER


class CFontIndividual
{
	CFontSettings	m_set;
	StringHashFont	m_hash;

public:
	CFontIndividual()
	{
	}
	CFontIndividual(LPCTSTR name)
		: m_hash(name)
	{
	}

	CFontSettings& GetIndividual() { return m_set; }
	LPCTSTR GetName() const { return m_hash.c_str(); }
	const StringHashFont& GetHash() const { return m_hash; }
	bool operator ==(const CFontIndividual& x) const { return (m_hash == x.m_hash); }
};

class CFontLinkInfo
{
public:
	enum {
		INFOMAX = 180,	//源文件=15
		FONTMAX = 31,
	};
private:
	LPWSTR info[INFOMAX + 1][FONTMAX + 1];
	bool AllowDefaultLink[256];
	WCHAR DefaultFontLink[FF_DECORATIVE + 1][LF_FACESIZE + 1];	//存放对应字体类型的默认链接
public:
	CFontLinkInfo();
	~CFontLinkInfo();
	void init();
	void clear();
	const bool IsAllowFontLink(BYTE aCharset) const { return AllowDefaultLink[aCharset]; }
	const LPCWSTR sysfn(int nFontFamily) const { 
		return *DefaultFontLink[nFontFamily] ? DefaultFontLink[nFontFamily] : DefaultFontLink[1];	}
	const LPCWSTR * lookup(LPCWSTR fontname) const;
	LPCWSTR get(int row, int col) const;
};

class CFontSubstitutesInfo;

class CFontSubstituteData
{
	friend CFontSubstitutesInfo;
private:
	LOGFONT m_lf;
	TEXTMETRIC m_tm;
	bool m_bCharSet;
	//TCHAR CustomName[LF_FACESIZE];
public:
	bool operator == (const CFontSubstituteData& o) const;
private:
	CFontSubstituteData();
	bool init(LPCTSTR config);
	static int CALLBACK EnumFontFamProc(const LOGFONT *lplf, const TEXTMETRIC *lptm, DWORD FontType, LPARAM lParam);

};

typedef StringHashT<LF_FACESIZE + 10,true> CFontSubstitutesHash;
typedef set<wstring> CFontSubstitutesIniArray;

class CFontSubstitutesInfo : public CSimpleMap<CFontSubstituteData, CFontSubstituteData>
{
private:
	//typedef map<wstring, wstring> FontSubMap; 
	//FontSubMap m_mfontsub;
	void initini(const CFontSubstitutesIniArray& iniarray);
	void initreg();
public:
	void init(int nFontSubstitutes, const CFontSubstitutesIniArray& iniarray);
	const LOGFONT * lookup(LOGFONT &lf) const;
	//void RemoveAll() {m_mfontsub.clear();};
	//bool TrySub(LPCTSTR lpFacename) {return m_mfontsub.find(lpFacename)!=m_mfontsub.end(); };
};

#define SETTING_FONTSUBSTITUTE_DISABLE (0)
#define SETTING_FONTSUBSTITUTE_INIONLY (1)
#define SETTING_FONTSUBSTITUTE_ALL     (2)

#define SETTING_WIDTHMODE_GDI32    (0)
#define SETTING_WIDTHMODE_FREETYPE (1)

#define SETTING_FONTLOADER_FREETYPE  (0)
#define SETTING_FONTLOADER_WIN32     (1)

class CGdippSettings;

interface IControlCenter
{
	virtual HRESULT STDMETHODCALLTYPE QueryInterface( 
		/* [in] */ REFIID riid,
		/* [iid_is][out] */ __RPC__deref_out void __RPC_FAR *__RPC_FAR *ppvObject) = 0;
	virtual ULONG STDMETHODCALLTYPE AddRef( void) = 0;
	virtual ULONG STDMETHODCALLTYPE Release( void) = 0;
	virtual ULONG WINAPI GetVersion(void) = 0;
	virtual BOOL WINAPI SetIntAttribute(int eSet, int nValue) = 0;
	virtual BOOL WINAPI SetFloatAttribute(int eSet, float nValue) = 0;
	virtual int WINAPI GetIntAttribute(int eSet) = 0;
	virtual float WINAPI GetFloatAttribute(int eSet) = 0;
	virtual BOOL WINAPI RefreshSetting(void) = 0;
	virtual BOOL WINAPI EnableRender(BOOL bEnable) = 0;
	virtual BOOL WINAPI EnableCache(BOOL bEnable) = 0;
	virtual BOOL WINAPI ClearIndividual() = 0;
	virtual BOOL WINAPI AddIndividual(WCHAR* fontSetting) = 0;
	virtual BOOL WINAPI DelIndividual(WCHAR* lpFaceName) = 0;
	virtual void WINAPI LoadSetting(WCHAR* lpFileName) = 0;
};
class CControlCenter;

class CGdippSettings
{
	friend CControlCenter;
	friend CFontSubstituteData;
	friend BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID);
private:
	static CGdippSettings* s_pInstance;
	//INI梡
	CFontSettings m_FontSettings;
	static CParseIni m_Config;
	bool m_bHookChildProcesses		: 1;
	bool m_bUseMapping				: 1;
	bool m_bLoadOnDemand			: 1;
	bool m_bEnableShadow			: 1;

	//偦傟埲奜
	bool m_bIsWinXPorLater			: 1;
	bool m_bRunFromGdiExe			: 1;
	bool m_bIsInclude				: 1;
	bool m_bDelayedInit				: 1;
//	bool m_bIsHDBench				: 1;
//	bool m_bHaveNewerFreeType		: 1;
	bool							: 0;
	bool m_bUseCustomLcdFilter;	//使用自定义lcdfilter

	BOOL m_bHintSmallFont;
	BOOL m_bDirectWrite;
	int  m_nBolderMode;
	int  m_nGammaMode;
	float m_fGammaValue;
	float m_fRenderWeight;
	float m_fContrast;
	int  m_nMaxHeight;
	int  m_nMinHeight;
	int  m_nBitmapHeight;
	int  m_nLcdFilter;
	int  m_nShadow[4];
	int  m_nFontSubstitutes;
	int	 m_bFontLink;	//改为可以使用多种参数
	int  m_nWidthMode;
	int  m_nFontLoader;
	int	 m_nScreenDpi;	// screen dpi
	DWORD m_nShadowLightColor;
	DWORD m_nShadowDarkColor;
	unsigned char m_arrLcdFilterWeights[5];

	//settings for experimental
	bool m_bEnableClipBoxFix;

	//settings for directwrite
	float m_fGammaValueForDW;
	float m_fContrastForDW;
	float m_fClearTypeLevelForDW;
	int	m_nRenderingModeForDW;

	//FTC_Manager_New偵搉偡僷儔儊乕僞
	int  m_nCacheMaxFaces;
	int  m_nCacheMaxSizes;
	int  m_nCacheMaxBytes;

	// 傾儞僠僄僀儕傾僗挷惍梡僥乕僽儖
	int  m_nTuneTable[256];
	// LCD梡
	int  m_nTuneTableR[256];
	int  m_nTuneTableG[256];
	int  m_nTuneTableB[256];
	static TCHAR m_szexeName[MAX_PATH+1];

	typedef set<wstring>	FontHashMap;
	typedef set<wstring>	ModuleHashMap;
	typedef set<wstring> FontSubSet;
	typedef CArray<CFontIndividual>	IndividualArray;
	FontHashMap		m_arrExcludeFont;
	ModuleHashMap	m_arrExcludeModule;
	ModuleHashMap	m_arrIncludeModule;
	ModuleHashMap	m_arrUnloadModule;
	ModuleHashMap	m_arrUnFontSubModule;
	IndividualArray	m_arrIndividual;

	// 巜掕僼僅儞僩
	LOGFONT m_lfForceFont;
	TCHAR m_szForceChangeFont[LF_FACESIZE];

	//INI僼傽僀儖柤
	TCHAR m_szFileName[MAX_PATH];

	//INI偐傜偺撉傒崬傒張棟
	bool LoadAppSettings(LPCTSTR lpszFile);
	float FastGetProfileFloat(LPCTSTR lpszSection, LPCTSTR lpszKey, float fDefault);
	int FastGetProfileInt(LPCTSTR lpszSection, LPCTSTR lpszKey, int nDefault);
	DWORD FastGetProfileString(LPCTSTR lpszSection, LPCTSTR lpszKey, LPCTSTR lpszDefault, LPTSTR lpszRet, DWORD cch);
	static bool		_IsFreeTypeProfileSectionExists(LPCTSTR lpszKey, LPCTSTR lpszFile);
	static LPTSTR _GetPrivateProfileSection    (LPCTSTR lpszSection, LPCTSTR lpszFile);
	static int    _GetFreeTypeProfileInt       (LPCTSTR lpszKey, int nDefault, LPCTSTR lpszFile);
	static int	  _GetFreeTypeProfileIntFromSection(LPCTSTR lpszSection, LPCTSTR lpszKey, int nDefault, LPCTSTR lpszFile);
	static int    _GetFreeTypeProfileBoundInt  (LPCTSTR lpszKey, int nDefault, int nMin, int nMax, LPCTSTR lpszFile);
	static float  _GetFreeTypeProfileFloat     (LPCTSTR lpszKey, float fDefault, LPCTSTR lpszFile);
	static float  _GetFreeTypeProfileBoundFloat(LPCTSTR lpszKey, float fDefault, float fMin, float fMax, LPCTSTR lpszFile);
	static DWORD  _GetFreeTypeProfileString    (LPCTSTR lpszKey, LPCTSTR lpszDefault, LPTSTR lpszRet, DWORD cch, LPCTSTR lpszFile);
	static int CALLBACK EnumFontFamProc(const LOGFONT* lplf, const TEXTMETRIC* lptm, DWORD FontType, LPARAM lParam);
	//template <typename T>
	static bool AddListFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, set<wstring> & arr);
	static bool AddExcludeListFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, set<wstring> & arr);
	bool AddIndividualFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, IndividualArray& arr);
	bool AddLcdFilterFromSection(LPCTSTR lpszKey, LPCTSTR lpszFile, unsigned char* arr);
	static int   _StrToInt(LPCTSTR pStr, int nDefault);
	static float _StrToFloat(LPCTSTR pStr, float fDefault);
	static int _httoi(const TCHAR *value);
	void InitInitTuneTable();
	static void InitTuneTable(int v, int* table);
	void DelayedInit();

	CFontLinkInfo m_fontlinkinfo;
	CFontSubstitutesInfo m_FontSubstitutesInfo;

	CGdippSettings()
		: m_bHookChildProcesses(false)
		, m_bUseMapping(false)
		, m_bLoadOnDemand(false)
		, m_bEnableShadow(false)
		, m_bFontLink(0)
//		, m_bEnableKerning(false)
		, m_bIsWinXPorLater(false)
		, m_bRunFromGdiExe(false)
		, m_bIsInclude(false)
		, m_bDelayedInit(false)
//		, m_bIsHDBench(false)
//		, m_bHaveNewerFreeType(false)
		, m_nBolderMode(0)
		, m_nGammaMode(0)
		, m_fGammaValue(1.0f)
		, m_fGammaValueForDW(0.0f)
		, m_fRenderWeight(1.0f)
		, m_fContrast(1.0f)
		, m_nMaxHeight(0)
		, m_nMinHeight(0)
		, m_nBitmapHeight(0)
		, m_nLcdFilter(0)
		, m_nCacheMaxFaces(0)
		, m_nCacheMaxSizes(0)
		, m_nCacheMaxBytes(0)
		, m_bHintSmallFont(true)
		, m_bDirectWrite(true)
		, m_nScreenDpi(96)
	{
		ZeroMemory(m_nTuneTable,		sizeof(m_nTuneTable));
		ZeroMemory(m_nTuneTableR,		sizeof(m_nTuneTableR));
		ZeroMemory(m_nTuneTableG,		sizeof(m_nTuneTableG));
		ZeroMemory(m_nTuneTableB,		sizeof(m_nTuneTableB));
		ZeroMemory(&m_lfForceFont,		sizeof(LOGFONT));
		ZeroMemory(m_nShadow,			sizeof(m_nShadow));
		ZeroMemory(m_szFileName,		sizeof(m_szFileName));
		ZeroMemory(m_szForceChangeFont,	sizeof(m_szForceChangeFont));
	}

	~CGdippSettings()
	{

	}

	static CGdippSettings* CreateInstance();
	static void DestroyInstance();

public:
	static CGdippSettings* GetInstance();
	static const CGdippSettings* GetInstanceNoInit();	//FreeTypeFontEngine

	//INI梡
	const CFontSettings& GetFontSettings() const { return m_FontSettings; }
	bool HookChildProcesses() const { return m_bHookChildProcesses; }
	bool UseMapping() const { return m_bUseMapping; }
	bool LoadOnDemand() const { return m_bLoadOnDemand; }
	char FontLink() const { return m_bFontLink; }
	BOOL DirectWrite() const { return m_bDirectWrite; }
	BOOL HintSmallFont() const { return m_bHintSmallFont; }
//	bool EnableKerning() const { return m_bEnableKerning; }

	int BolderMode() const { return m_nBolderMode; }
	int GammaMode() const { return m_nGammaMode; }
	float GammaValue() const { return m_fGammaValue; }

	//DW options
	float GammaValueForDW() const {	return m_fGammaValueForDW;	}
	float ContrastForDW() const { return m_fContrastForDW;  }
	float ClearTypeLevelForDW() const { return m_fClearTypeLevelForDW;  }
	int RenderingModeForDW() const { return m_nRenderingModeForDW; }

	float RenderWeight() const { return m_fRenderWeight; }
	float Contrast() const { return m_fContrast; }
	int MaxHeight() const { return m_nMaxHeight; }
	int MinHeight() const { return m_nMinHeight; }
	int BitmapHeight() const { return m_nBitmapHeight; }
	int ScreenDpi() const { return m_nScreenDpi;  }
	int LcdFilter() const { return m_nLcdFilter; }
	const unsigned char* LcdFilterWeights() const { return m_arrLcdFilterWeights; }
	bool UseCustomLcdFilter() const { return m_bUseCustomLcdFilter; }
	int WidthMode() const { return m_nWidthMode; }
	int FontLoader() const { return m_nFontLoader; }
	bool EnableClipBoxFix() const { return m_bEnableClipBoxFix; }
	DWORD ShadowLightColor() const { return m_nShadowLightColor; }
	DWORD ShadowDarkColor() const { return m_nShadowDarkColor; }
	int FontSubstitutes() const { return m_nFontSubstitutes; }	//判断替换模式
	int CacheMaxFaces() const { return m_nCacheMaxFaces; }
	int CacheMaxSizes() const { return m_nCacheMaxSizes; }
	int CacheMaxBytes() const { return m_nCacheMaxBytes; }

	bool EnableShadow()  const { return m_bEnableShadow; }
	const int* GetShadowParams() const { return m_nShadow; }

	// 僼僅儞僩柤傛傒偲傝
	LPCTSTR GetForceFontName() const
	{
		_ASSERTE(m_bDelayedInit);
		LPCTSTR lpszFace = m_lfForceFont.lfFaceName;
		return lpszFace[0] ? lpszFace : NULL;
	}

	bool CopyForceFont(LOGFONT& lf, const LOGFONT& lfOrg) const;

	//偦傟埲奜
	bool IsWinXPorLater() const { return m_bIsWinXPorLater; }
	bool IsInclude() const { return m_bIsInclude; }
//	bool IsHDBench() const { return m_bIsHDBench; }
	bool RunFromGdiExe() const { return m_bRunFromGdiExe; }
//	bool HaveNewerFreeType() const { return m_bHaveNewerFreeType; }
	const int* GetTuneTable() const { return m_nTuneTable; }
	const int* GetTuneTableR() const { return m_nTuneTableR; }
	const int* GetTuneTableG() const { return m_nTuneTableG; }
	const int* GetTuneTableB() const { return m_nTuneTableB; }

	bool LoadSettings(HINSTANCE hModule);

	bool IsFontExcluded(LPCSTR lpFaceName) const;
	bool IsFontExcluded(LPCWSTR lpFaceName) const;

	//Snowie!!
	bool IsProcessUnload() const;
	bool IsExeUnload(LPCTSTR lpApp) const;
	bool IsExeInclude(LPCTSTR lpApp) const;
	void AddFontExclude(LPCWSTR lpFaceName);	//点阵字体直接自动添加到此列表
	bool IsProcessExcluded() const;
	bool IsProcessIncluded() const;
	const CFontSettings& FindIndividual(LPCTSTR lpFaceName) const;

	const CFontLinkInfo& GetFontLinkInfo() const
		{ _ASSERTE(m_bDelayedInit); return m_fontlinkinfo; }
	const CFontSubstitutesInfo& GetFontSubstitutesInfo() const
		{ _ASSERTE(m_bDelayedInit); return m_FontSubstitutesInfo; }
};

class CFontFaceNamesEnumerator
{
private:
	enum {
		MAXFACENAMES = CFontLinkInfo::FONTMAX * 2 + 1,
	};
	LPCWSTR m_facenames[MAXFACENAMES];
	int m_pos;
	int m_endpos;
	CFontFaceNamesEnumerator();
public:
	CFontFaceNamesEnumerator(LPCWSTR facename, int nFontFamily);
	operator LPCWSTR () {
		return m_facenames[m_pos];
	}
	LPCWSTR getname()
	{
		return m_facenames[m_pos];
	}
	void next() {
		++m_pos;
	}
	void prev() {
		m_pos>0?--m_pos:0;
	}
	bool atend() {
		return !!(m_pos >= m_endpos);
	}
};
#include "fteng.h"
#include "ft.h"
#include <freetype/ftlcdfil.h>
#include "strtoken.h"
extern FreeTypeFontEngine* g_pFTEngine;
extern BOOL g_ccbCache;
extern BOOL g_ccbRender;

class CControlCenter: public IControlCenter
{
private:
	int m_nRefCount;
	bool m_bDirty;
	enum eMTSettings{
		ATTR_HINTINGMODE,
		ATTR_ANTIALIASMODE,
		ATTR_NormalWeight,
		ATTR_BoldWeight,
		ATTR_ItalicSlant,
		ATTR_EnableKerning,
		ATTR_GammaMode,
		ATTR_LcdFilter,
		ATTR_BolderMode,
		ATTR_TextTuning,
		ATTR_TextTuningR,
		ATTR_TextTuningG,
		ATTR_TextTuningB,
		ATTR_GammaValue,
		ATTR_Contrast,
		ATTR_RenderWeight,
		ATTR_ShadowAlpha,
		ATTR_ShadowOffset,
		ATTR_Fontlink,
		ATTR_HookChildProcess,
		ATTR_LoadOnDemand,
		ATTR_FontLoader,
		ATTR_FontSubstitute,
		ATTR_LcdFilterWeight,
		ATTR_ShadowBuffer,
		ATTR_MaxBitmap, 
		ATTR_DirectWrite, 
		ATTR_HintSmallFont
	};
	typedef CArray<CFontIndividual>		IndividualArray;
public:
	HRESULT STDMETHODCALLTYPE QueryInterface( 
		/* [in] */ REFIID riid,
		/* [iid_is][out] */ __RPC__deref_out void __RPC_FAR *__RPC_FAR *ppvObject)
	{
		ppvObject = (void**)this;
		return 0;
	}
	ULONG STDMETHODCALLTYPE AddRef( void) {return InterlockedIncrement((LONG*)&m_nRefCount);};
	ULONG STDMETHODCALLTYPE Release( void) { 
		int result = InterlockedDecrement((LONG*)&m_nRefCount);
		if (!result)
			delete this;
		return result;
	}
	ULONG WINAPI GetVersion(void){ return MACTYPE_VERSION; };
	BOOL WINAPI SetIntAttribute(int eSet, int nValue)
	{
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		switch ((eMTSettings)eSet)
		{
		case ATTR_HINTINGMODE:
			pSettings->m_FontSettings.SetHintingMode(nValue);
			break;
		case ATTR_ANTIALIASMODE:
			pSettings->m_FontSettings.SetAntiAliasMode(nValue);
			break;
		case ATTR_NormalWeight:
			pSettings->m_FontSettings.SetNormalWeight(nValue);
			break;
		case ATTR_BoldWeight:
			pSettings->m_FontSettings.SetBoldWeight(nValue);
			break;
		case ATTR_ItalicSlant:
			pSettings->m_FontSettings.SetItalicSlant(nValue);
			break;
		case ATTR_EnableKerning:
			pSettings->m_FontSettings.SetKerning(nValue);
			break;
		case ATTR_GammaMode:
			pSettings->m_nGammaMode = nValue;
			RefreshAlphaTable();
			break;
		case ATTR_LcdFilter:
			pSettings->m_nLcdFilter = nValue;
			FT_Library_SetLcdFilter(freetype_library, (FT_LcdFilter)nValue);
			break;
		case ATTR_BolderMode:
			pSettings->m_nBolderMode = nValue;
			break;
		case ATTR_TextTuning:
			pSettings->InitTuneTable(nValue,  pSettings->m_nTuneTable);
			RefreshAlphaTable();
			break;
		case ATTR_TextTuningR:
			pSettings->InitTuneTable(nValue,  pSettings->m_nTuneTableR);
			RefreshAlphaTable();
			break;
		case ATTR_TextTuningG:
			pSettings->InitTuneTable(nValue,  pSettings->m_nTuneTableG);
			RefreshAlphaTable();
			break;
		case ATTR_TextTuningB:
			pSettings->InitTuneTable(nValue,  pSettings->m_nTuneTableB);
			RefreshAlphaTable();
			break;
		case ATTR_LoadOnDemand:
			pSettings->m_bLoadOnDemand = !!nValue;
			break;
		case ATTR_ShadowAlpha:
			pSettings->m_nShadow[2] = nValue;
			pSettings->m_bEnableShadow = (nValue!=1);
			RefreshAlphaTable();
			break;
		case ATTR_ShadowOffset:
			pSettings->m_nShadow[1] = nValue;
			pSettings->m_nShadow[0] = nValue;
		case ATTR_Fontlink:
			if (!!pSettings->m_bFontLink != !!nValue)
			{
				pSettings->m_fontlinkinfo.clear();
				if (nValue)
					pSettings->m_fontlinkinfo.init();
			}
			pSettings->m_bFontLink = nValue;
			break;	
		case ATTR_FontLoader:
			pSettings->m_nFontLoader = nValue;
			break;
		case ATTR_LcdFilterWeight:
			if (!nValue)
				pSettings->m_bUseCustomLcdFilter = false;	//传NULL过来就是关闭自定义过滤器
			else
			{
				pSettings->m_bUseCustomLcdFilter = true;	//否则打开过滤器
				if (!IsBadReadPtr((void*)nValue, sizeof(pSettings->m_arrLcdFilterWeights)))	//如果指针有效
					memcpy(pSettings->m_arrLcdFilterWeights, (void*)nValue, sizeof(pSettings->m_arrLcdFilterWeights));	//复制数据
			}
			UpdateLcdFilter();	//刷新过滤器
			break;
		case ATTR_HintSmallFont:
			pSettings->m_bHintSmallFont = !!nValue;
			break;
		case ATTR_MaxBitmap:
			pSettings->m_nBitmapHeight = nValue;
			break;
		case ATTR_ShadowBuffer:
			if (nValue && !IsBadReadPtr((void*)nValue, sizeof(pSettings->m_nShadow)))	//指针有效
			{
				LPCTSTR szShadow = (LPCTSTR)nValue;
				CStringTokenizer token;

				if (token.Parse(szShadow) < 3) {
					break;
				}
				for (int i=0; i<3; i++) {
					pSettings->m_nShadow[i] = pSettings->_StrToInt(token.GetArgument(i), 0);
					/*if (m_nShadow[i] <= 0) {
						goto SKIP;
					}*/
				}
				pSettings->m_bEnableShadow = true;
				if (token.GetCount()>=4)	//如果指定了浅色阴影
					pSettings->m_nShadowDarkColor = pSettings->_httoi(token.GetArgument(3));	//读取阴影
				else
					pSettings->m_nShadowDarkColor = 0;	//否则为黑色
				if (token.GetCount()>=6)	//如果指定了深色阴影
				{
					pSettings->m_nShadowLightColor = pSettings->_httoi(token.GetArgument(5));	//读取阴影
					pSettings->m_nShadow[3] = pSettings->_StrToInt(token.GetArgument(4), pSettings->m_nShadow[2]); //读取深度
				}
				else
				{
					//pSettings->m_nShadowLightColor = pSettings->m_nShadowLightColor;		//否则和浅色阴影相同
					pSettings->m_nShadow[3] = pSettings->m_nShadow[2];		//深度也相同
				}
				RefreshAlphaTable();
			}
			break;
		default:
			return FALSE;
		}	
		m_bDirty = true;
		return true;
	}
	BOOL WINAPI SetFloatAttribute(int eSet, float nValue)
	{
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		switch ((eMTSettings)eSet)
		{
			case ATTR_GammaValue:
				pSettings->m_fGammaValue = nValue;
				RefreshAlphaTable();
				break;
			case ATTR_Contrast:
				pSettings->m_fContrast = nValue;
				RefreshAlphaTable();
				break;
			case ATTR_RenderWeight:
				pSettings->m_fRenderWeight = nValue;
				RefreshAlphaTable();
				break;
			default:
				return FALSE;
		}	
		m_bDirty = true;
		return true;
	};
	int WINAPI GetIntAttribute(int eSet) {
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		switch ((eMTSettings)eSet)
		{
		case ATTR_HINTINGMODE:
			return pSettings->m_FontSettings.GetHintingMode();
		case ATTR_ANTIALIASMODE:
			return pSettings->m_FontSettings.GetAntiAliasMode();
		case ATTR_NormalWeight:
			return pSettings->m_FontSettings.GetNormalWeight();
		case ATTR_BoldWeight:
			return pSettings->m_FontSettings.GetBoldWeight();
		case ATTR_ItalicSlant:
			return pSettings->m_FontSettings.GetItalicSlant();
		case ATTR_EnableKerning:
			return pSettings->m_FontSettings.GetKerning();
		case ATTR_GammaMode:
			return pSettings->m_nGammaMode;
		case ATTR_HintSmallFont:
			return pSettings->m_bHintSmallFont;
		case ATTR_MaxBitmap:
			return pSettings->m_nBitmapHeight;
		case ATTR_LcdFilter:
			return pSettings->m_nLcdFilter;
		case ATTR_BolderMode:
			return pSettings->m_nBolderMode;
		case ATTR_TextTuning:
			return 0;
		case ATTR_TextTuningR:
			return 0;
		case ATTR_TextTuningG:
			return 0;
		case ATTR_TextTuningB:
			return 0;
		case ATTR_LoadOnDemand:
			return pSettings->m_bLoadOnDemand;
		case ATTR_ShadowAlpha:
			return pSettings->EnableShadow()? pSettings->m_nShadow[2] : 1;
		case ATTR_ShadowOffset:
			return pSettings->EnableShadow()? pSettings->m_nShadow[0] : 1;
		case ATTR_Fontlink:
			return pSettings->m_bFontLink;
		case ATTR_LcdFilterWeight:
			return pSettings->m_bUseCustomLcdFilter? (int)pSettings->m_arrLcdFilterWeights:NULL;
		default:
			return 0;
		}	
		return 0;
	};
	float WINAPI GetFloatAttribute(int eSet) {
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		switch ((eMTSettings)eSet)
		{
		case ATTR_GammaValue:
			return pSettings->m_fGammaValue;
		case ATTR_Contrast:
			return pSettings->m_fContrast;
		case ATTR_RenderWeight:
			return pSettings->m_fRenderWeight;
		default:
			return 0;
		}	
		return 0;
	};
	BOOL WINAPI RefreshSetting(void) {
		if (m_bDirty)
			g_pFTEngine->ReloadAll();
		m_bDirty = false;
		return true;
	};
	BOOL WINAPI EnableRender(BOOL bEnable){g_ccbRender = bEnable; return true;};
	BOOL WINAPI EnableCache(BOOL bEnable){g_ccbCache = bEnable; return true;};
	BOOL WINAPI ClearIndividual(){
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		pSettings->m_arrIndividual.RemoveAll();
		m_bDirty = true;
		return true;
	};
	BOOL WINAPI AddIndividual(WCHAR* fontSetting) 
	{ 
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		IndividualArray& arr = pSettings->m_arrIndividual;
		LPTSTR p = fontSetting;
		LOGFONT truefont={0};
		if (*p) {
			bool b = false;

			LPTSTR pnext = p;
			for (; *pnext; pnext++);

			//"俵俽 俹僑僔僢僋=0,0" 傒偨偄側暥帤楍傪暘妱
			LPTSTR value = _tcschr(p, _T('='));
			CStringTokenizer token;
			int argc = 0;
			if (value) {
				*value++ = _T('\0');
				argc = token.Parse(value);
			}
			TCHAR buff[LF_FACESIZE+1];				
			GetFontLocalName(p, buff); //转换字体名

			CFontIndividual fi(buff);
			const CFontSettings& fsCommon = pSettings->m_FontSettings;
			CFontSettings& fs = fi.GetIndividual();
			//Individual偑柍偗傟偽嫟捠愝掕傪巊偆
			fs = fsCommon;
			for (int i = 0; i < MAX_FONT_SETTINGS; i++) {
				LPCTSTR arg = token.GetArgument(i);
				if (!arg)
					break;
				const int n = pSettings->_StrToInt(arg, fsCommon.GetParam(i));
				fs.SetParam(i, n);
			}

			for (int i = 0 ; i < arr.GetSize(); i++) {
				if (arr[i] == fi) {
					b = true;
					break;
				}
			}
			if (!b) {
				arr.Add(fi);
			}
		}
		m_bDirty = true;
		return true; };
	BOOL WINAPI DelIndividual(WCHAR* lpFaceName) { 
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		CFontIndividual* p		= pSettings->m_arrIndividual.Begin();
		CFontIndividual* end	= pSettings->m_arrIndividual.End();
		StringHashFont hash(lpFaceName);

		for(; p != end; ++p) {
			if (p->GetHash() == hash) {
				pSettings->m_arrIndividual.Remove(*p);
			}
		}
		m_bDirty = true;
		return true;};
	void WINAPI LoadSetting(WCHAR* lpFileName)
	{
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		ClearIndividual();
		pSettings->m_FontSubstitutesInfo.RemoveAll();
		pSettings->m_fontlinkinfo.clear();
		pSettings->LoadAppSettings(lpFileName);
		pSettings->m_bDelayedInit = false;
		m_bDirty = true;
		RefreshAlphaTable();
		RefreshSetting();
	}
	CControlCenter():m_nRefCount(1), m_bDirty(false){};
	~CControlCenter(){};
	static void WINAPI ReloadConfig()
	{
		//CCriticalSectionLock __lock(CCriticalSectionLock::CS_LIBRARY);
		CGdippSettings* pSettings = CGdippSettings::GetInstance();
		extern HINSTANCE g_dllInstance;
		pSettings->m_arrIndividual.RemoveAll();
		pSettings->m_FontSubstitutesInfo.RemoveAll();
		pSettings->m_fontlinkinfo.clear();
		pSettings->LoadSettings(g_dllInstance);
		pSettings->m_bDelayedInit = false;
		RefreshAlphaTable();
		UpdateLcdFilter();
		if (g_pFTEngine)
			g_pFTEngine->ReloadAll();
	}
};
