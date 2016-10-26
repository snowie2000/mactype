#pragma once

#define _CRT_SECURE_NO_DEPRECATE 1
#ifdef _WIN64
#define _WIN32_WINNT _WIN32_WINNT_WIN10
#define WINVER _WIN32_WINNT_VISTA
#else
#define _WIN32_WINNT _WIN32_WINNT_WIN10
#define WINVER _WIN32_WINNT_WIN10
#endif
#define NTDDI_VERSION NTDDI_WIN10_RS1
#define WIN32_LEAN_AND_MEAN 1
#define UNICODE  1
#define _UNICODE 1

#define NOMINMAX
#include <Windows.h>
#include <usp10.h>
//#include <limits>
#include <functional>
//#include <iterator>
#include <algorithm>
#include <memory>
#include "array.h"
#include <set>
#include "ownedcs.h"
#include "undocAPI.h"
#include <d2d1.h>
#include <d2d1_1.h>
#include <d2d1_3.h>
#include <dwrite.h>
#include <dwrite_1.h>
#include <dwrite_2.h>
#include <dwrite_3.h>
//#include <wincodec.h>
//#include <wincodecsdk.h>

#define for if(0);else for

#include <tchar.h>
#include <stddef.h>
#define STRSAFE_NO_DEPRECATE
#include <strsafe.h>

#define _CRTDBG_MAP_ALLOC
#include <cstdlib>
#include <malloc.h>
#include <crtdbg.h>

#include <map>
#include <string>
using namespace std;
#ifdef _M_IX86
//#include "optimize/optimize.h"
#endif

#define FONT_MAGIC_NUMBER 0xA8

#define ASSERT			_ASSERTE
#define Assert			_ASSERTE
#ifdef _DEBUG
#define new				new(_NORMAL_BLOCK, __FILE__, __LINE__)
#endif

#ifndef NOP_FUNCTION
#if (_MSC_VER >= 1210)
#define NOP_FUNCTION	__noop
#else
#define NOP_FUNCTION	(void)0
#endif	//_MSC_VER
#endif	//!NOP_FUNCTION
#ifndef C_ASSERT
#define C_ASSERT(e)		typedef char __C_ASSERT__[(e)?1:-1]
#endif	//!C_ASSERT
#ifndef FORCEINLINE
#if (_MSC_VER >= 1200)
#define FORCEINLINE		__forceinline
#else
#define FORCEINLINE		__inline
#endif	//_MSC_VER
#endif	//!FORCEINLINE


void Log(char* Msg);
void Log(wchar_t* Msg);


FORCEINLINE HINSTANCE GetDLLInstance()
{
	extern HINSTANCE g_hinstDLL;
	return g_hinstDLL;
}

//攔懠惂屼
class CCriticalSectionLock
{
#define MAX_CRITICAL_COUNT 20
private:
	static CRITICAL_SECTION m_cs[MAX_CRITICAL_COUNT];
	friend class CCriticalSectionLockTry;
	int m_index;
public:
	enum {
		CS_LIBRARY,
		CS_CACHEDFONT,
		CS_FONTENG,
		//CS_CMAPCACHE,
		//CS_IMAGECACHE,
		CS_SETTING,
		CS_MAIN,
		CS_FONTCACHE,
		CS_MANAGER,
		CS_CREATEFONT,
		CS_FONTLINK,
		CS_FONTMAP,
		CS_OWNEDCS,
		CS_VIRTMEM,
	};
	CCriticalSectionLock(int index=CS_LIBRARY):
	  m_index(index)
	{
		::EnterCriticalSection(&m_cs[index]);
	}
	~CCriticalSectionLock()
	{
		::LeaveCriticalSection(&m_cs[m_index]);
	}
	static void Init()
	{
		for (int i=0;i<MAX_CRITICAL_COUNT;i++)
			::InitializeCriticalSection(&m_cs[i]);
	}
	static void Term()
	{
		for (int i=0;i<MAX_CRITICAL_COUNT;i++)
			::DeleteCriticalSection(&m_cs[i]);
	}
};

#ifdef _DEBUG
#define TRACE	_Trace
#include <stdarg.h>	//va_list
#include <stdio.h>	//_vsnwprintf

static void _Trace(LPCTSTR pszFormat, ...)
{
	CCriticalSectionLock __lock;
	va_list argptr;
	va_start(argptr, pszFormat);
	//w(v)sprintf偼1024暥帤埲忋曉偟偰偙側偄
	TCHAR szBuffer[10240];
	wvsprintf(szBuffer, pszFormat, argptr);

	//僨僶僢僈傪傾僞僢僠偟偰傞帪偼僨僶僢僈偵儊僢僙乕僕傪弌偡
	//if (IsDebuggerPresent()) {
	OutputDebugString(szBuffer);
	return;
	//}

	extern HANDLE g_hfDbgText;
	HANDLE hf = g_hfDbgText;
	if (!hf) {
		TCHAR szFileName[MAX_PATH+16];
		GetModuleFileName(GetDLLInstance(), szFileName, MAX_PATH);
		TCHAR *p1 = _tcsrchr(szFileName, _T('\\'));
		if (p1 && p1 > szFileName) {
			*p1 = 0;
			TCHAR *p2 = _tcsrchr(szFileName, _T('\\'));
			if (p2)
				memmove(p2 + 1, p1 + 1, (_tcslen(p1) + 1) * sizeof(TCHAR));
		}
		_tcscpy(szFileName + _tcslen(szFileName) - 4, L"_dbg.txt");
		g_hfDbgText = hf = CreateFile(szFileName, GENERIC_WRITE, FILE_SHARE_READ, NULL, CREATE_ALWAYS, 0, NULL);
		if(hf != INVALID_HANDLE_VALUE) {
			SetFilePointer(hf, 0, NULL, FILE_BEGIN);
#ifdef _UNICODE
			WORD w = 0xfeff;
			DWORD cb;
			WriteFile(hf, &w, sizeof(WORD), &cb, NULL);
#endif
		}
	}
	if(hf != INVALID_HANDLE_VALUE) {
		DWORD cb;
		WriteFile(hf, szBuffer, _tcslen(szBuffer) * sizeof(TCHAR), &cb, NULL);
	}
}
#else	//!_DEBUG
#define TRACE	NOP_FUNCTION
//伀PSDK 2003R2偺winnt.h
//#ifndef NOP_FUNCTION
//#if (_MSC_VER >= 1210)
//#define NOP_FUNCTION __noop
//#else
//#define NOP_FUNCTION (void)0
//#endif
//#endif
#endif	//_DEBUG

//TRACE儅僋儘
//巊梡椺: TRACE(_T("cx: %d\n"), cx);
#ifdef USE_TRACE
#define TRACE2	_Trace2
#define TRACE2_STR	_Trace2_Str
#define TRACE2_BIN	_Trace2_Bin
#include <stdarg.h>	//va_list
#include <stdio.h>	//_vsnwprintf

static void _Trace2(LPCTSTR pszFormat, ...)
{
	CCriticalSectionLock __lock;
	va_list argptr;
	va_start(argptr, pszFormat);
	//w(v)sprintf偼1024暥帤埲忋曉偟偰偙側偄
	TCHAR szBuffer[1024];
	wvsprintf(szBuffer, pszFormat, argptr);
	OutputDebugString(szBuffer);
}

static void _Trace2_Bin(LPWSTR func, int line, LPVOID lpString, UINT cbString)
{
	const PBYTE srcp = (const PBYTE)lpString;
	WCHAR buf[0x1000];
	LPWSTR p = buf;
	for (UINT i = 0; i < 32 && i < cbString; ++i) {
		wsprintf(p, L"%02x ", srcp[i]);
		p += lstrlen(p);
	}
	*p = 0;
	TRACE(_T("%s %d: %d %08x %s\n"), func, line, cbString, lpString, buf);
}

static void _Trace2_Str(LPWSTR func, int line, LPCWSTR lpString, UINT cbString)
{
	WCHAR buf[0x1000];
	UINT len = Min(cbString, countof(buf) - 1);
	lstrcpyn(buf, lpString, len + 1);
	buf[countof(buf) - 1] = 0;
	TRACE(_T("%s %d: %d %s\n"), func, line, cbString, buf);
	LPWSTR p = buf;
	for (UINT i = 0; i < 32 && i < cbString; ++i) {
		wsprintf(p, L"%04x ", lpString[i]);
		p += lstrlen(p);
	}
	TRACE(_T("%s %d: %d %08x %s\n"), func, line, cbString, lpString, buf);
}

#else	//!USE_TRACE
#define TRACE2	NOP_FUNCTION
#define TRACE2_STR	NOP_FUNCTION
#define TRACE2_BIN	NOP_FUNCTION
#endif	//USE_TRACE


class COwnedCriticalSectionLock
{
private:
	static OWNED_CRITIAL_SECTION m_cs[2];
	WORD FOwner;
	int m_index;
public:
	enum {
		OCS_FREETYPE,
		OCS_DC
	};
	COwnedCriticalSectionLock():FOwner(0), m_index(OCS_FREETYPE)
	{
		EnterOwnedCritialSection(&m_cs[m_index], FOwner);
	}
	COwnedCriticalSectionLock(WORD Owner, int index=OCS_FREETYPE):FOwner(Owner), m_index(index)
	{
		EnterOwnedCritialSection(&m_cs[m_index], Owner);
	}
	~COwnedCriticalSectionLock()
	{
		LeaveOwnedCritialSection(&m_cs[m_index], FOwner);
	}
	static void Init()
	{
		for (int i=0;i<2;i++)
		{
			InitializeOwnedCritialSection(&m_cs[i]);
		}
	}
	static void Term()
	{
		for (int i=0;i<2;i++)
		{
			DeleteOwnedCritialSection(&m_cs[i]);
		}
	}
};

class CThreadCounter
{
private:
	static LONG interlock;
public:
	CThreadCounter()
	{
		InterlockedIncrement(&interlock);
	}
	~CThreadCounter()
	{
		InterlockedDecrement(&interlock);
	}
	static void Init()
	{
		interlock = 0;
	}
	static int Count()
	{
		return InterlockedExchange(&interlock, interlock);
	}
};

class CCriticalSectionLockTry
{
public:
	BOOL TryEnter(int index=CCriticalSectionLock::CS_LIBRARY)
	{
		return ::TryEnterCriticalSection(&CCriticalSectionLock::m_cs[index]);
	}
	int CritalCount(int index=CCriticalSectionLock::CS_LIBRARY)
	{
		return CCriticalSectionLock::m_cs[index].RecursionCount;
	}
	void Leave(int index=CCriticalSectionLock::CS_LIBRARY)
	{
		::LeaveCriticalSection(&CCriticalSectionLock::m_cs[index]);
	}
};

// 巊梡屻偼free偱奐曻偡傞帠
LPWSTR _StrDupExAtoW(LPCSTR pszMB, int cchMB, LPWSTR pszStack, int cchStack, int* pcchWC, int nACP = CP_ACP);
static inline LPWSTR _StrDupAtoW(LPCSTR pszMB, int cchMB = -1, int* pcchWC = NULL)
{
	return _StrDupExAtoW(pszMB, cchMB, NULL, 0, pcchWC);
}

// useful macros
#define NOCOPY(T)					T(const T&); T& operator=(const T&)
#define countof(array)				(sizeof(array)/sizeof(array[0]))
#define sizeof_struct(s, m)			(((int)((char*)(&((s*)0)->m) - ((char*)((s*)0)))) + sizeof(((s*)0)->m))
#ifndef offsetof
#define offsetof(s,m)				(size_t)&(((s*)0)->m)
#endif
#ifdef _DEBUG
#define Verify(expr)				_ASSERTE(expr)
#else
#define Verify(expr)				(expr)
#endif

template<typename T> FORCEINLINE T Min(T x, T y) { return (x < y) ? x : y; }
template<typename T> FORCEINLINE T Max(T x, T y) { return (y < x) ? x : y; }
template<typename T> FORCEINLINE T Bound(T x, T m, T M) { return (x < m) ? m : ((x > M) ? M : x); }
template<typename T> FORCEINLINE int Sgn(T x, T y) { return (x > y) ? 1 : ((x < y) ? -1 : 0); }


//宆僠僃僢僋婡擻偮偒DeleteXXX/SelectXXX
//SelectObject/DeleteObject偼巊梡偱偒側偔側傞

#ifdef _DEBUG
#undef DeletePen
#undef DeleteBrush
#undef DeleteRgn
#undef DeleteFont
#undef DeleteBitmap
#undef SelectPen
#undef SelectBrush
#undef SelectRgn
#undef SelectFont
#undef SelectBitmap

#define _IsValidPen(hPen)		\
	(hPen == NULL || ::GetObjectType(hPen) == OBJ_PEN || ::GetObjectType(hPen) == OBJ_EXTPEN)
#define _IsValidBrush(hBrush)	\
	(hBrush == NULL || ::GetObjectType(hBrush) == OBJ_BRUSH)
#define _IsValidRgn(hRgn)		\
	(hRgn == NULL || ::GetObjectType(hRgn) == OBJ_REGION)
#define _IsValidFont(hFont)		\
	(hFont == NULL || ::GetObjectType(hFont) == OBJ_FONT)
#define _IsValidBitmap(hBitmap)	\
	(hBitmap == NULL || ::GetObjectType(hBitmap) == OBJ_BITMAP)

#define DEFINE_DELETE_FUNCTION(type, name) \
	FORCEINLINE BOOL WINAPI Delete##name(type h##name) \
	{ \
		_ASSERTE(_IsValid##name(h##name)); \
		return ::DeleteObject(h##name); \
	}

#define DEFINE_SELECT_FUNCTION(type, name) \
	FORCEINLINE type WINAPI Select##name(HDC hDC, type h##name) \
	{ \
		_ASSERTE(hDC != NULL); \
		if (!_IsValid##name(h##name)) { \
		TRACE(_T("Select object %x for DC %x"), (DWORD)h##name, (DWORD)hDC); \
			}; \
		return (type)::SelectObject(hDC, h##name); \
	}

DEFINE_DELETE_FUNCTION(HPEN,	Pen)
DEFINE_DELETE_FUNCTION(HBRUSH,	Brush)
DEFINE_DELETE_FUNCTION(HRGN,	Rgn)
DEFINE_DELETE_FUNCTION(HFONT,	Font)
DEFINE_DELETE_FUNCTION(HBITMAP,	Bitmap)

DEFINE_SELECT_FUNCTION(HPEN,	Pen)
DEFINE_SELECT_FUNCTION(HBRUSH,	Brush)
DEFINE_SELECT_FUNCTION(HRGN,	Rgn)
DEFINE_SELECT_FUNCTION(HFONT,	Font)
DEFINE_SELECT_FUNCTION(HBITMAP,	Bitmap)

#undef _IsValidPen
#undef _IsValidBrush
#undef _IsValidRgn
#undef _IsValidFont
#undef _IsValidBitmap
#undef DEFINE_DELETE_FUNCTION
#undef DEFINE_SELECT_FUNCTION

#if (_MSC_VER >= 1300)
#pragma deprecated(DeleteObject)
#pragma deprecated(SelectObject)
#else	//_MSC_VER < 1300
#undef DeleteObject
#define DeleteObject		DeleteObject_instead_use_DeleteXXX
#undef SelectObject
#define SelectObject		SelectObject_instead_use_DeleteXXX
#endif	//_MSC_VER

#else	//!_DEBUG
#ifndef _INC_WINDOWSX
#define DeletePen			::DeleteObject
#define DeleteBrush			::DeleteObject
#define DeleteRgn			::DeleteObject
#define DeleteFont			::DeleteObject
#define DeleteBitmap		::DeleteObject

#define SelectPen(d,o)		(HPEN)::SelectObject(d,o)
#define SelectBrush(d,o)	(HBRUSH)::SelectObject(d,o)
#define SelectRgn(d,o)		(HRGN)::SelectObject(d,o)
#define SelectFont(d,o)		(HFONT)::SelectObject(d,o)
#define SelectBitmap(d,o)	(HBITMAP)::SelectObject(d,o)
#endif	//!_INC_WINDOWSX
#endif	//_DEBUG


//TRACE儅僋儘
//巊梡椺: TRACE(_T("cx: %d\n"), cx);
#ifndef _WIN64
#ifdef _DEBUG
FORCEINLINE static __int64 GetClockCount()
{
	LARGE_INTEGER cycles;
	__asm {
		rdtsc
		mov cycles.LowPart,  eax
		mov cycles.HighPart, edx
	}
	return cycles.QuadPart;
}

//巊梡椺
//{
//   CDebugElapsedCounter _cntr("hogehoge");
//     : (揔摉側張棟)
//}
//弌椡椺: "hogehoge: 10000 clocks"
class CDebugElapsedCounter
{
private:
	__int64 m_ilClk;
	LPCSTR m_pszName;
public:
	CDebugElapsedCounter(LPCSTR psz)
		: m_ilClk(GetClockCount())
		, m_pszName(psz)
	{
	}
	~CDebugElapsedCounter()
	{
		TRACE(_T("%hs: %u clocks\n"), m_pszName, (DWORD)(GetClockCount() - m_ilClk));
	}
};
#else
class CDebugElapsedCounter
{
public:
	CDebugElapsedCounter(LPCSTR psz)
	{
	}
};
#endif
#endif


//String to int等系列函数的定义
/*
int _StrToInt(LPCTSTR pStr, int nDefault)
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

int _httoi(const TCHAR *value)
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
float _StrToFloat(LPCTSTR pStr, float fDefault)
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
}*/

void HookD2DDll();
bool HookD2D1();
void HookGdiplus();