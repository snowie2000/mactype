#include "common.h"
#include "settings.h"
CRITICAL_SECTION CCriticalSectionLock::m_cs[20];
OWNED_CRITIAL_SECTION COwnedCriticalSectionLock::m_cs[2];
LONG CThreadCounter::interlock;	//snowie!! C++还需要额外申明，汗
TCHAR CGdippSettings::m_szexeName[MAX_PATH+1] = {0};

#ifdef _UNICODE
#undef CharPrev
#define CharPrev(s, c)	((c) - 1)
#endif

//手抜きのパス操作関数群
BOOL WINAPI PathIsRelative(LPCTSTR pszPath)
{
	if (!pszPath || !*pszPath) {
		return FALSE;
	}

	const TCHAR ch1 = pszPath[0];
	const TCHAR ch2 = pszPath[1];
	if (ch1 == _T('\\') && ch2 == _T('\\')) {
		//UNC
		return FALSE;
	}
	if (ch1 == _T('\\') || (ch1 && ch2 == _T(':'))) {
		//絶対パス
		return FALSE;
	}
	return TRUE;
}

BOOL WINAPI PathRemoveFileSpec(LPTSTR pszPath)
{
	if (!pszPath) {
		return FALSE;
	}
	LPTSTR p = pszPath + _tcslen(pszPath);

	while (p >= pszPath) {
		switch (*p) {
		case _T('\\'):
		case _T('/'):
			if (p > pszPath) {
				//c:\aaa -> c:\   <
				//c:\    -> c:\   <
				switch (*(p - 1)) {
				case _T('\\'):
				case _T('/'):
				case _T(':'):
					break;
				default:
					goto END;
				}
			}
		case _T(':'):
			// c:aaa -> c:
			p++;
			goto END;
		}
		if (p <= pszPath) {
			break;
		}
		p = CharPrev(NULL, p);
	}

END:
	if (*p) {
		*p = _T('\0');
		return TRUE;
	}
	return FALSE;
}

LPTSTR WINAPI PathFindExtension(LPCTSTR pszPath)
{
	if (!pszPath) {
		return NULL;
	}

	LPCTSTR p, pszEnd;
	p = pszEnd = pszPath + _tcslen(pszPath);

	while (p > pszPath) {
		switch (*p) {
		case _T('.'):
			return (LPTSTR)p;
		case _T('\\'):
		case _T('/'):
		case _T(':'):
			return (LPTSTR)pszEnd;
		}
		p = CharPrev(pszPath, p);
	}
	return (LPTSTR)pszEnd;
}

LPTSTR WINAPI PathAddBackslash(LPTSTR pszPath)
{
	if (!pszPath) {
		return NULL;
	}

	int cch = _tcslen(pszPath);
	if (cch + 1 >= MAX_PATH) {
		return NULL;
	}

	LPTSTR p = pszPath + cch;
	switch (*CharPrev(pszPath, p)) {
	case _T('\\'):
	case _T('/'):
		break;
	default:
		p[0] = _T('\\');
		p[1] = _T('\0');
		break;
	}
	return pszPath;
}

LPTSTR WINAPI PathCombine(LPTSTR pszDest, LPCTSTR pszDir, LPCTSTR pszFile)
{
	if (!pszDest || !pszDir || !pszFile) {
		return NULL;
	}

	//かなり手抜き
	TCHAR szCurDir[MAX_PATH], szDir[MAX_PATH+1];
	GetCurrentDirectory(MAX_PATH, szCurDir);
	_tcsncpy(szDir, pszDir, MAX_PATH - 1);
	szDir[MAX_PATH - 1] = _T('\0');
	PathAddBackslash(szDir);
	if (!SetCurrentDirectory(szDir)) {
		*pszDest = _T('\0');
		return NULL;
	}
	TCHAR szFile[MAX_PATH];
	_tcsncpy(szFile, pszFile, MAX_PATH - 1);
	szFile[MAX_PATH - 1] = _T('\0');
	GetFullPathName(szFile, MAX_PATH, pszDest, NULL);
	SetCurrentDirectory(szCurDir);
TRACE(_T("PathCombine: %s\n"), pszDest);
	return pszDest;
}

LPWSTR _StrDupExAtoW(LPCSTR pszMB, int cchMB /*= -1*/, LPWSTR pszStack /*= NULL*/, int cchStack /*= 0*/, int* pcchWC /*= NULL*/, int nACP)
{
	int _cchWC;
	if (!pcchWC) {
		pcchWC = &_cchWC;
	}
	*pcchWC = 0;

	if (!pszMB) {
		return NULL;
	}
	if (cchMB == -1) {
		cchMB = strlen(pszMB);
	}
	const int cchWC = MultiByteToWideChar(nACP, 0, pszMB, cchMB, NULL, 0);
	if(cchWC < 0) {
		return NULL;
	}

	LPWSTR pszWC;
	if(cchWC < cchStack && pszStack) {
		pszWC = pszStack;
		ZeroMemory(pszWC, sizeof(WCHAR) * (cchWC + 1));
	} else {
		pszWC = (LPWSTR)calloc(sizeof(WCHAR), cchWC + 1);
		if (!pszWC) {
			return NULL;
		}
	}
	MultiByteToWideChar(nACP, 0, pszMB, cchMB, pszWC, cchWC);
	pszWC[cchWC] = L'\0';
	*pcchWC = cchWC;
	return pszWC;
}

#ifdef _DEBUG
typedef struct {
	LPCSTR		name;
	UINT		flag;
	UINT		mask;
	UINT		xor;
} FLAG_NAME_MAP;
#define DEF_FLAG_NAME(f,m,x)	{ #f, f, m, x }
#define END_FLAG()				{ NULL }

LPCSTR Dbg_GetFlagNames(UINT flags, const FLAG_NAME_MAP* p, LPSTR worker)
{
	if (!worker)
		return "";
	if (!flags || !p)
		return "0";

	LPSTR wk = worker;
	*wk = '\0';
	while (p->name) {
		if (((flags & p->mask) ^ p->xor) == p->flag) {
			if (wk != worker) {
				memcpy(wk, " | ", 4);
				wk += 3;
			}
			strcpy(wk, p->name);
			wk += strlen(p->name);
			flags &= ~p->mask;
		}
		p++;
	}
	if (flags) {
		if (wk != worker) {
			memcpy(wk, " | ", 4);
			wk += 3;
		}
		sprintf(wk, "%04X", flags);
	}
	return *worker ? worker : "0";
}

void Dbg_TraceExtTextOutW(int nXStart, int nYStart, UINT fuOptions, LPCWSTR lpString, int cbString, const int* lpDx)
{
	LPWSTR p;
	if (fuOptions & ETO_GLYPH_INDEX) {
		p = (LPWSTR)_alloca((cbString * 5 + 1) * sizeof(WCHAR));
		LPWSTR q = p;
		for (int i = 0; i < cbString; ++i) {
			q += wsprintf(q, _T(" %04X"), lpString[i]);
		}
	} else {
		p = (LPWSTR)_alloca((cbString + 1) * sizeof(WCHAR));
		memcpy(p, lpString, cbString * sizeof(WCHAR));
		p[cbString] = 0;
	}

	static const FLAG_NAME_MAP c_map[] = {
		DEF_FLAG_NAME(ETO_OPAQUE, ETO_OPAQUE, 0),
		DEF_FLAG_NAME(ETO_CLIPPED, ETO_CLIPPED, 0),
		DEF_FLAG_NAME(ETO_GLYPH_INDEX, ETO_GLYPH_INDEX, 0),
		DEF_FLAG_NAME(ETO_RTLREADING, ETO_RTLREADING, 0),
		DEF_FLAG_NAME(ETO_NUMERICSLOCAL, ETO_NUMERICSLOCAL, 0),
		DEF_FLAG_NAME(ETO_NUMERICSLATIN, ETO_NUMERICSLATIN, 0),
		DEF_FLAG_NAME(ETO_IGNORELANGUAGE, ETO_IGNORELANGUAGE, 0),
		DEF_FLAG_NAME(ETO_PDY, ETO_PDY, 0),
		END_FLAG(),
	};
	CHAR wk[1024];
	TRACE(_T("ExtTextOutW(%d, %d, %hs, \"%ls\", %d, %hs)\n")
			, nXStart, nYStart, Dbg_GetFlagNames(fuOptions, c_map, wk), p, cbString, lpDx ? "{...}" : "NULL");
}

void Dbg_TraceScriptItemize(const WCHAR* pwcInChars, int cInChars)
{
	LPWSTR p = (LPWSTR)_alloca((cInChars + 1) * sizeof(WCHAR));
	memcpy(p, pwcInChars, cInChars * sizeof(WCHAR));
	p[cInChars] = 0;

	TRACE(_T("ScriptItemize(\"%ls\", %d)\n"), p, cInChars);
}

void Dbg_TraceScriptShape(const WCHAR* pwcChars, int cChars, const SCRIPT_ANALYSIS* psa, const WORD* pwOutGlyphs, int cGlyphs)
{
	LPWSTR pc = (LPWSTR)_alloca((cChars + 1) * sizeof(WCHAR));
	memcpy(pc, pwcChars, cChars * sizeof(WCHAR));
	pc[cChars] = 0;
	LPWSTR pg;
	if (pwOutGlyphs) {
		if (psa->fNoGlyphIndex) {
			pg = (LPWSTR)_alloca((cGlyphs + 1) * sizeof(WCHAR));
			memcpy(pg, pwOutGlyphs, cGlyphs * sizeof(WCHAR));
			pg[cGlyphs] = 0;
		} else {
			pg = (LPWSTR)_alloca((cGlyphs * 5 + 1) * sizeof(WCHAR));
			LPWSTR q = pg;
			for (int i = 0; i < cGlyphs; ++i) {
				q += wsprintf(q, _T(" %04X"), pwOutGlyphs[i]);
			}
		}
	}

	TRACE(_T("ScriptShape(\"%ls\", %d, Script=%d%S%S%S%S%S%S, BidiLevel=%d%S%S%S%S%S%S%S%S"), 
		pc, cChars,
		psa->eScript, psa->fRTL ? ", RTL" : "", psa->fLayoutRTL ? ", LayoutRTL" : "", 
		psa->fLinkBefore ? ", LinkBefore" : "", psa->fLinkAfter ? ", LinkAfter" : "", 
		psa->fLogicalOrder ? ", LogicalOrder" : "", psa->fNoGlyphIndex ? ", NoGlyphIndex" : "", 
		psa->s.uBidiLevel, psa->s.fOverrideDirection ? ", OverrideDirection" : "", 
		psa->s.fInhibitSymSwap ? ", InhibitSymSwap" : "", psa->s.fCharShape ? ", CharShape" : "", 
		psa->s.fDigitSubstitute ? ", DigitSubstitute" : "", psa->s.fInhibitLigate ? ", InhibitLigate" : "", 
		psa->s.fDisplayZWG ? ", DisplayZWG" : "", psa->s.fArabicNumContext ? ", ArabicNumContext" : "", 
		psa->s.fGcpClusters ? ", GcpClusters" : ""
	);

	if (pwOutGlyphs) {
		TRACE(_T(", \"%ls\", %d"), pg, cGlyphs);
	}

	TRACE(_T(")\n"));
}

void Dbg_TractGetTextExtent(LPCSTR lpString, int cbString, LPSIZE lpSize)
{
	LPSTR p = (LPSTR)_alloca((cbString + 1) * sizeof(CHAR));
	memcpy(p, lpString, cbString * sizeof(CHAR));
	p[cbString] = 0;
	TRACE(_T("GetTextExtentA(\"%hs\", %d) = { %d, %d }\n")
			, p, cbString, lpSize->cx, lpSize->cy);
}

void Dbg_TractGetTextExtent(LPCWSTR lpString, int cbString, LPSIZE lpSize)
{
	LPWSTR p = (LPWSTR)_alloca((cbString + 1) * sizeof(WCHAR));
	memcpy(p, lpString, cbString * sizeof(WCHAR));
	p[cbString] = 0;
	TRACE(_T("GetTextExtentW(\"%ls\", %d) = { %d, %d }\n")
			, p, cbString, lpSize->cx, lpSize->cy);
}
#endif
