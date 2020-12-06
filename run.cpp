// dll injection
#define _CRT_SECURE_NO_DEPRECATE 1
#define WINVER 0x500
#define _WIN32_WINNT 0x500
#define _WIN32_IE 0x601
#define WIN32_LEAN_AND_MEAN 1
#define UNICODE  1
#define _UNICODE 1
#include <Windows.h>
#include <ShellApi.h>
#include <ComDef.h>
#include <ShlObj.h>
#include <ShLwApi.h>
#include <tchar.h>
#include "array.h"
#include <strsafe.h>
#include "gdiexe.rc"

// _vsnwprintf用
#include <wchar.h>		
#include <stdarg.h>

#define _CRTDBG_MAP_ALLOC
#include <cstdlib>
#include <malloc.h>
#include <crtdbg.h>

#define for if(0);else for
#ifndef _countof
#define _countof(array)		(sizeof(array) / sizeof((array)[0]))
#endif

#pragma comment(linker, "/subsystem:windows,5.0")
#pragma comment(lib, "Kernel32.lib")
#pragma comment(lib, "User32.lib")
#pragma comment(lib, "Shell32.lib")
#pragma comment(lib, "ShLwApi.lib")
#pragma comment(lib, "Ole32.lib")

static void errmsg(UINT id, DWORD code)
{
	char  buffer [512];
	char  format [128];
	LoadStringA(GetModuleHandleA(NULL), id, format, 128);
	wnsprintfA(buffer, 512, format, code);
	MessageBoxA(NULL, buffer, "MacType ERROR", MB_OK|MB_ICONSTOP);
}

inline HRESULT HresultFromLastError()
{
	DWORD dwErr = GetLastError();
	return HRESULT_FROM_WIN32(dwErr);
}


//#include <detours.h>

HINSTANCE hinstDLL;

#include <stddef.h>
#define GetDLLInstance()	(hinstDLL)

#define _GDIPP_EXE
#define _GDIPP_RUN_CPP
#include "supinfo.h"

static BOOL (WINAPI * ORIG_CreateProcessW)(LPCWSTR lpApp, LPWSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCWSTR lpDir, LPSTARTUPINFOW psi, LPPROCESS_INFORMATION ppi)
	= CreateProcessW;

static BOOL WINAPI IMPL_CreateProcessW(LPCWSTR lpApp, LPWSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, LPCWSTR lpDir, LPSTARTUPINFOW psi, LPPROCESS_INFORMATION ppi)
{
	return _CreateProcessAorW(lpApp, lpCmd, pa, ta, bInherit, dwFlags, lpEnv, lpDir, psi, ppi, ORIG_CreateProcessW);
}


//#define OLD_PSDK

#ifdef OLD_PSDK
extern "C" {
HRESULT WINAPI _SHILCreateFromPath(LPCWSTR pszPath, LPITEMIDLIST *ppidl, DWORD *rgflnOut)
{
	if (!pszPath || !ppidl) {
		return E_INVALIDARG;
	}

	LPSHELLFOLDER psf;
	HRESULT hr = ::SHGetDesktopFolder(&psf);
	if (hr != NOERROR) {
		return hr;
	}

	ULONG chEaten;
	LPOLESTR lpszDisplayName = ::StrDupW(pszPath);
	hr = psf->ParseDisplayName(NULL, NULL, lpszDisplayName, &chEaten, ppidl, rgflnOut);
	::LocalFree(lpszDisplayName);
	psf->Release();
	return hr;
}

void WINAPI _SHFree(void *pv)
{
	if (!pv) {
		return;
	}

	LPMALLOC pMalloc = NULL;
	if (::SHGetMalloc(&pMalloc) == NOERROR) {
		pMalloc->Free(pv);
		pMalloc->Release();
	}
}
}
#else
#define _SHILCreateFromPath	SHILCreateFromPath
#define _SHFree				SHFree
#endif


// １つ目の引数だけファイルとして扱い、実行する。
//
// コマンドは こんな感じで連結されます。
//  exe linkpath linkarg cmdarg2 cmdarg3 cmdarg4 ...
//
static HRESULT HookAndExecute(int show)
{
	int     argc = 0;
	LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
	if(!argv) {
		return HresultFromLastError();
	}
	if(argc <= 1) {
		char buffer [256];
		LoadStringA(GetModuleHandleA(NULL), IDS_USAGE, buffer, 256);
		MessageBoxA(NULL,
			buffer
			,"MacType", MB_OK|MB_ICONINFORMATION);
		LocalFree(argv);
		return S_OK;
	}

	int i;
	size_t length = 1;
	for(i=2; i<argc; i++) {
		length += wcslen(argv[i]) + 3;
	}

	LPWSTR cmdline = (WCHAR*)calloc(sizeof(WCHAR), length);
	if(!cmdline) {
		LocalFree(argv);
		return E_OUTOFMEMORY;
	}

	LPWSTR p = cmdline;
	*p = L'\0';
	for(i=2; i<argc; i++) {
		const bool dq = !!wcschr(argv[i], L' ');
		if (dq) {
			*p++ = '"';
			length--;
		}
		StringCchCopyExW(p, length, argv[i], &p, &length, STRSAFE_NO_TRUNCATION);
		if (dq) {
			*p++ = '"';
			length--;
		}
		*p++ = L' ';
		length--;
	}

	*CharPrevW(cmdline, p) = L'\0';

	WCHAR file[MAX_PATH], dir[MAX_PATH];
	GetCurrentDirectoryW(_countof(dir), dir);
	StringCchCopyW(file, _countof(file), argv[1]);
	if(PathIsRelativeW(file)) {
		PathCombineW(file, dir, file);
	} else {
		WCHAR gdippDir[MAX_PATH];
		GetModuleFileNameW(NULL, gdippDir, _countof(gdippDir));
		PathRemoveFileSpec(gdippDir);

		// カレントディレクトリがgdi++.exeの置かれているディレクトリと同じだったら、
		// 起動しようとしているEXEのフルパスから抜き出したディレクトリ名をカレント
		// ディレクトリとして起動する。(カレントディレクトリがEXEと同じ場所である
		// 前提で作られているアプリ対策)
		if (wcscmp(dir, gdippDir) == 0) {
			StringCchCopyW(dir, _countof(dir), argv[1]);
			PathRemoveFileSpec(dir);
		}
	}

	LocalFree(argv);
	argv = NULL;

#ifdef _DEBUG
	if((GetAsyncKeyState(VK_CONTROL) & 0x8000)
			&& MessageBoxW(NULL, cmdline, NULL, MB_YESNO) != IDYES) {
		free(cmdline);
		return NOERROR;
	}
#endif

	LPITEMIDLIST pidl = NULL;
	HRESULT hr;

	//fileのアイテムIDリストを取得
	hr = _SHILCreateFromPath(file, &pidl, NULL);
	if(SUCCEEDED(hr) && pidl) {
		//SEE_MASK_INVOKEIDLISTを使うと
		//explorerでクリックして起動したのと同じ動作になる
		SHELLEXECUTEINFOW sei = { sizeof(SHELLEXECUTEINFOW) };
		sei.fMask			= SEE_MASK_INVOKEIDLIST
								| SEE_MASK_CONNECTNETDRV
								| SEE_MASK_FLAG_DDEWAIT
								| SEE_MASK_DOENVSUBST
								| SEE_MASK_WAITFORINPUTIDLE;
		sei.hwnd			= GetDesktopWindow();
		sei.lpParameters	= cmdline;
		sei.lpDirectory		= dir;
		sei.nShow			= show;
		sei.lpIDList		= pidl;

		//ShellExecuteExWが内部で呼び出すCreateProcessWをフックして
		//HookChildProcesses相当の処理を行う

		DetourTransactionBegin();
		DetourUpdateThread(GetCurrentThread());
		DetourAttach(&(PVOID&)ORIG_CreateProcessW, IMPL_CreateProcessW);
		hr = DetourTransactionCommit();

		if(hr == NOERROR) {
			if(ShellExecuteExW(&sei)) {
				hr = S_OK;
			} else {
				hr = HresultFromLastError();
			}
		}

		DetourTransactionBegin();
		DetourUpdateThread(GetCurrentThread());
		DetourDetach(&(PVOID&)ORIG_CreateProcessW, IMPL_CreateProcessW);
		DetourTransactionCommit();
	}

	if(pidl)
		_SHFree(pidl);
	free(cmdline);
	return hr;
}


int WINAPI wWinMain(HINSTANCE ins, HINSTANCE prev, LPWSTR cmd, int show)
{
	_CrtSetDbgFlag(_CrtSetDbgFlag(_CRTDBG_REPORT_FLAG) | _CRTDBG_LEAK_CHECK_DF);
	OleInitialize(NULL);

	WCHAR path [MAX_PATH];
	if(GetModuleFileNameW(NULL, path, _countof(path))) {
		PathRenameExtensionW(path, L".dll");
		//DONT_RESOLVE_DLL_REFERENCESを指定すると依存関係の解決や
		//DllMainの呼び出しが行われない
		hinstDLL = LoadLibraryExW(path, NULL, DONT_RESOLVE_DLL_REFERENCES);
	}
	if(!hinstDLL) {
		errmsg(IDS_DLL, HresultFromLastError());
	} else {
		PathRemoveFileSpecW(path);
		SetCurrentDirectoryW(path);

		HRESULT hr = HookAndExecute(show);
		if(hr != S_OK) {
			errmsg(IDC_EXEC, hr);
		}
	}

	OleUninitialize();
	return 0;
}

//EOF
