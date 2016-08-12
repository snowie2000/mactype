
#pragma once

#include "common.h"
#include "tlsdata.h"
#include "undocAPI.h"
#include "gdiPlusFlat2.h"

#include "cache.h"
#include "settings.h"
#include <set>
#include <dwrite_1.h>
#include <dwrite_2.h>
#include <dwrite_3.h>

using namespace std;


/*
typedef struct _STRING {
	USHORT  Length;
	USHORT  MaximumLength;
	PCHAR  Buffer;
} ANSI_STRING, *PANSI_STRING;

typedef int 
(WINAPI *PFNLdrGetProcedureAddress)(
									IN HMODULE              ModuleHandle,
									IN PANSI_STRING         FunctionName OPTIONAL,
									IN WORD                 Oridinal OPTIONAL,
									OUT PVOID               *FunctionAddress );
*/

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
		if (lpRealName)
			free(lpRealName);
		if (lpGDIName)
			free(lpGDIName);
	}
};

typedef map<HFONT, CFontSubResult*> CFontCache;
typedef set<HDC> CDCArray;

class CThreadLocalInfo
{
private:
	CBitmapCache	m_bmpCache;
	bool			m_bInExtTextOut;
	bool			m_bInUniscribe;
	bool			m_bInUniTextOut;
	bool			m_bPadding[2];
	bool			m_bInPath;

public:
	CThreadLocalInfo()
		: m_bInExtTextOut(false), m_bInUniscribe(false), m_bInPath(false), m_bInUniTextOut(false)
	{
		TLSDCArray.insert(&m_bmpCache);
	}
	~CThreadLocalInfo()
	{
		TLSDCArray.erase(&m_bmpCache);
	}

	CBitmapCache& BitmapCache()
	{
		return m_bmpCache;
	}

	void InExtTextOut(bool b)
	{
		m_bInExtTextOut = b;
	}
	bool InExtTextOut() const
	{
		return m_bInExtTextOut;
	}

	void InUniscribe(bool b)
	{
		m_bInUniscribe = b;
	}
	bool InUniscribe() const
	{
		return m_bInUniscribe;
	}
	void InUniTextOut(bool b)
	{
		m_bInUniTextOut = b;
	}
	bool InUniTextOut() const
	{
		return m_bInUniTextOut;
	}
	void InPath(bool b)
	{
		m_bInPath = b;
	}
	bool Path() const
	{
		return m_bInPath;
	}
};

extern CTlsData<CThreadLocalInfo>	g_TLInfo;

BOOL IsProcessExcluded();
BOOL IsProcessUnload();
BOOL IsExeUnload(LPCTSTR lpApp);

FORCEINLINE BOOL IsOSXPorLater()
{
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	return pSettings->IsWinXPorLater();
}

#define HOOK_MANUALLY HOOK_DEFINE
#define HOOK_DEFINE(rettype, name, argtype) \
	extern rettype (WINAPI * ORIG_##name) argtype; \
	extern rettype WINAPI IMPL_##name argtype;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

HFONT GetCurrentFont(HDC hdc);
void AddFontToFT(HFONT hf, LPCTSTR lpszFace, int weight, bool italic);
void AddFontToFT(LPCTSTR lpszFace, int weight, bool italic);
int MyGetProcAddress(HMODULE dll, LPSTR funcname);

#define HOOK_MANUALLY(rettype, name, argtype) \
	LONG hook_demand_##name(bool bForce);
#define HOOK_DEFINE(rettype, name, argtype) ;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY
//EOF
