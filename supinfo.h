#pragma once

#include <mmsystem.h>	//mmioFOURCC
#define FOURCC_GDIPP	mmioFOURCC('G', 'D', 'I', 'P')

typedef struct {
	int dummy;
	FOURCC magic;
//	BYTE reserved[256];
} GDIPP_CREATE_MAGIC;

//嶲徠
//http://www.catch22.net/tuts/undoc01.asp

#ifdef _GDIPP_EXE
template <typename _STARTUPINFO>
void FillGdiPPStartupInfo(_STARTUPINFO& si, GDIPP_CREATE_MAGIC& gppcm)
{
	ZeroMemory(&gppcm, sizeof(GDIPP_CREATE_MAGIC));
	gppcm.magic = FOURCC_GDIPP;
	si.cbReserved2 = sizeof(GDIPP_CREATE_MAGIC);
	si.lpReserved2 = (LPBYTE)&gppcm;
}
#endif

#ifdef _GDIPP_DLL
template <typename _STARTUPINFO>
bool IsGdiPPStartupInfo(const _STARTUPINFO& si)
{
	if(si.cbReserved2 >= sizeof(int) + sizeof(FOURCC)) {
		GDIPP_CREATE_MAGIC* pMagic = (GDIPP_CREATE_MAGIC*)si.lpReserved2;
		if (pMagic->dummy == 0 && pMagic->magic == FOURCC_GDIPP) {
			return true;
		}
	}
	return false;
}
#endif

EXTERN_C BOOL WINAPI GdippInjectDLL(const PROCESS_INFORMATION* ppi);
EXTERN_C LPWSTR WINAPI GdippEnvironment(DWORD& dwCreationFlags, LPVOID lpEnvironment);



//巕僾儘僙僗偵傕帺摦偱gdi++揔梡
template <typename _TCHAR, typename _STARTUPINFO, class _Function>
BOOL _CreateProcessAorW(const _TCHAR* lpApp, _TCHAR* lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, const _TCHAR* lpDir, _STARTUPINFO* psi, LPPROCESS_INFORMATION ppi, _Function fn)
{
#ifdef _GDIPP_RUN_CPP
	const bool hookCP = true;
	const bool runGdi = true;
#else
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	const bool hookCP = pSettings->HookChildProcesses();
	const bool runGdi = pSettings->RunFromGdiExe();
#endif
	
	if (!hookCP || (!lpApp && !lpCmd) || (dwFlags & (DEBUG_PROCESS | DEBUG_ONLY_THIS_PROCESS | DETACHED_PROCESS))/* || !psi || psi->cb < sizeof(_STARTUPINFO)*/) {
		return fn(lpApp, lpCmd, pa, ta, bInherit, dwFlags, lpEnv, lpDir, psi, ppi);
	}

	PROCESS_INFORMATION _pi = { 0 };
	if (!ppi) {
		ppi = &_pi;
	}

	int szpsi = psi ? (psi->cb ? psi->cb: sizeof(_STARTUPINFO)) : sizeof(_STARTUPINFO);
	_STARTUPINFO& si = *(_STARTUPINFO*)_alloca(szpsi);
	memset(&si, 0, sizeof(si));
	si.cb=szpsi;
	if (psi && psi->cb)
		memcpy(&si, psi, psi->cb);
	psi = &si;

	GDIPP_CREATE_MAGIC gppcm;
	if (runGdi && !si.cbReserved2) {
		FillGdiPPStartupInfo(si, gppcm);
	}

	LPWSTR pEnvW = GdippEnvironment(dwFlags, lpEnv);
	if (pEnvW) {
		lpEnv = pEnvW;
	}

	if (!fn(lpApp, lpCmd, pa, ta, bInherit, dwFlags | CREATE_SUSPENDED, lpEnv, lpDir, &si, ppi)) {
		ZeroMemory(ppi, sizeof(*ppi));
		free(pEnvW);
		return FALSE;
	}

	GdippInjectDLL(ppi);
	if (!(dwFlags & CREATE_SUSPENDED)) {
		ResumeThread(ppi->hThread);
	}
	free(pEnvW);
	return TRUE;
}

template <typename _TCHAR, typename _STARTUPINFO, class _Function>
BOOL _CreateProcessAsUserAorW(HANDLE hToken, const _TCHAR* lpApp, _TCHAR* lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, DWORD dwFlags, LPVOID lpEnv, const _TCHAR* lpDir, _STARTUPINFO* psi, LPPROCESS_INFORMATION ppi, _Function fn)
{
#ifdef _GDIPP_RUN_CPP
	const bool hookCP = true;
	const bool runGdi = true;
#else
	const CGdippSettings* pSettings = CGdippSettings::GetInstance();
	const bool hookCP = pSettings->HookChildProcesses();
	const bool runGdi = pSettings->RunFromGdiExe();
#endif

	if (!hookCP || (!lpApp && !lpCmd) || (dwFlags & (DEBUG_PROCESS | DEBUG_ONLY_THIS_PROCESS))/* || !psi || psi->cb < sizeof(_STARTUPINFO)*/) {
		return fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags, lpEnv, lpDir, psi, ppi);
	}

	PROCESS_INFORMATION _pi = { 0 };
	if (!ppi) {
		ppi = &_pi;
	}
	int szpsi = psi ? (psi->cb ? psi->cb: sizeof(_STARTUPINFO)) : sizeof(_STARTUPINFO);
	_STARTUPINFO& si = *(_STARTUPINFO*)_alloca(szpsi);
	memset(&si, 0, sizeof(si));
	si.cb=szpsi;
	if (psi && psi->cb)
		memcpy(&si, psi, psi->cb);
	psi = &si;

	GDIPP_CREATE_MAGIC gppcm;
	if (runGdi && !si.cbReserved2) {
		FillGdiPPStartupInfo(si, gppcm);
	}

	LPWSTR pEnvW = GdippEnvironment(dwFlags, lpEnv);
	if (pEnvW) {
		lpEnv = pEnvW;
	}

	if (!fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags | CREATE_SUSPENDED, lpEnv, lpDir, &si, ppi)) {
		ZeroMemory(ppi, sizeof(*ppi));
		free(pEnvW);
		return FALSE;
	}

	GdippInjectDLL(ppi);
	if (!(dwFlags & CREATE_SUSPENDED)) {
		ResumeThread(ppi->hThread);
	}
	free(pEnvW);
	return TRUE;
}

static wstring GetExeName(LPCTSTR lpApp, LPTSTR lpCmd)
{
// 	HANDLE logfile = CreateFile(_T("C:\\mt.log"), FILE_ALL_ACCESS, FILE_SHARE_READ|FILE_SHARE_WRITE, NULL, OPEN_ALWAYS, NULL, NULL);
// 	SetFilePointer(logfile,0,NULL, FILE_END);

	wstring ret;
// 	DWORD aa=0;
// 	if (GetFileSize(logfile, NULL)==0)
// 		WriteFile(logfile, "\xff\xfe", 2, &aa, NULL);
	LPTSTR vlpApp = (LPTSTR)lpApp;	//变成可以操作的参数
	if (lpApp)
	{
		do 
		{
// 			WriteFile(logfile, L"lpApp=", 12, &aa, NULL);
// 			WriteFile(logfile, lpApp, _tcslen(lpApp)*2, &aa, NULL);
// 			WriteFile(logfile, _T("\n"), 2, &aa, NULL);
			vlpApp = _tcsstr(vlpApp+1, _T(" "));	//获得第一个空格所在的位置
			ret.assign(lpApp);
			if (vlpApp)
				ret.resize(vlpApp-lpApp);
// 			WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 			WriteFile(logfile, _T("\n"), 2, &aa, NULL);
			DWORD fa = GetFileAttributes(ret.c_str()); 
			if (fa!=INVALID_FILE_ATTRIBUTES && fa!=FILE_ATTRIBUTE_DIRECTORY)	//文件是否存在
			{		
				int p = ret.find_last_of(_T("\\"));
				if (p!=-1)
					ret.erase(0, p+1);	//如果有路径就删掉路径
// 				WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 				WriteFile(logfile, _T("\n"), 2, &aa, NULL);
// 				WriteFile(logfile, _T("==========\n"), 24, &aa, NULL);
// 				CloseHandle(logfile);
				return ret;
			}
			else
			{
				ret+=_T(".exe");	//加上.exe扩展名再试
				DWORD fa = GetFileAttributes(ret.c_str()); 
				if (fa!=INVALID_FILE_ATTRIBUTES && fa!=FILE_ATTRIBUTE_DIRECTORY)
				{		
					int p = ret.find_last_of(_T("\\"));
					if (p!=-1)
						ret.erase(0, p+1);	//如果有路径就删掉路径
// 					WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 					WriteFile(logfile, _T("\n"), 2, &aa, NULL);
// 					WriteFile(logfile, _T("==========\n"), 24, &aa, NULL);
// 					CloseHandle(logfile);
					return ret;
				}
			}
		} while (vlpApp);
	}

	if (lpCmd)
	{
// 		WriteFile(logfile, L"lpCmd=", 10, &aa, NULL);
// 		WriteFile(logfile, lpCmd, _tcslen(lpCmd)*2, &aa, NULL);
		ret.assign(lpCmd);
		int p=0;
		if ((*lpCmd)==_T('\"'))
		{
			ret.erase(0,1);	//删除第一个引号
			p=ret.find_first_of(_T("\""));	//查找下一个引号
		}
		else
			p=ret.find_first_of(_T(" "));
		if (p>0)
			ret.resize(p);	//获得Cmd里面的文件名
// 		WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 		WriteFile(logfile, _T("\n"), 2, &aa, NULL);
		p = ret.find_last_of(_T("\\"));
		if (p>0)
			ret.erase(0, p+1);	//如果有路径就删掉路径
// 		WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 		WriteFile(logfile, _T("\n"), 2, &aa, NULL);
// 		WriteFile(logfile, _T("==========\n"), 24, &aa, NULL);
// 		CloseHandle(logfile);
		return ret;
	}
// 	WriteFile(logfile, ret.c_str(), ret.length()*2, &aa, NULL);
// 	WriteFile(logfile, _T("\n"), 2, &aa, NULL);
// 	WriteFile(logfile, _T("==========\n"), 24, &aa, NULL);
// 	CloseHandle(logfile);
	return ret;
}

template <class _Function>
BOOL _CreateProcessInternalW(HANDLE hToken, LPCTSTR lpApp, LPTSTR lpCmd, LPSECURITY_ATTRIBUTES pa, LPSECURITY_ATTRIBUTES ta, BOOL bInherit, \
							 DWORD dwFlags, LPVOID lpEnv, LPCTSTR lpDir, LPSTARTUPINFO psi, LPPROCESS_INFORMATION ppi , PHANDLE hNewToken, _Function fn)
{
#ifdef _GDIPP_RUN_CPP
	const bool hookCP = true;
	const bool runGdi = true;
#else
	const CGdippSettings* pSettings = CGdippSettings::GetInstanceNoInit();
	const bool hookCP = pSettings->HookChildProcesses();
	const bool runGdi = pSettings->RunFromGdiExe();
#endif

#ifdef _GDIPP_EXE
	if (!hookCP || (!lpApp && !lpCmd) || (dwFlags & (DEBUG_PROCESS | DEBUG_ONLY_THIS_PROCESS))	|| !psi || psi->cb < sizeof(STARTUPINFO)) {
			return fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags, lpEnv, lpDir, psi, ppi, hNewToken);
	}

	STARTUPINFO& si = *(STARTUPINFO*)_alloca(psi->cb);
	memcpy(&si, psi, psi->cb);
	psi = &si;

	GDIPP_CREATE_MAGIC gppcm;
	if (runGdi && !si.cbReserved2) {
		FillGdiPPStartupInfo(si, gppcm);
	}

	PROCESS_INFORMATION _pi = { 0 };
	if (!ppi) {
		ppi = &_pi;
	}

	LPWSTR pEnvW = GdippEnvironment(dwFlags, lpEnv);
	if (pEnvW) {
		lpEnv = pEnvW;
	}
	if (!fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags | CREATE_SUSPENDED, lpEnv, lpDir, psi, ppi, hNewToken)) {
		ZeroMemory(ppi, sizeof(*ppi));
		free(pEnvW);
		return FALSE;
	}
	GdippInjectDLL(ppi);
	if (!(dwFlags & CREATE_SUSPENDED)) {
		ResumeThread(ppi->hThread);
	}
	free(pEnvW);
#else
	wstring exe_name = GetExeName(lpApp, lpCmd);
	if (!hookCP || (!lpApp && !lpCmd) || (dwFlags & (DEBUG_PROCESS | DEBUG_ONLY_THIS_PROCESS)) || IsExeUnload(exe_name.c_str())) {
			return fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags, lpEnv, lpDir, psi, ppi, hNewToken);
	}
	LPWSTR pEnvW = GdippEnvironment(dwFlags, lpEnv);
	if (pEnvW) {
		lpEnv = pEnvW;
	}
	if (!fn(hToken, lpApp, lpCmd, pa, ta, bInherit, dwFlags | CREATE_SUSPENDED, lpEnv, lpDir, psi, ppi, hNewToken)) {
		free(pEnvW);
		return FALSE;
	}
	GdippInjectDLL(ppi);
	if (!(dwFlags & CREATE_SUSPENDED)) {
		ResumeThread(ppi->hThread);
	}
	free(pEnvW);
#endif

	return TRUE;
}