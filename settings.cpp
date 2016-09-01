#include "settings.h"
#include "strtoken.h"
#include <math.h>	//pow
#include "supinfo.h"
#include "fteng.h"
#include <stdlib.h>

CGdippSettings* CGdippSettings::s_pInstance;
CHashedStringList FontNameCache;

static const TCHAR c_szGeneral[]  = _T("General");
static const TCHAR c_szFreeType[] = _T("FreeType");
#define HINTING_MIN			0
#define HINTING_MAX			2
#define AAMODE_MIN			-1
#define AAMODE_MAX			5
#define GAMMAVALUE_MIN		0.0625f
#define GAMMAVALUE_MAX		10.0f
#define CONTRAST_MIN		0.0625f
#define CONTRAST_MAX		10.0f
#define RENDERWEIGHT_MIN	0.0625f
#define RENDERWEIGHT_MAX	10.0f
#define NWEIGHT_MIN			-64
#define NWEIGHT_MAX			+64
#define BWEIGHT_MIN			-32
#define BWEIGHT_MAX			+32
#define SLANT_MIN			-32
#define SLANT_MAX			+32

CGdippSettings* CGdippSettings::CreateInstance()
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);
	CGdippSettings* pSettings = new CGdippSettings;
	CGdippSettings* pOldSettings = reinterpret_cast<CGdippSettings*>(InterlockedExchangePointer(reinterpret_cast<void**>(&s_pInstance), pSettings));
	_ASSERTE(pOldSettings == NULL);
	return pSettings;
}

void CGdippSettings::DestroyInstance()
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);

	CGdippSettings* pSettings = reinterpret_cast<CGdippSettings*>(InterlockedExchangePointer(reinterpret_cast<void**>(&s_pInstance), NULL));
	if (pSettings) {
		delete pSettings;
	}
}

CGdippSettings* CGdippSettings::GetInstance()
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);
	CGdippSettings* pSettings = s_pInstance;
	_ASSERTE(pSettings != NULL);

	if (!pSettings->m_bDelayedInit) {
		pSettings->DelayedInit();
	}
	return pSettings;
}

const CGdippSettings* CGdippSettings::GetInstanceNoInit()
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);
	CGdippSettings* pSettings = s_pInstance;
	_ASSERTE(pSettings != NULL);
	return pSettings;
}

void CGdippSettings::DelayedInit()
{
	if (!g_pFTEngine) {
		return;
	}
	if (IsBadCodePtr((FARPROC)RegOpenKeyExW) || *(DWORD_PTR*)RegOpenKeyExW==0)
		return;
	
	m_bDelayedInit = true;

	//ForceChangeFont
	if (m_szForceChangeFont[0]) {
		HDC hdc = GetDC(NULL);
		EnumFontFamilies(hdc, m_szForceChangeFont, EnumFontFamProc, reinterpret_cast<LPARAM>(this));
		ReleaseDC(NULL, hdc);
	}

// 	//FontLink
// 	if (FontLink()) {
// 		m_fontlinkinfo.init();
// 	}

	//FontSubstitutes
	CFontSubstitutesIniArray arrFontSubstitutes;
	wstring names = _T("FontSubstitutes@") + wstring(m_szexeName);
	if (_IsFreeTypeProfileSectionExists(names.c_str(), m_szFileName))
		AddListFromSection(names.c_str(), m_szFileName, arrFontSubstitutes);
	else
		AddListFromSection(_T("FontSubstitutes"), m_szFileName, arrFontSubstitutes);
	m_FontSubstitutesInfo.init(m_nFontSubstitutes, arrFontSubstitutes);



	WritePrivateProfileString(NULL, NULL, NULL, m_szFileName);

	//m_bDelayedInit = true;

	//FontLink
	if (FontLink()) {
		m_fontlinkinfo.init();
	}


	//嫮惂僼僅儞僩
/*
	LPCTSTR lpszFace = GetForceFontName();
	if (lpszFace)
		g_pFTEngine->AddFont(lpszFace, FW_NORMAL, false);*/


/*	DWORD dwVersion = GetVersion();

	if (m_bDirectWrite && (DWORD)(LOBYTE(LOWORD(dwVersion)))>5)	//vista or later
	{
		if (GetModuleHandle(_T("d2d1.dll")))	//directwrite support
			HookD2D1();
	}*/
}

bool CGdippSettings::LoadSettings()
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);
	Assert(m_szFileName[0]);
	return LoadAppSettings(m_szFileName);
}

bool CGdippSettings::LoadSettings(HINSTANCE hModule)
{
	CCriticalSectionLock __lock(CCriticalSectionLock::CS_SETTING);
	if (!::GetModuleFileName(hModule, m_szFileName, MAX_PATH - sizeof(".ini") + 1)) {
		return false;
	}

	*PathFindExtension(m_szFileName) = L'\0';
#ifdef _WIN64
	if (wcsstr(m_szFileName, L"64"))
		wcscpy(wcsstr(m_szFileName, L"64"), wcsstr(m_szFileName, L"64")+2);	//删掉其中的64，统一使用mactype.ini配置文件
#endif
	wcsncat(m_szFileName, _T(".ini"), MAX_PATH);
	//StringCchCat(m_szFileName, MAX_PATH, _T(".ini"));
	return LoadAppSettings(m_szFileName);
}

int CGdippSettings::_GetFreeTypeProfileIntFromSection(LPCTSTR lpszSection, LPCTSTR lpszKey, int nDefault, LPCTSTR lpszFile)
{
	wstring names = wstring((LPTSTR)lpszSection) + _T("@") + wstring((LPTSTR)m_szexeName);
	int retA = GetPrivateProfileInt(names.c_str(), lpszKey, -1, lpszFile);
	int retB = GetPrivateProfileInt(names.c_str(), lpszKey, -2, lpszFile);

	if (retA == retB) {
		return retA;
	}

	retA = GetPrivateProfileInt(lpszSection, lpszKey, -1, lpszFile);
	retB = GetPrivateProfileInt(lpszSection, lpszKey, -2, lpszFile);

	if (retA == retB) {
		return retA;
	}

	return nDefault;
}

int CGdippSettings::_GetFreeTypeProfileInt(LPCTSTR lpszKey, int nDefault, LPCTSTR lpszFile)
{
	int ret = _GetFreeTypeProfileIntFromSection(c_szFreeType, lpszKey, nDefault, lpszFile);
	if (ret == nDefault)
		return _GetFreeTypeProfileIntFromSection(c_szGeneral, lpszKey, nDefault, lpszFile);
	else
		return ret;
}

int CGdippSettings::_GetFreeTypeProfileBoundInt(LPCTSTR lpszKey, int nDefault, int nMin, int nMax, LPCTSTR lpszFile)
{
	const int ret = _GetFreeTypeProfileInt(lpszKey, nDefault, lpszFile);
	return Bound(ret, nMin, nMax);
}

bool CGdippSettings::_IsFreeTypeProfileSectionExists(LPCTSTR lpszKey, LPCTSTR lpszFile)
{
	TCHAR temp[4];
	return !!GetPrivateProfileSection(lpszKey, temp, 3, lpszFile) ;
}

float CGdippSettings::_GetFreeTypeProfileFloat(LPCTSTR lpszKey, float fDefault, LPCTSTR lpszFile)
{
	TCHAR TEMP[257];
	wstring names = wstring((LPTSTR)c_szFreeType) + _T("@") + wstring((LPTSTR)m_szexeName);
	int retA = GetPrivateProfileInt(names.c_str(), lpszKey, -1, lpszFile);
	int retB = GetPrivateProfileInt(names.c_str(), lpszKey, -2, lpszFile);
	if (retA == retB) {
		GetPrivateProfileString(names.c_str(), lpszKey, _T(""), TEMP, 256, lpszFile);
		return _StrToFloat(TEMP, fDefault);
	}
	names = wstring((LPTSTR)c_szGeneral) + _T("@") + wstring((LPTSTR)m_szexeName);
	retA = GetPrivateProfileInt(names.c_str(), lpszKey, -1, lpszFile);
	retB = GetPrivateProfileInt(names.c_str(), lpszKey, -2, lpszFile);
	if (retA == retB) {
		GetPrivateProfileString(names.c_str(), lpszKey, _T(""), TEMP, 256, lpszFile);
		return _StrToFloat(TEMP, fDefault);
	}

	retA = GetPrivateProfileInt(c_szFreeType, lpszKey, -1, lpszFile);
	retB = GetPrivateProfileInt(c_szFreeType, lpszKey, -2, lpszFile);

	if (retA == retB) {
		GetPrivateProfileString(c_szFreeType, lpszKey, _T(""), TEMP, 256, lpszFile);
		return _StrToFloat(TEMP, fDefault);
	}
		GetPrivateProfileString(c_szGeneral, lpszKey, _T(""), TEMP, 256, lpszFile);
		return _StrToFloat(TEMP, fDefault);
}

float CGdippSettings::_GetFreeTypeProfileBoundFloat(LPCTSTR lpszKey, float fDefault, float fMin, float fMax, LPCTSTR lpszFile)
{
	const float ret = _GetFreeTypeProfileFloat(lpszKey, fDefault, lpszFile);
	return Bound(ret, fMin, fMax);
}


DWORD CGdippSettings::_GetFreeTypeProfileString(LPCTSTR lpszKey, LPCTSTR lpszDefault, LPTSTR lpszRet, DWORD cch, LPCTSTR lpszFile)
{
	wstring names = wstring((LPTSTR)c_szFreeType) + _T("@") + wstring((LPTSTR)m_szexeName);
	int retA = GetPrivateProfileInt(names.c_str(), lpszKey, -1, lpszFile);
	int retB = GetPrivateProfileInt(names.c_str(), lpszKey, -2, lpszFile);

	if (retA == retB) {
		return GetPrivateProfileString(names.c_str(), lpszKey, lpszDefault, lpszRet, cch, lpszFile);
	}
	names = wstring((LPTSTR)c_szGeneral) + _T("@") + wstring((LPTSTR)m_szexeName);
	retA = GetPrivateProfileInt(names.c_str(), lpszKey, -1, lpszFile);
	retB = GetPrivateProfileInt(names.c_str(), lpszKey, -2, lpszFile);

	if (retA == retB) {
		return GetPrivateProfileString(names.c_str(), lpszKey, lpszDefault, lpszRet, cch, lpszFile);
	}

	retA = GetPrivateProfileInt(c_szFreeType, lpszKey, -1, lpszFile);
	retB = GetPrivateProfileInt(c_szFreeType, lpszKey, -2, lpszFile);

	if (retA == retB) {
		return GetPrivateProfileString(c_szFreeType, lpszKey, lpszDefault, lpszRet, cch, lpszFile);
	}
	return GetPrivateProfileString(c_szGeneral, lpszKey, lpszDefault, lpszRet, cch, lpszFile);
}

int CGdippSettings::_GetAlternativeProfileName(LPTSTR lpszName, LPCTSTR lpszFile)
{
	TCHAR szexe[MAX_PATH+1];
	TCHAR* pexe = szexe + GetModuleFileName(NULL, szexe, MAX_PATH);
	while (pexe>=szexe && *pexe!='\\')
		pexe--;
	pexe++;
	wstring exename = _T("General@")+ wstring((LPTSTR)pexe);
	if (GetPrivateProfileString(exename.c_str(), _T("Alternative"), NULL, lpszName, MAX_PATH, lpszFile))
	{
		return true;
	} 
	else
	{
		StringCchCopy(lpszName, MAX_PATH+1, pexe);
		return false;
	}
}

bool CGdippSettings::LoadAppSettings(LPCTSTR lpszFile)
{
	// 奺庬愝掕撉傒崬傒
	// INI僼傽僀儖偺椺:
	// [General]
	// HookChildProcesses=0
	// HintingMode=0
	// AntiAliasMode=0
	// NormalWeight=0
	// BoldWeight=0
	// ItalicSlant=0
	// EnableKerning=0
	// MaxHeight=0
	// ForceChangeFont=俵俽 俹僑僔僢僋
	// TextTuning=0
	// TextTuningR=0
	// TextTuningG=0
	// TextTuningB=0
	// CacheMaxFaces=0
	// CacheMaxSizes=0
	// CacheMaxBytes=0
	// AlternativeFile=
	// LoadOnDemand=0
	// UseMapping=0
	// LcdFilter=0
	// Shadow=1,1,4
	// [Individual]
	// 俵俽 俹僑僔僢僋=0,1,2,3,4,5

	WritePrivateProfileString(NULL, NULL, NULL, lpszFile);

	TCHAR szAlternative[MAX_PATH], szMainFile[MAX_PATH];
	if (GetPrivateProfileString(c_szGeneral, _T("AlternativeFile"), _T(""), szAlternative, MAX_PATH, lpszFile)) {
		if (PathIsRelative(szAlternative)) {
			TCHAR szDir[MAX_PATH];
			StringCchCopy(szDir, MAX_PATH, lpszFile);
			PathRemoveFileSpec(szDir);
			PathCombine(szAlternative, szDir, szAlternative);
		}
		StringCchCopy(szMainFile, MAX_PATH, lpszFile);	//把原始文件名保存下来
		StringCchCopy(m_szFileName, MAX_PATH, szAlternative);	
		lpszFile = m_szFileName;
		WritePrivateProfileString(NULL, NULL, NULL, lpszFile);
	}

	_GetAlternativeProfileName(m_szexeName, lpszFile);	//获得可能的独立配置名称
	CFontSettings& fs = m_FontSettings;
	fs.Clear();
	fs.SetHintingMode(_GetFreeTypeProfileBoundInt(_T("HintingMode"), 0, HINTING_MIN, HINTING_MAX, lpszFile));
	fs.SetAntiAliasMode(_GetFreeTypeProfileBoundInt(_T("AntiAliasMode"), 0, AAMODE_MIN, AAMODE_MAX, lpszFile));
	fs.SetNormalWeight(_GetFreeTypeProfileBoundInt(_T("NormalWeight"), 0, NWEIGHT_MIN, NWEIGHT_MAX, lpszFile));
	fs.SetBoldWeight(_GetFreeTypeProfileBoundInt(_T("BoldWeight"), 0, BWEIGHT_MIN, BWEIGHT_MAX, lpszFile));
	fs.SetItalicSlant(_GetFreeTypeProfileBoundInt(_T("ItalicSlant"), 0, SLANT_MIN, SLANT_MAX, lpszFile));
	fs.SetKerning(!!_GetFreeTypeProfileInt(_T("EnableKerning"), 0, lpszFile));

	{
		TCHAR szShadow[256];
		CStringTokenizer token;
		m_bEnableShadow = false;
		if (!_GetFreeTypeProfileString(_T("Shadow"), _T(""), szShadow, countof(szShadow), lpszFile)
				|| token.Parse(szShadow) < 3) {
			goto SKIP;
		}
		for (int i=0; i<3; i++) {
			m_nShadow[i] = _StrToInt(token.GetArgument(i), 0);
			/*if (m_nShadow[i] <= 0) {
				goto SKIP;
			}*/
		}
		m_bEnableShadow = true;
		if (token.GetCount()>=4)	//如果指定了浅色阴影
			m_nShadowDarkColor = _httoi(token.GetArgument(3));	//读取阴影
		else
			m_nShadowDarkColor = 0;	//否则为黑色
		if (token.GetCount()>=6)	//如果指定了深色阴影
		{
			m_nShadowLightColor = _httoi(token.GetArgument(5));	//读取阴影
			m_nShadow[3] = _StrToInt(token.GetArgument(4), m_nShadow[2]); //读取深度
		}
		else
		{
			m_nShadowLightColor = m_nShadowLightColor;		//否则和浅色阴影相同
			m_nShadow[3] = m_nShadow[2];		//深度也相同
		}
SKIP:
		;
	}

	m_bHookChildProcesses = !!GetPrivateProfileInt(c_szGeneral, _T("HookChildProcesses"), false, lpszFile);
	m_bUseMapping	= !!_GetFreeTypeProfileInt(_T("UseMapping"), false, lpszFile);
	m_nBolderMode	= _GetFreeTypeProfileInt(_T("BolderMode"), 0, lpszFile);
	m_nGammaMode	= _GetFreeTypeProfileInt(_T("GammaMode"), -1, lpszFile);
	m_fGammaValue	= _GetFreeTypeProfileBoundFloat(_T("GammaValue"), 1.0f, GAMMAVALUE_MIN, GAMMAVALUE_MAX, lpszFile);
	m_fRenderWeight	= _GetFreeTypeProfileBoundFloat(_T("RenderWeight"), 1.0f, RENDERWEIGHT_MIN, RENDERWEIGHT_MAX, lpszFile);
	m_fContrast		= _GetFreeTypeProfileBoundFloat(_T("Contrast"), 1.0f, CONTRAST_MIN, CONTRAST_MAX, lpszFile);
#ifdef _DEBUG
	// GammaValue専徹梡
	//CHAR GammaValueTest[1025];
	//sprintf(GammaValueTest, "GammaValue=%.6f\nContrast=%.6f\n", m_fGammaValue, m_fContrast);
	//MessageBoxA(NULL, GammaValueTest, "GammaValue僥僗僩", 0);
#endif
	m_bLoadOnDemand	= !!_GetFreeTypeProfileInt(_T("LoadOnDemand"), false, lpszFile);
	m_bFontLink		= _GetFreeTypeProfileInt(_T("FontLink"), 0, lpszFile);

	m_bIsInclude	= !!_GetFreeTypeProfileInt(_T("UseInclude"), false, lpszFile);
	m_nMaxHeight	= _GetFreeTypeProfileBoundInt(_T("MaxHeight"), 0, 0, 0xfff, lpszFile);	//最高只能到65535，cache的限制，而且大字体无实际价值
	m_nBitmapHeight = _GetFreeTypeProfileBoundInt(_T("MaxBitmap"), 0, 0, 255, lpszFile);
	m_bHintSmallFont = _GetFreeTypeProfileInt(_T("HintSmallFont"), 0, lpszFile);
	m_bDirectWrite = _GetFreeTypeProfileInt(_T("DirectWrite"), 0, lpszFile);
	m_nLcdFilter	= _GetFreeTypeProfileInt(_T("LcdFilter"), 0, lpszFile);
	m_nFontSubstitutes = _GetFreeTypeProfileBoundInt(_T("FontSubstitutes"),
													 SETTING_FONTSUBSTITUTE_DISABLE,
													 SETTING_FONTSUBSTITUTE_DISABLE,
													 SETTING_FONTSUBSTITUTE_ALL,
													 lpszFile);
	m_nWidthMode = SETTING_WIDTHMODE_GDI32;
/*
	_GetFreeTypeProfileBoundInt(_T("WidthMode"),
											   SETTING_WIDTHMODE_GDI32,
											   SETTING_WIDTHMODE_GDI32,
											   SETTING_WIDTHMODE_FREETYPE,
											   lpszFile);*/

	m_nFontLoader = _GetFreeTypeProfileBoundInt(_T("FontLoader"),
												SETTING_FONTLOADER_FREETYPE,
												SETTING_FONTLOADER_FREETYPE,
												SETTING_FONTLOADER_WIN32,
												lpszFile);
	m_nCacheMaxFaces = _GetFreeTypeProfileInt(_T("CacheMaxFaces"), 64, lpszFile);
	m_nCacheMaxFaces = m_nCacheMaxFaces > 64 ? m_nCacheMaxFaces : 64;
	m_nCacheMaxSizes = _GetFreeTypeProfileInt(_T("CacheMaxSizes"), 1200, lpszFile);
	m_nCacheMaxBytes = _GetFreeTypeProfileInt(_T("CacheMaxBytes"), 10485760, lpszFile);

	//experimental settings:
	m_bEnableClipBoxFix = !!_GetFreeTypeProfileIntFromSection(_T("Experimental"), _T("ClipBoxFix"), 1, lpszFile);

	if (m_nFontLoader == SETTING_FONTLOADER_WIN32) {
		// API偑張棟偟偰偔傟傞偼偢側偺偱帺慜張棟偼柍岠壔
		if (m_nFontSubstitutes == SETTING_FONTSUBSTITUTE_ALL) {
			m_nFontSubstitutes = SETTING_FONTSUBSTITUTE_DISABLE;
		}
		m_bFontLink = 0;
	}

	// 僼僅儞僩巜掕
	ZeroMemory(&m_lfForceFont, sizeof(LOGFONT));
	m_szForceChangeFont[0] = _T('\0');
	_GetFreeTypeProfileString(_T("ForceChangeFont"), _T(""), m_szForceChangeFont, LF_FACESIZE, lpszFile);

	const int nTextTuning	= _GetFreeTypeProfileInt(_T("TextTuning"),  0, lpszFile),
			  nTextTuningR	= _GetFreeTypeProfileInt(_T("TextTuningR"), 0, lpszFile),
			  nTextTuningG	= _GetFreeTypeProfileInt(_T("TextTuningG"), 0, lpszFile),
			  nTextTuningB	= _GetFreeTypeProfileInt(_T("TextTuningB"), 0, lpszFile);

	InitInitTuneTable();
	InitTuneTable(nTextTuning,  m_nTuneTable);
	InitTuneTable(nTextTuningR, m_nTuneTableR);
	InitTuneTable(nTextTuningG, m_nTuneTableG);
	InitTuneTable(nTextTuningB, m_nTuneTableB);

	// OS偺僶乕僕儑儞偑XP埲崀偐偳偆偐
	//OSVERSIONINFO osvi = { sizeof(OSVERSIONINFO) };
	//GetVersionEx(&osvi);
	m_bIsWinXPorLater = IsWindowsXPOrGreater(); 

	STARTUPINFO si = { sizeof(STARTUPINFO) };
	GetStartupInfo(&si);
	m_bRunFromGdiExe = IsGdiPPStartupInfo(si);
//	if (!m_bRunFromGdiExe) {
//		m_bHookChildProcesses = false;
//	}

//	m_bIsHDBench = (GetModuleHandle(_T("HDBENCH.EXE")) == GetModuleHandle(NULL));

	m_arrExcludeFont.clear();
	m_arrExcludeModule.clear();
	m_arrIncludeModule.clear();
	m_arrUnloadModule.clear();
	m_arrUnFontSubModule.clear();

	// [Exclude]僙僋僔儑儞偐傜彍奜僼僅儞僩儕僗僩傪撉傒崬傓
	AddExcludeListFromSection(_T("Exclude"), lpszFile, m_arrExcludeFont);
	AddExcludeListFromSection(_T("Exclude"), szMainFile, m_arrExcludeFont);
	// [ExcludeModule]僙僋僔儑儞偐傜彍奜儌僕儏乕儖儕僗僩傪撉傒崬傓
	AddListFromSection(_T("ExcludeModule"), lpszFile, m_arrExcludeModule);
	AddListFromSection(_T("ExcludeModule"), szMainFile, m_arrExcludeModule);
	// [IncludeModule]僙僋僔儑儞偐傜懳徾儌僕儏乕儖儕僗僩傪撉傒崬傓
	AddListFromSection(_T("IncludeModule"), lpszFile, m_arrIncludeModule);
	AddListFromSection(_T("IncludeModule"), szMainFile, m_arrIncludeModule);
	// [UnloadDLL]完全不加载的模块
	AddListFromSection(_T("UnloadDLL"), lpszFile, m_arrUnloadModule);
	AddListFromSection(_T("UnloadDLL"), szMainFile, m_arrUnloadModule);
	// [ExcludeSub]不进行字体替换的模块
	AddListFromSection(L"ExcludeSub", lpszFile, m_arrUnFontSubModule);
	AddListFromSection(L"ExcludeSub", szMainFile, m_arrUnFontSubModule);
	//如果是排除的模块，则关闭字体替换
	if (m_nFontSubstitutes)
	{
		ModuleHashMap::const_iterator it=m_arrUnFontSubModule.begin();
		while (it!=m_arrUnFontSubModule.end())
		{
			if (GetModuleHandle(it->c_str()))
			{
				m_nFontSubstitutes = 0;	//关闭替换
				break;
			}
			++it;
		}
	}

	// [Individual]僙僋僔儑儞偐傜僼僅儞僩暿愝掕傪撉傒崬傓
	wstring names = _T("Individual@") + wstring(m_szexeName);
	if (_IsFreeTypeProfileSectionExists(names.c_str(), lpszFile))
		AddIndividualFromSection(names.c_str(), lpszFile, m_arrIndividual);
	else
		AddIndividualFromSection(_T("Individual"), lpszFile, m_arrIndividual);
	names = _T("LcdFilterWeight@") + wstring(m_szexeName);
	if (_IsFreeTypeProfileSectionExists(names.c_str(), lpszFile))
		m_bUseCustomLcdFilter = AddLcdFilterFromSection(names.c_str(), lpszFile, m_arrLcdFilterWeights);
	else
		m_bUseCustomLcdFilter = AddLcdFilterFromSection(_T("LcdFilterWeight"), lpszFile, m_arrLcdFilterWeights);

	WritePrivateProfileString(NULL, NULL, NULL, lpszFile);
	return true;
}

int CALLBACK CGdippSettings::EnumFontFamProc(const LOGFONT* lplf, const TEXTMETRIC* /*lptm*/, DWORD FontType, LPARAM lParam)
{
	CGdippSettings* pThis = reinterpret_cast<CGdippSettings*>(lParam);
	if (pThis && FontType == TRUETYPE_FONTTYPE)
		pThis->m_lfForceFont = *lplf;
	return 0;
}

bool CGdippSettings::AddExcludeListFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, set<wstring> & arr)
{
	LPTSTR  buffer = _GetPrivateProfileSection(lpszSection, lpszFile);
	if (buffer == NULL) {
		SetLastError(ERROR_NOT_ENOUGH_MEMORY);
		return false;
	}

	LPTSTR p = buffer;
	TCHAR buff[LF_FACESIZE+1];
	LOGFONT truefont={0};
	while (*p) {
		bool b = false;
		GetFontLocalName(p, buff);//转换字体名
		set<wstring>::const_iterator it = arr.find(buff);
		if (it==arr.end())
			arr.insert(buff);
		for (; *p; p++);	//来到下一行
		p++;
	}
	delete[] buffer;
	return false;
}

//template <typename T>
bool CGdippSettings::AddListFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, set<wstring> & arr)
{
	LPTSTR  buffer = _GetPrivateProfileSection(lpszSection, lpszFile);
	if (buffer == NULL) {
		SetLastError(ERROR_NOT_ENOUGH_MEMORY);
		return false;
	}

	LPTSTR p = buffer;
	while (*p) {
		bool b = false;
		set<wstring>::const_iterator it = arr.find(p);
		if (it==arr.end())
			arr.insert(p);
		for (; *p; p++);	//来到下一行
			p++;
	}
	delete[] buffer;
	return false;
}

bool CGdippSettings::AddLcdFilterFromSection(LPCTSTR lpszKey, LPCTSTR lpszFile, unsigned char* arr)
{
	TCHAR buffer[100];
	_GetFreeTypeProfileString(lpszKey, _T("\0"), buffer, sizeof(buffer), lpszFile);
	if (buffer[0] == '\0') {
		SetLastError(ERROR_NOT_ENOUGH_MEMORY);
		return false;
	}

	LPTSTR p = buffer;
	CStringTokenizer token;
	int argc = 0;
	argc = token.Parse(buffer);

	for (int i = 0; i < 5; i++) {
		LPCTSTR arg = token.GetArgument(i);
		if (!arg)
			return false;	//参数少于5个则视为不使用此参数
		arr[i] = _StrToInt(arg, arr[i]);
	}

	return true;
}

bool CGdippSettings::AddIndividualFromSection(LPCTSTR lpszSection, LPCTSTR lpszFile, IndividualArray& arr)
{
	LPTSTR  buffer = _GetPrivateProfileSection(lpszSection, lpszFile);
	if (buffer == NULL) {
		SetLastError(ERROR_NOT_ENOUGH_MEMORY);
		return false;
	}

	LPTSTR p = buffer;
	TCHAR buff[LF_FACESIZE+1];
	LOGFONT truefont={0};
	while (*p) {
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

		GetFontLocalName(p, buff);//转换字体名

		CFontIndividual fi(buff);
		const CFontSettings& fsCommon = m_FontSettings;
		CFontSettings& fs = fi.GetIndividual();
		//Individual偑柍偗傟偽嫟捠愝掕傪巊偆
		fs = fsCommon;
		for (int i = 0; i < MAX_FONT_SETTINGS; i++) {
			LPCTSTR arg = token.GetArgument(i);
			if (!arg)
				break;
			const int n = _StrToInt(arg, fsCommon.GetParam(i));
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
#ifdef _DEBUG
			TRACE(_T("Individual: %s, %d, %d, %d, %d, %d, %d\n"), fi.GetName(),
					fs.GetParam(0), fs.GetParam(1), fs.GetParam(2), fs.GetParam(3), fs.GetParam(4), fs.GetParam(5));
#endif
		}
		p = pnext;
		p++;
	}
	delete[] buffer;
	return false;
}

LPTSTR CGdippSettings::_GetPrivateProfileSection(LPCTSTR lpszSection, LPCTSTR lpszFile)
{
	LPTSTR buffer = NULL;
	int nRes = 0;
	for (int cch = 256; ; cch += 256) {
		buffer = new TCHAR[cch];
		if (!buffer) {
			break;
		}

		ZeroMemory(buffer, sizeof(TCHAR) * cch);
		nRes = GetPrivateProfileSection(lpszSection, buffer, cch, lpszFile);
		if (nRes < cch - 2) {
			break;
		}
		delete[] buffer;
		buffer = NULL;
	}
	return buffer;
}

//atol偵僨僼僅儖僩抣傪曉偣傞傛偆偵偟偨傛偆側暔
int CGdippSettings::_StrToInt(LPCTSTR pStr, int nDefault)
{
#define isspace(ch)		(ch == _T('\t') || ch == _T(' '))
#define isdigit(ch)		((_TUCHAR)(ch - _T('0')) <= 9)

	int ret;
	bool neg = false;
	LPCTSTR pStart;

	for (; isspace(*pStr); pStr++);
	switch (*pStr) {
	case _T('-'):
		neg = true;
	case _T('+'):
		pStr++;
		break;
	}

	pStart = pStr;
	ret = 0;
	for (; isdigit(*pStr); pStr++) {
		ret = 10 * ret + (*pStr - _T('0'));
	}

	if (pStr == pStart) {
		return nDefault;
	}
	return neg ? -ret : ret;

#undef isspace
#undef isdigit
}

int CGdippSettings::_httoi(const TCHAR *value)
{
	struct CHexMap
	{
		TCHAR chr;
		int value;
	};
	const int HexMapL = 16;
	CHexMap HexMap[HexMapL] =
	{
		{'0', 0}, {'1', 1},
		{'2', 2}, {'3', 3},
		{'4', 4}, {'5', 5},
		{'6', 6}, {'7', 7},
		{'8', 8}, {'9', 9},
		{'A', 10}, {'B', 11},
		{'C', 12}, {'D', 13},
		{'E', 14}, {'F', 15}
	};
	TCHAR *mstr = _tcsupr(_tcsdup(value));
	TCHAR *s = mstr;
	int result = 0;
	if (*s == '0' && *(s + 1) == 'X') s += 2;
	bool firsttime = true;
	while (*s != '\0')
	{
		bool found = false;
		for (int i = 0; i < HexMapL; i++)
		{
			if (*s == HexMap[i].chr)
			{
				if (!firsttime) result <<= 4;
				result |= HexMap[i].value;
				found = true;
				break;
			}
		}
		if (!found) break;
		s++;
		firsttime = false;
	}
	free(mstr);
	return result;
}

//atof偵僨僼僅儖僩抣傪曉偣傞傛偆偵偟偨傛偆側暔
float CGdippSettings::_StrToFloat(LPCTSTR pStr, float fDefault)
{
#define isspace(ch)		(ch == _T('\t') || ch == _T(' '))
#define isdigit(ch)		((_TUCHAR)(ch - _T('0')) <= 9)

	int ret_i;
	int ret_d;
	float ret;
	bool neg = false;
	LPCTSTR pStart;

	for (; isspace(*pStr); pStr++);
	switch (*pStr) {
	case _T('-'):
		neg = true;
	case _T('+'):
		pStr++;
		break;
	}

	pStart = pStr;
	ret = 0;
	ret_i = 0;
	ret_d = 1;
	for (; isdigit(*pStr); pStr++) {
		ret_i = 10 * ret_i + (*pStr - _T('0'));
	}
	if (*pStr == _T('.')) {
		pStr++;
		for (; isdigit(*pStr); pStr++) {
			ret_i = 10 * ret_i + (*pStr - _T('0'));
			ret_d *= 10;
		}
	}
	ret = (float)ret_i / (float)ret_d;

	if (pStr == pStart) {
		return fDefault;
	}
	return neg ? -ret : ret;

#undef isspace
#undef isdigit
}

bool CGdippSettings::IsFontExcluded(LPCSTR lpFaceName) const
{
	WCHAR szStack[LF_FACESIZE];
	LPWSTR lpUnicode = _StrDupExAtoW(lpFaceName, -1, szStack, LF_FACESIZE, NULL);
	if (!lpUnicode) {
		return false;
	}

	bool b = IsFontExcluded(lpUnicode);
	if (lpUnicode != szStack)
		free(lpUnicode);
	return b;
}

bool CGdippSettings::IsFontExcluded(LPCWSTR lpFaceName) const
{
	FontHashMap::const_iterator it=m_arrExcludeFont.find(lpFaceName);
	return it!=m_arrExcludeFont.end();
}

void CGdippSettings::AddFontExclude(LPCWSTR lpFaceName)
{
	if (!IsFontExcluded(lpFaceName))
		m_arrExcludeFont.insert(lpFaceName);
}

bool CGdippSettings::IsProcessUnload() const
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrUnloadModule.begin();
	for(; it != m_arrUnloadModule.end(); ++it) {
		if (GetModuleHandleW(it->c_str())) {
			return true;
		}
	}
	return false;
}

bool CGdippSettings::IsExeUnload(LPCTSTR lpApp) const	//检查是否在黑名单列表内
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrUnloadModule.begin();
	for(; it != m_arrUnloadModule.end(); ++it) {
		if (!lstrcmpi(lpApp, it->c_str())) {	//匹配排除项
			return true;
		}
	}
	return false;
}

bool CGdippSettings::IsExeInclude(LPCTSTR lpApp) const	//检查是否在白名单列表内
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrIncludeModule.begin();
	for(; it != m_arrIncludeModule.end(); ++it) {
		if (!lstrcmpi(lpApp, it->c_str())) {	//匹配排除项
			return true;
		}
	}
	return false;
}

bool CGdippSettings::IsProcessExcluded() const
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_EXCLUDE", NULL, 0);
	if (GetLastError() != ERROR_ENVVAR_NOT_FOUND)
		return true;
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrExcludeModule.begin();
	for(; it != m_arrExcludeModule.end(); ++it) {
		if (GetModuleHandleW(it->c_str())) {
			return true;
		}
	}
	return false;
}

bool CGdippSettings::IsProcessIncluded() const
{
	if (m_bRunFromGdiExe) {
		return true;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return true;
	ModuleHashMap::const_iterator it = m_arrIncludeModule.begin();
	for(; it != m_arrIncludeModule.end(); ++it) {
		if (GetModuleHandleW(it->c_str())) {
			return true;
		}
	}
	return false;
}

void CGdippSettings::InitInitTuneTable()
{
	int i, *table;
#define init_table(name) \
		for (i=0,table=name; i<256; i++) table[i] = i
	init_table(m_nTuneTable);
	init_table(m_nTuneTableR);
	init_table(m_nTuneTableG);
	init_table(m_nTuneTableB);
#undef init_table
}

// 僥乕僽儖弶婜壔娭悢 0 - 12傑偱
// LCD梡僥乕僽儖弶婜壔娭悢 奺0 - 12傑偱
void CGdippSettings::InitTuneTable(int v, int* table)
{
	int i;
	int col;
	double tmp, p;

	if (v < 0) {
		return;
	}
	v = Min(v, 12);
	p = (double)v;
	p = 1 - (p / (p + 10.0));
	for(i = 0;i < 256;i++){
	    tmp = (double)i / 255.0;
        tmp = pow(tmp, p);
	    col = 255 - (int)(tmp * 255.0 + 0.5);
		table[255 - i] = col;
	}
}

//尒偮偐傜側偄応崌偼嫟捠愝掕傪曉偡
extern BOOL g_ccbIndividual;
const CFontSettings& CGdippSettings::FindIndividual(LPCTSTR lpFaceName) const
{
	CFontIndividual* p		= m_arrIndividual.Begin();
	CFontIndividual* end	= m_arrIndividual.End();
	if (lpFaceName && *lpFaceName==L'@')
		++lpFaceName;	//纵向字体使用横向的设定
	StringHashFont hash(lpFaceName);

	for(; p != end; ++p) {
		if (p->GetHash() == hash) {
			return p->GetIndividual();
		}
	}
	return GetFontSettings();
}

bool CGdippSettings::CopyForceFont(LOGFONT& lf, const LOGFONT& lfOrg) const
{
	_ASSERTE(m_bDelayedInit);
	//__asm{ int 3 }
	GetEnvironmentVariableW(L"MACTYPE_FONTSUBSTITUTES_ENV", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	//&lf == &lfOrg傕壜
	bool bForceFont = !!GetForceFontName();
	BOOL bFontExist = true;
	const LOGFONT *lplf;
	if (bForceFont) {
		lplf = &m_lfForceFont;
	} else {
		lplf = GetFontSubstitutesInfo().lookup((LOGFONT&)lfOrg);
		if (lplf) bForceFont = true;
	}
	if (bForceFont) {
		memcpy(&lf, &lfOrg, sizeof(LOGFONT)-sizeof(lf.lfFaceName));
		StringCchCopy(lf.lfFaceName, LF_FACESIZE, lplf->lfFaceName);
	}
	return bForceFont;
}

//抣揑偵char(-128乣127)偱廫暘
const char CFontSettings::m_bound[MAX_FONT_SETTINGS][2] = {
	{ HINTING_MIN,	HINTING_MAX	},	//Hinting
	{ AAMODE_MIN,	AAMODE_MAX	},	//AAMode
	{ NWEIGHT_MIN,	NWEIGHT_MAX	},	//NormalWeight
	{ BWEIGHT_MIN,	BWEIGHT_MAX	},	//BoldWeight
	{ SLANT_MIN,	SLANT_MAX	},	//ItalicSlant
	{ 0,			1			},	//Kerning
};

CFontLinkInfo::CFontLinkInfo()
{
	memset(&info, 0, sizeof info);
	memset(AllowDefaultLink, 1, sizeof(AllowDefaultLink));	//默认允许字体链接
}

CFontLinkInfo::~CFontLinkInfo()
{
	clear();
}

/*
static int CALLBACK EnumFontCallBack(const LOGFONT *lplf, const TEXTMETRIC *lptm, DWORD / *FontType* /, LPARAM lParam)
{	
	LOGFONT * lf=(LOGFONT *)lParam;
	StringCchCopy(lf->lfFaceName, LF_FACESIZE, lplf->lfFaceName);
	return 0;
}

static void GetFontLocalName(LOGFONT& lf)	//获得字体的本地化名称
{
	HDC dc=GetDC(NULL);
	EnumFontFamiliesEx(dc, &lf, &EnumFontCallBack, (LPARAM)&lf, 0);
	ReleaseDC(NULL, dc);
}*/


void CFontLinkInfo::init()
{
	const TCHAR REGKEY1[] = _T("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\FontLink\\SystemLink");
	const TCHAR REGKEY2[] = _T("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts");
	const TCHAR REGKEY3[] = _T("SYSTEM\\CurrentControlSet\\Control\\FontAssoc\\Associated DefaultFonts");
	const TCHAR REGKEY4[] = _T("SYSTEM\\CurrentControlSet\\Control\\FontAssoc\\Associated Charset");

	HKEY h1;
	HKEY h2;
	if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY1, 0, KEY_QUERY_VALUE, &h1)) return;
	if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY2, 0, KEY_QUERY_VALUE, &h2)) {
		RegCloseKey(h1);
	}
	//OSVERSIONINFO sOsVinfo={sizeof(OSVERSIONINFO),0,0,0,0,{0}};
	//GetVersionEx(&sOsVinfo);	//获得操作系统版本号
	//const CGdippSettings* pSettings = CGdippSettings::GetInstance();

	WCHAR name[0x2000];
	DWORD namesz;
	DWORD valuesz;
	WCHAR value[0x2000];
	WCHAR buf[0x2000];
	LONG rc;
	DWORD regtype;

	for (int k = 0; ; ++k) {	//获得字体表中的所有字体
		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h2, k, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);		//从字体表中寻找
		if (rc == ERROR_NO_MORE_ITEMS) break;
		if (rc != ERROR_SUCCESS) break;
		if (regtype != REG_SZ) continue;
		StringCchCopy(buf, sizeof(buf)/sizeof(buf[0]), name);
		if (buf[wcslen(buf) - 1] == L')') {				//去掉括号
			LPWSTR p;
			if ((p = wcsrchr(buf, L'(')) != NULL) {
				*p = 0;
			}
		}
		while (buf[wcslen(buf)-1] == L' ')
			buf[wcslen(buf)-1] = 0;
		//获得的对应的字体名
		FontNameCache.Add(value, buf);
	}

	int row = 0;
	LOGFONT truefont;
	memset(&truefont, 0, sizeof(truefont));
	for (int i = 0; row < INFOMAX; ++i) {
		int col = 0;

		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h1, i, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);	//获得一个字体的字体链接
		if (rc == ERROR_NO_MORE_ITEMS) break;
		if (rc != ERROR_SUCCESS) break;
		if (regtype != REG_MULTI_SZ) continue;		//有效的字体链接
		//获得字体的真实名字
		
		TCHAR buff[LF_FACESIZE];
		GetFontLocalName(name, buff);

		info[row][col] = _wcsdup(buff);		//第一项为字体名
		++col;

		for (LPCWSTR linep = value; col < FONTMAX && *linep; linep += wcslen(linep) + 1) {
			LPCWSTR valp = NULL;
			for (LPCWSTR p = linep; *p; ++p) {
				if (*p == L',' && ((char)*(p+1)<0x30 || (char)*(p+1)>0x39))		//尝试寻找字体链接中“，”后提供的字体名称
					{
						LPWSTR lp;
						StringCchCopy(buf, sizeof(buf)/sizeof(buf[0]), p+1);
						if (lp=wcschr(buf, L','))
							*lp = 0;
						valp = buf;
						break;
					}
			}
			if (!valp) {		//没找到字体链接中提供的名称
				/*for (int k = 0; ; ++k) {
					namesz = sizeof name;
					value2sz = sizeof value2;
					rc = RegEnumValue(h2, k, name, &namesz, 0, &regtype, (LPBYTE)value2, &value2sz);		//从字体表中寻找
					if (rc == ERROR_NO_MORE_ITEMS) break;
					if (rc != ERROR_SUCCESS) break;
					if (regtype != REG_SZ) continue;
					if (lstrcmpi(value2, linep) != 0) continue;		//寻找字体链接中字体文件对应的字体名

					StringCchCopyW(buf, sizeof(buf)/sizeof(buf[0]), name);
					if (buf[wcslen(buf) - 1] == L')') {				//去掉括号
						LPWSTR p;
						if ((p = wcsrchr(buf, L'(')) != NULL) {
							*p = 0;
						}
					}
					while (buf[wcslen(buf)-1] == L' ')
						buf[wcslen(buf)-1] = 0;
					valp = buf;
					break;
				}*/
				LPWSTR lp;
				StringCchCopy(buf, sizeof(buf)/sizeof(buf[0]), linep);
				if (lp=wcschr(buf, L','))
					*lp = 0;

				valp = FontNameCache.Find((TCHAR*)buf);
			}
			if (valp) {
				GetFontLocalName((TCHAR*)valp, buff);;
				//StringCchCopy(truefont.lfFaceName, LF_FACESIZE, buff);	//复制到结构中
				//pSettings->CopyForceFont(truefont, truefont);		//获得替换字体
				info[row][col] = _wcsdup(buff);//truefont.lfFaceName);			//复制到链接表中
				++col;
			}
		}
		if (col == 1) {			//只有一项，即没有链接，删掉。
			free(info[row][0]);
			info[row][0] = NULL;
		} else {
			/*if (sOsVinfo.dwMajorVersion>=6 && sOsVinfo.dwMinorVersion>=1)	//版本号>=6.1，是Win7系列
			{
				//对字体链接表做逆向处理
				LPWSTR swapbuff[32];
				memcpy(swapbuff, info[row], 32*sizeof(LPWSTR));	//整个表复制过来
				for (int i=1; i<col; i++)
					info[row][i]=swapbuff[col-i];	//逆序字体链接表
			}*/
			++row;
		}
	}
	RegCloseKey(h1);
	RegCloseKey(h2);
	LOGFONT syslf = {0};
	HGDIOBJ h = ORIG_GetStockObject(DEFAULT_GUI_FONT);
	ORIG_GetObjectW(h, sizeof syslf, &syslf);
	GetFontLocalName(syslf.lfFaceName, syslf.lfFaceName);

	extern HFONT g_alterGUIFont;
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	if (pSettings->FontSubstitutes()>=SETTING_FONTSUBSTITUTE_INIONLY && pSettings->CopyForceFont(truefont, syslf))	//使用完全替换模式时，替换掉系统字体
	{
		WCHAR envname[30];
		WCHAR envvalue[30];
		HFONT tempfont;
		if (GetEnvironmentVariable(L"MT_SYSFONT", envvalue, 29) && GetObjectType(tempfont = (HFONT)wcstoull(envvalue, 0 ,10)) == OBJ_FONT)//已经有字体存在
		{
			g_alterGUIFont = tempfont;	//直接使用先前的字体
		}
		else
		{
			g_alterGUIFont = CreateFontIndirectW(&truefont);	//创建一个新的替换字体
			_ui64tow((ULONG_PTR)g_alterGUIFont, envvalue, 10);	//转换为字符串
			SetEnvironmentVariable(envname, envvalue);		//写入环境变量
		}
	}

	//现在获取对应字体类型的默认字体链接
	memset(DefaultFontLink, 0, sizeof(TCHAR)*(FF_DECORATIVE+1)*(LF_FACESIZE+1));	//初始化为0
	HKEY h3;
	DWORD len;
	if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY3, 0, KEY_QUERY_VALUE, &h3)) return;
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackage"), 0, &regtype, (LPBYTE)DefaultFontLink[1], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageDecorative"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_DECORATIVE], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageDontCare"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_DONTCARE], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageModern"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_MODERN], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageRoman"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_ROMAN], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageScript"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_SCRIPT], &len);
	len = (LF_FACESIZE+1)*sizeof(TCHAR);
	RegQueryValueEx(h3, _T("FontPackageSwiss"), 0, &regtype, (LPBYTE)DefaultFontLink[FF_SWISS], &len);
	RegCloseKey(h3);
	
	for (int i=0; i<FF_DECORATIVE+1; ++i)	//转换字体名称
	{
		if (DefaultFontLink[i])
			GetFontLocalName(DefaultFontLink[i], DefaultFontLink[i]);
	}

	//现在获取对应的CodePage是否需要进行fontlink。默认都需要进行链接。
	HKEY h4;
	if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY4, 0, KEY_QUERY_VALUE, &h4)) return;

	for (int i=0; i<0xff; ++i)
	{
		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h4, i, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);	//获得一个charset的值
		if (rc == ERROR_NO_MORE_ITEMS) break;
		if (rc != ERROR_SUCCESS) break;
		if (regtype != REG_SZ) continue;
		if (_tcsicmp(value, _T("YES")))
		{
			TCHAR* p = name;
			while (*p!=_T('(') && p-name<(int)namesz) ++p;
			++p;
			AllowDefaultLink[_tcstol(p,NULL,16)]=false;
		}
	}
	RegCloseKey(h4);
}

void CFontLinkInfo::clear()
{
	for (int i = 0; i < INFOMAX; ++i) {
		for (int j = 0; j < FONTMAX; ++j) {
			free(info[i][j]);
			info[i][j] = NULL;
		}
	}
}

const LPCWSTR * CFontLinkInfo::lookup(LPCWSTR fontname) const
{
	for (int i = 0; i < INFOMAX && info[i][0]; ++i) {
		if (_wcsicmp(fontname, info[i][0]) == 0) {
			return &info[i][1];
		}
	}
	return NULL;
}

LPCWSTR CFontLinkInfo::get(int row, int col) const
{
	if ((unsigned int)row >= (unsigned int)INFOMAX || (unsigned int)col >= (unsigned int)FONTMAX) {
		return NULL;
	}
	return info[row][col];
}

CFontSubstituteData::CFontSubstituteData()
{
	memset(this, 0, sizeof *this);
}

int CALLBACK
CFontSubstituteData::EnumFontFamProc(const LOGFONT *lplf, const TEXTMETRIC *lptm, DWORD /*FontType*/, LPARAM lParam)
{
	CFontSubstituteData& self = *(CFontSubstituteData *)lParam;
	self.m_lf = *lplf;
	self.m_tm = *lptm;
	return 0;
}

bool
CFontSubstituteData::init(LPCTSTR config)
{
	memset(this, 0, sizeof *this);

	TCHAR buf[LF_FACESIZE + 20];
	StringCchCopy(buf, countof(buf), config);

	LOGFONT lf;
	memset(&lf, 0, sizeof lf);

	LPTSTR p;
	for (p = buf + lstrlen(buf) - 1; p >= buf; --p ) {
		if (*p == _T(',')) {
			*p++ = 0;
			break;
		}
	}
	if (p >= buf) {
		StringCchCopy(lf.lfFaceName, countof(lf.lfFaceName), buf);
		lf.lfCharSet = (BYTE)CGdippSettings::_StrToInt(p + 1, 0);
		m_bCharSet = true;
	} else {
		StringCchCopy(lf.lfFaceName, LF_FACESIZE, buf);
		lf.lfCharSet = DEFAULT_CHARSET;
		m_bCharSet = false;
	}

	HDC hdc = GetDC(NULL);
	EnumFontFamiliesEx(hdc, &lf, &CFontSubstituteData::EnumFontFamProc, (LPARAM)this, 0);
	ReleaseDC(NULL, hdc);

	return m_lf.lfFaceName[0] != 0;
}

bool
CFontSubstituteData::operator == (const CFontSubstituteData& o) const
{
	if (m_bCharSet != o.m_bCharSet) return false;
	if (m_bCharSet) {
		if (m_lf.lfCharSet != o.m_lf.lfCharSet) return false;
	}
	if (_wcsicmp(m_lf.lfFaceName, o.m_lf.lfFaceName) == 0) return true;
	return false;
}


void
CFontSubstitutesInfo::initreg()
{
	/*
	const LPCTSTR REGKEY = _T("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\FontSubstitutes");
		HKEY h;
		if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY, 0, KEY_QUERY_VALUE, &h)) return;
	
		for (int i = 0; ; ++i) {
			WCHAR name[0x2000];
			DWORD namesz;
			WCHAR value[0x2000];
			DWORD valuesz;
			namesz = sizeof name;
			valuesz = sizeof value;
			DWORD regtype;
	
			LONG rc = RegEnumValue(h, i, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);
			if (rc == ERROR_NO_MORE_ITEMS) break;
			if (rc != ERROR_SUCCESS) break;
			if (regtype != REG_SZ) continue;
	
			CFontSubstituteData k;
			CFontSubstituteData v;
			if (k.init(name) && v.init(value)) {
				if (FindKey(k) < 0 && k.m_bCharSet == v.m_bCharSet) Add(k, v);
			}
		}
	
		RegCloseKey(h);*/
	
}

void
CFontSubstitutesInfo::initini(const CFontSubstitutesIniArray& iniarray)
{
	CFontSubstitutesIniArray::const_iterator it=iniarray.begin();
//	LOGFONT truefont={0}, truefont2={0};
//	TCHAR* buff, *buff2;
	for (; it!=iniarray.end(); ++it) {
		LPCTSTR inistr = it->c_str();
		LPTSTR buf = _tcsdup(inistr);
		for (LPTSTR vp = buf; *vp; ++vp) {
			if (*vp == _T('=')) {
				*vp++ = 0;
				CFontSubstituteData k;
				CFontSubstituteData v;
				if (k.init(buf) && v.init(vp)) {
				if (FindKey(k) < 0 && k.m_bCharSet == v.m_bCharSet) Add(k, v);
				}
/*					StringCchCopy(truefont.lfFaceName, LF_FACESIZE, buf);
					truefont.lfCharSet=DEFAULT_CHARSET;
					if (!GetFontLocalName(truefont))
						continue;	//没有此字体
					buff = truefont.lfFaceName;

					StringCchCopy(truefont2.lfFaceName, LF_FACESIZE, vp);
					truefont2.lfCharSet=DEFAULT_CHARSET;
					if (!GetFontLocalName(truefont2))
						continue;	//没有此字体
					buff2 = truefont2.lfFaceName;

				if (m_mfontsub.find(buff)==m_mfontsub.end())
					m_mfontsub[buff]=wstring(buff2);
*/
			}
		}

		free(buf);
	}
}

void
CFontSubstitutesInfo::init(int nFontSubstitutes, const CFontSubstitutesIniArray& iniarray)
{
	if (nFontSubstitutes >= SETTING_FONTSUBSTITUTE_INIONLY) initini(iniarray);
	//if (nFontSubstitutes >= SETTING_FONTSUBSTITUTE_ALL) initreg();
}

void GetMacTypeInternalFontName(LOGFONT* lf, LPTSTR fn)
{
	StringCchCopy(fn, 9, _T("MACTYPE_"));
	fn+=8;
	TCHAR* lfbyte=(TCHAR*)lf;
	for (int i=0;i<(sizeof(LOGFONT)-sizeof(TCHAR)*LF_FACESIZE+1)/sizeof(TCHAR);i++)
		*fn++=_T('0') + *lfbyte++;
	*fn=0;
}

const LOGFONT *
CFontSubstitutesInfo::lookup(LOGFONT& lf) const
{
	if (GetSize() <= 0) return false;
	//bFontExist = true;
	CFontSubstituteData v;
	CFontSubstituteData k;

	k.m_bCharSet = true;
	k.m_lf = lf;

	TCHAR * buff;	//快速获得字体的真实名称
	LOGFONT mylf(lf);
	if (!(buff = FontNameCache.Find((TCHAR*)lf.lfFaceName)))
	{
		TCHAR localname[LF_FACESIZE+1];
		if (GetFontLocalName(mylf.lfFaceName, localname))
			FontNameCache.Add((TCHAR*)lf.lfFaceName, localname);
		else
		{
			TCHAR inName[LF_FACESIZE];
			GetMacTypeInternalFontName(&mylf, inName);
			if (!(buff = FontNameCache.Find(inName)))
			{
				mylf.lfClipPrecision = FONT_MAGIC_NUMBER;
				HFONT tempfont = CreateFontIndirect(&mylf);
				HDC dc=CreateCompatibleDC(NULL);
				HFONT oldfont = SelectFont(dc, tempfont);
				ORIG_GetTextFaceW(dc, LF_FACESIZE, mylf.lfFaceName);
				SelectFont(dc, oldfont);
				DeleteFont(tempfont);
				DeleteDC(dc);
				FontNameCache.Add(inName, mylf.lfFaceName);
			}
		}
		buff = mylf.lfFaceName;
	}
	StringCchCopy(k.m_lf.lfFaceName, LF_FACESIZE, buff);
	StringCchCopy(lf.lfFaceName, LF_FACESIZE, buff);

	int pos = FindKey(k);
	if (pos < 0) {
		k.m_bCharSet = false;
		pos = FindKey(k);
	}
	if (pos >= 0) {
		return (const LOGFONT *)&GetValueAt(pos);
	}
	return NULL;
}

CFontFaceNamesEnumerator::CFontFaceNamesEnumerator(LPCWSTR facename, int nFontFamily) : m_pos(0)
{
	//CCriticalSectionLock __lock;
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	TCHAR  buff[LF_FACESIZE+1];
	GetFontLocalName((TCHAR*)facename, buff);
	LPCWSTR srcfacenames[] = {
		buff, NULL, NULL
	};

	int destpos = 0;
	for (const LPCWSTR *p = srcfacenames; *p && destpos < MAXFACENAMES; ++p) {
		m_facenames[destpos++] = *p;
		if (pSettings->FontLink()) {
			const LPCWSTR *facenamep = pSettings->GetFontLinkInfo().lookup(*p);
			if (facenamep) {
				for ( ; *facenamep && **facenamep && destpos < MAXFACENAMES; ++facenamep) {
					m_facenames[destpos++] = *facenamep;
				}
			}
		}
	}
	m_facenames[0] = facename;
	if (pSettings->IsWinXPorLater() && pSettings->FontLink() &&
		pSettings->FontLoader() == SETTING_FONTLOADER_FREETYPE) {
			m_facenames[destpos++] = pSettings->GetFontLinkInfo().sysfn(nFontFamily);
	}
	m_endpos = destpos;
}
