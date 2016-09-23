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


	//�����t�H���g
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
		wcscpy(wcsstr(m_szFileName, L"64"), wcsstr(m_szFileName, L"64")+2);	//ɾ�����е�64��ͳһʹ��mactype.ini�����ļ�
#endif
	wcsncat(m_szFileName, _T(".ini"), MAX_PATH-1);
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
	// �e��ݒ�ǂݍ���
	// INI�t�@�C���̗�:
	// [General]
	// HookChildProcesses=0
	// HintingMode=0
	// AntiAliasMode=0
	// NormalWeight=0
	// BoldWeight=0
	// ItalicSlant=0
	// EnableKerning=0
	// MaxHeight=0
	// ForceChangeFont=�l�r �o�S�V�b�N
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
	// �l�r �o�S�V�b�N=0,1,2,3,4,5

	WritePrivateProfileString(NULL, NULL, NULL, lpszFile);

	TCHAR szAlternative[MAX_PATH], szMainFile[MAX_PATH];
	if (GetPrivateProfileString(c_szGeneral, _T("AlternativeFile"), _T(""), szAlternative, MAX_PATH, lpszFile)) {
		if (PathIsRelative(szAlternative)) {
			TCHAR szDir[MAX_PATH];
			StringCchCopy(szDir, MAX_PATH, lpszFile);
			PathRemoveFileSpec(szDir);
			PathCombine(szAlternative, szDir, szAlternative);
		}
		StringCchCopy(szMainFile, MAX_PATH, lpszFile);	//��ԭʼ�ļ�����������
		StringCchCopy(m_szFileName, MAX_PATH, szAlternative);	
		lpszFile = m_szFileName;
		WritePrivateProfileString(NULL, NULL, NULL, lpszFile);
	}

	_GetAlternativeProfileName(m_szexeName, lpszFile);	//��ÿ��ܵĶ�����������
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
		if (token.GetCount()>=4)	//���ָ����ǳɫ��Ӱ
			m_nShadowDarkColor = _httoi(token.GetArgument(3));	//��ȡ��Ӱ
		else
			m_nShadowDarkColor = 0;	//����Ϊ��ɫ
		if (token.GetCount()>=6)	//���ָ������ɫ��Ӱ
		{
			m_nShadowLightColor = _httoi(token.GetArgument(5));	//��ȡ��Ӱ
			m_nShadow[3] = _StrToInt(token.GetArgument(4), m_nShadow[2]); //��ȡ���
		}
		else
		{
			m_nShadowLightColor = m_nShadowLightColor;		//�����ǳɫ��Ӱ��ͬ
			m_nShadow[3] = m_nShadow[2];		//���Ҳ��ͬ
		}
SKIP:
		;
	}

	m_bHookChildProcesses = !!GetPrivateProfileInt(c_szGeneral, _T("HookChildProcesses"), false, lpszFile);
	m_bUseMapping	= !!_GetFreeTypeProfileInt(_T("UseMapping"), false, lpszFile);
	m_nBolderMode	= _GetFreeTypeProfileInt(_T("BolderMode"), 0, lpszFile);
	m_nGammaMode	= _GetFreeTypeProfileInt(_T("GammaMode"), -1, lpszFile);
	m_fGammaValue	= _GetFreeTypeProfileBoundFloat(_T("GammaValue"), 1.0f, GAMMAVALUE_MIN, GAMMAVALUE_MAX, lpszFile);
	m_fGammaValueForDW = _GetFreeTypeProfileBoundFloat(_T("GammaValueDW"), 0.0f, 0, GAMMAVALUE_MAX, lpszFile);
	m_fRenderWeight	= _GetFreeTypeProfileBoundFloat(_T("RenderWeight"), 1.0f, RENDERWEIGHT_MIN, RENDERWEIGHT_MAX, lpszFile);
	m_fContrast		= _GetFreeTypeProfileBoundFloat(_T("Contrast"), 1.0f, CONTRAST_MIN, CONTRAST_MAX, lpszFile);
#ifdef _DEBUG
	// GammaValue���ؗp
	//CHAR GammaValueTest[1025];
	//sprintf(GammaValueTest, "GammaValue=%.6f\nContrast=%.6f\n", m_fGammaValue, m_fContrast);
	//MessageBoxA(NULL, GammaValueTest, "GammaValue�e�X�g", 0);
#endif
	m_bLoadOnDemand	= !!_GetFreeTypeProfileInt(_T("LoadOnDemand"), false, lpszFile);
	m_bFontLink		= _GetFreeTypeProfileInt(_T("FontLink"), 0, lpszFile);

	m_bIsInclude	= !!_GetFreeTypeProfileInt(_T("UseInclude"), false, lpszFile);
	m_nMaxHeight	= _GetFreeTypeProfileBoundInt(_T("MaxHeight"), 0, 0, 0xfff, lpszFile);	//���ֻ�ܵ�65535��cache�����ƣ����Ҵ�������ʵ�ʼ�ֵ
	m_nMinHeight = _GetFreeTypeProfileBoundInt(_T("MinHeight"), 0, 0, m_nMaxHeight, lpszFile);	//Minimum size of rendered font. DPI aware alternative.
	m_nBitmapHeight = _GetFreeTypeProfileBoundInt(_T("MaxBitmap"), 0, 0, 255, lpszFile);
	m_bHintSmallFont = _GetFreeTypeProfileInt(_T("HintSmallFont"), 0, lpszFile);
	m_bDirectWrite = _GetFreeTypeProfileInt(_T("DirectWrite"), 0, lpszFile);
	m_nRenderingModeForDW	= _GetFreeTypeProfileInt(_T("RenderingModeForDW"), 5, lpszFile);
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
		// API���������Ă����͂��Ȃ̂Ŏ��O�����͖�����
		if (m_nFontSubstitutes == SETTING_FONTSUBSTITUTE_ALL) {
			m_nFontSubstitutes = SETTING_FONTSUBSTITUTE_DISABLE;
		}
		m_bFontLink = 0;
	}

	// �t�H���g�w��
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

	// OS�̃o�[�W������XP�ȍ~���ǂ���
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

	// [Exclude]�Z�N�V�������珜�O�t�H���g���X�g��ǂݍ���
	AddExcludeListFromSection(_T("Exclude"), lpszFile, m_arrExcludeFont);
	AddExcludeListFromSection(_T("Exclude"), szMainFile, m_arrExcludeFont);
	// [ExcludeModule]�Z�N�V�������珜�O���W���[�����X�g��ǂݍ���
	AddListFromSection(_T("ExcludeModule"), lpszFile, m_arrExcludeModule);
	AddListFromSection(_T("ExcludeModule"), szMainFile, m_arrExcludeModule);
	// [IncludeModule]�Z�N�V��������Ώۃ��W���[�����X�g��ǂݍ���
	AddListFromSection(_T("IncludeModule"), lpszFile, m_arrIncludeModule);
	AddListFromSection(_T("IncludeModule"), szMainFile, m_arrIncludeModule);
	// [UnloadDLL]��ȫ�����ص�ģ��
	AddListFromSection(_T("UnloadDLL"), lpszFile, m_arrUnloadModule);
	AddListFromSection(_T("UnloadDLL"), szMainFile, m_arrUnloadModule);
	// [ExcludeSub]�����������滻��ģ��
	AddListFromSection(L"ExcludeSub", lpszFile, m_arrUnFontSubModule);
	AddListFromSection(L"ExcludeSub", szMainFile, m_arrUnFontSubModule);
	//������ų���ģ�飬��ر������滻
	if (m_nFontSubstitutes)
	{
		ModuleHashMap::const_iterator it=m_arrUnFontSubModule.begin();
		while (it!=m_arrUnFontSubModule.end())
		{
			if (GetModuleHandle(it->c_str()))
			{
				m_nFontSubstitutes = 0;	//�ر��滻
				break;
			}
			++it;
		}
	}

	// [Individual]�Z�N�V��������t�H���g�ʐݒ��ǂݍ���
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
		GetFontLocalName(p, buff);//ת��������
		set<wstring>::const_iterator it = arr.find(buff);
		if (it==arr.end())
			arr.insert(buff);
		for (; *p; p++);	//������һ��
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
		for (; *p; p++);	//������һ��
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
			return false;	//��������5������Ϊ��ʹ�ô˲���
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

		//"�l�r �o�S�V�b�N=0,0" �݂����ȕ�����𕪊�
		LPTSTR value = _tcschr(p, _T('='));
		CStringTokenizer token;
		int argc = 0;
		if (value) {
			*value++ = _T('\0');
			argc = token.Parse(value);
		}

		GetFontLocalName(p, buff);//ת��������

		CFontIndividual fi(buff);
		const CFontSettings& fsCommon = m_FontSettings;
		CFontSettings& fs = fi.GetIndividual();
		//Individual��������΋��ʐݒ���g��
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

//atol�Ƀf�t�H���g�l��Ԃ���悤�ɂ����悤�ȕ�
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

//atof�Ƀf�t�H���g�l��Ԃ���悤�ɂ����悤�ȕ�
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

bool CGdippSettings::IsExeUnload(LPCTSTR lpApp) const	//����Ƿ��ں������б���
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrUnloadModule.begin();
	for(; it != m_arrUnloadModule.end(); ++it) {
		if (!lstrcmpi(lpApp, it->c_str())) {	//ƥ���ų���
			return true;
		}
	}
	return false;
}

bool CGdippSettings::IsExeInclude(LPCTSTR lpApp) const	//����Ƿ��ڰ������б���
{
	if (m_bRunFromGdiExe) {
		return false;
	}
	GetEnvironmentVariableW(L"MACTYPE_FORCE_LOAD", NULL, 0);
	if (GetLastError()!=ERROR_ENVVAR_NOT_FOUND)
		return false;
	ModuleHashMap::const_iterator it = m_arrIncludeModule.begin();
	for(; it != m_arrIncludeModule.end(); ++it) {
		if (!lstrcmpi(lpApp, it->c_str())) {	//ƥ���ų���
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

// �e�[�u���������֐� 0 - 12�܂�
// LCD�p�e�[�u���������֐� �e0 - 12�܂�
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

//������Ȃ��ꍇ�͋��ʐݒ��Ԃ�
extern BOOL g_ccbIndividual;
const CFontSettings& CGdippSettings::FindIndividual(LPCTSTR lpFaceName) const
{
	CFontIndividual* p		= m_arrIndividual.Begin();
	CFontIndividual* end	= m_arrIndividual.End();
	if (lpFaceName && *lpFaceName==L'@')
		++lpFaceName;	//��������ʹ�ú�����趨
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
	//&lf == &lfOrg����
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

//�l�I��char(-128�`127)�ŏ\��
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
	memset(AllowDefaultLink, 1, sizeof(AllowDefaultLink));	//Ĭ��������������
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

static void GetFontLocalName(LOGFONT& lf)	//�������ı��ػ�����
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
	//GetVersionEx(&sOsVinfo);	//��ò���ϵͳ�汾��
	//const CGdippSettings* pSettings = CGdippSettings::GetInstance();

	WCHAR name[0x2000];
	DWORD namesz;
	DWORD valuesz;
	WCHAR value[0x2000];
	WCHAR buf[0x2000];
	LONG rc;
	DWORD regtype;

	for (int k = 0; ; ++k) {	//���������е���������
		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h2, k, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);		//���������Ѱ��
		if (rc == ERROR_NO_MORE_ITEMS) break;
		if (rc != ERROR_SUCCESS) break;
		if (regtype != REG_SZ) continue;
		StringCchCopy(buf, sizeof(buf)/sizeof(buf[0]), name);
		if (buf[wcslen(buf) - 1] == L')') {				//ȥ������
			LPWSTR p;
			if ((p = wcsrchr(buf, L'(')) != NULL) {
				*p = 0;
			}
		}
		while (buf[wcslen(buf)-1] == L' ')
			buf[wcslen(buf)-1] = 0;
		//��õĶ�Ӧ��������
		FontNameCache.Add(value, buf);
	}

	int row = 0;
	LOGFONT truefont;
	memset(&truefont, 0, sizeof(truefont));
	for (int i = 0; row < INFOMAX; ++i) {
		int col = 0;

		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h1, i, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);	//���һ���������������
		if (rc == ERROR_NO_MORE_ITEMS) break;
		if (rc != ERROR_SUCCESS) break;
		if (regtype != REG_MULTI_SZ) continue;		//��Ч����������
		//����������ʵ����
		
		TCHAR buff[LF_FACESIZE];
		GetFontLocalName(name, buff);

		info[row][col] = _wcsdup(buff);		//��һ��Ϊ������
		++col;

		for (LPCWSTR linep = value; col < FONTMAX && *linep; linep += wcslen(linep) + 1) {
			LPCWSTR valp = NULL;
			for (LPCWSTR p = linep; *p; ++p) {
				if (*p == L',' && ((char)*(p+1)<0x30 || (char)*(p+1)>0x39))		//����Ѱ�����������С��������ṩ����������
					{
						LPWSTR lp;
						StringCchCopy(buf, sizeof(buf)/sizeof(buf[0]), p+1);
						if (lp=wcschr(buf, L','))
							*lp = 0;
						valp = buf;
						break;
					}
			}
			if (!valp) {		//û�ҵ������������ṩ������
				/*for (int k = 0; ; ++k) {
					namesz = sizeof name;
					value2sz = sizeof value2;
					rc = RegEnumValue(h2, k, name, &namesz, 0, &regtype, (LPBYTE)value2, &value2sz);		//���������Ѱ��
					if (rc == ERROR_NO_MORE_ITEMS) break;
					if (rc != ERROR_SUCCESS) break;
					if (regtype != REG_SZ) continue;
					if (lstrcmpi(value2, linep) != 0) continue;		//Ѱ�����������������ļ���Ӧ��������

					StringCchCopyW(buf, sizeof(buf)/sizeof(buf[0]), name);
					if (buf[wcslen(buf) - 1] == L')') {				//ȥ������
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
				//StringCchCopy(truefont.lfFaceName, LF_FACESIZE, buff);	//���Ƶ��ṹ��
				//pSettings->CopyForceFont(truefont, truefont);		//����滻����
				info[row][col] = _wcsdup(buff);//truefont.lfFaceName);			//���Ƶ����ӱ���
				++col;
			}
		}
		if (col == 1) {			//ֻ��һ���û�����ӣ�ɾ����
			free(info[row][0]);
			info[row][0] = NULL;
		} else {
			/*if (sOsVinfo.dwMajorVersion>=6 && sOsVinfo.dwMinorVersion>=1)	//�汾��>=6.1����Win7ϵ��
			{
				//���������ӱ���������
				LPWSTR swapbuff[32];
				memcpy(swapbuff, info[row], 32*sizeof(LPWSTR));	//�������ƹ���
				for (int i=1; i<col; i++)
					info[row][i]=swapbuff[col-i];	//�����������ӱ�
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
	if (pSettings->FontSubstitutes()>=SETTING_FONTSUBSTITUTE_INIONLY && pSettings->CopyForceFont(truefont, syslf))	//ʹ����ȫ�滻ģʽʱ���滻��ϵͳ����
	{
		WCHAR envname[30] = L"MT_SYSFONT";
		WCHAR envvalue[30] = { 0 };
		HFONT tempfont;
		if (GetEnvironmentVariable(L"MT_SYSFONT", envvalue, 29) && GetObjectType(tempfont = (HFONT)wcstoull(envvalue, 0 ,10)) == OBJ_FONT)//�Ѿ����������
		{
			g_alterGUIFont = tempfont;	//ֱ��ʹ����ǰ������
		}
		else
		{
			g_alterGUIFont = CreateFontIndirectW(&truefont);	//����һ���µ��滻����
			_ui64tow((ULONG_PTR)g_alterGUIFont, envvalue, 10);	//ת��Ϊ�ַ���
			SetEnvironmentVariable(envname, envvalue);		//д�뻷������
		}
	}

	//���ڻ�ȡ��Ӧ�������͵�Ĭ����������
	memset(DefaultFontLink, 0, sizeof(TCHAR)*(FF_DECORATIVE+1)*(LF_FACESIZE+1));	//��ʼ��Ϊ0
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
	
	for (int i=0; i<FF_DECORATIVE+1; ++i)	//ת����������
	{
		if (!*DefaultFontLink[i])
			GetFontLocalName(DefaultFontLink[i], DefaultFontLink[i]);
	}

	//���ڻ�ȡ��Ӧ��CodePage�Ƿ���Ҫ����fontlink��Ĭ�϶���Ҫ�������ӡ�
	HKEY h4;
	if (ERROR_SUCCESS != RegOpenKeyEx(HKEY_LOCAL_MACHINE, REGKEY4, 0, KEY_QUERY_VALUE, &h4)) return;

	for (int i=0; i<0xff; ++i)
	{
		namesz = sizeof name;
		valuesz = sizeof value;
		rc = RegEnumValue(h4, i, name, &namesz, 0, &regtype, (LPBYTE)value, &valuesz);	//���һ��charset��ֵ
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
						continue;	//û�д�����
					buff = truefont.lfFaceName;

					StringCchCopy(truefont2.lfFaceName, LF_FACESIZE, vp);
					truefont2.lfCharSet=DEFAULT_CHARSET;
					if (!GetFontLocalName(truefont2))
						continue;	//û�д�����
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
	if (GetSize() <= 0) return NULL;
	//bFontExist = true;
	CFontSubstituteData v;
	CFontSubstituteData k;

	k.m_bCharSet = true;
	k.m_lf = lf;

	TCHAR * buff;	//���ٻ���������ʵ����
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
