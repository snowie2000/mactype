#ifndef _GDIPP_EXE
#include "settings.h"
#include "override.h"
#include <tlhelp32.h>
#include <shlwapi.h>	//DLLVERSIONINFO
#include "undocAPI.h"
#include <windows.h>
#include <dwrite_1.h>
#include <dwrite_2.h>
#include <dwrite_3.h>
#include "wow64ext.h"
#include <VersionHelpers.h>

// win2k埲崀
//#pragma comment(linker, "/subsystem:windows,5.0")
#ifndef _WIN64
#pragma comment(lib, "wow64ext.lib")
#endif

EXTERN_C LRESULT CALLBACK GetMsgProc(int code, WPARAM wParam, LPARAM lParam)
{
	//壗傕偟側偄
	return CallNextHookEx(NULL, code, wParam, lParam);
}

EXTERN_C HRESULT WINAPI GdippDllGetVersion(DLLVERSIONINFO* pdvi)
{
	if (!pdvi || pdvi->cbSize < sizeof(DLLVERSIONINFO)) {
		return E_INVALIDARG;
	}

	const UINT cbSize = pdvi->cbSize;
	ZeroMemory(pdvi, cbSize);
	pdvi->cbSize = cbSize;

	HRSRC hRsrc = FindResource(GetDLLInstance(), MAKEINTRESOURCE(VS_VERSION_INFO), RT_VERSION);
	if (!hRsrc) {
		return E_FAIL;
	}

	HGLOBAL hGlobal = LoadResource(GetDLLInstance(), hRsrc);
	if (!hGlobal) {
		return E_FAIL;
	}

	const WORD* lpwPtr = (const WORD*)LockResource(hGlobal);
	if (lpwPtr[1] != sizeof(VS_FIXEDFILEINFO)) {
		return E_FAIL;
	}

	const VS_FIXEDFILEINFO* pvffi = (const VS_FIXEDFILEINFO*)(lpwPtr + 20);
	if (pvffi->dwSignature != VS_FFI_SIGNATURE ||
			pvffi->dwStrucVersion != VS_FFI_STRUCVERSION) {
		return E_FAIL;
	}

	//8.0.2006.1027
	// -> Major: 8, Minor: 2006, Build: 1027
	pdvi->dwMajorVersion	= HIWORD(pvffi->dwFileVersionMS);
	pdvi->dwMinorVersion	= LOWORD(pvffi->dwFileVersionMS) * 10 + HIWORD(pvffi->dwFileVersionLS);
	pdvi->dwBuildNumber		= LOWORD(pvffi->dwFileVersionLS);
	pdvi->dwPlatformID		= DLLVER_PLATFORM_NT;

	if (pdvi->cbSize < sizeof(DLLVERSIONINFO2)) {
		return S_OK;
	}

	DLLVERSIONINFO2* pdvi2 = (DLLVERSIONINFO2*)pdvi;
	pdvi2->ullVersion		= MAKEDLLVERULL(pdvi->dwMajorVersion, pdvi->dwMinorVersion, pdvi->dwBuildNumber, 2);
	return S_OK;
}

#endif	//!_GDIPP_EXE

extern LONG interlock;
extern LONG g_bHookEnabled;
#include "gdiPlusFlat2.h"

#ifdef USE_DETOURS
#include "detours.h"
#define HOOK_DEFINE(rettype, name, argtype) \
	DetourDetach(&(PVOID&)ORIG_##name, IMPL_##name);
static LONG hook_term()
{
	DetourTransactionBegin();

	DetourUpdateThread(GetCurrentThread());

#include "hooklist.h"

	LONG error = DetourTransactionCommit();

	if (error != NOERROR) {
		TRACE(_T("hook_term error: %#x\n"), error);
	}
	return error;
}
#undef HOOK_DEFINE
#else
#include "easyhook.h"
#define HOOK_MANUALLY(rettype, name, argtype) ;
#define HOOK_DEFINE(rettype, name, argtype) \
	ORIG_##name = name;
#pragma optimize("s", on)
static LONG hook_term()
{
#include "hooklist.h"
	LhUninstallAllHooks();
	return LhWaitForPendingRemovals();
}
#pragma optimize("", on)
#undef HOOK_DEFINE
#undef HOOK_MANUALLY
#endif

HMODULE GetSelfModuleHandle()
{
	MEMORY_BASIC_INFORMATION mbi;

	return ((::VirtualQuery(GetSelfModuleHandle, &mbi, sizeof(mbi)) != 0) 
		? (HMODULE) mbi.AllocationBase : NULL);
}

EXTERN_C void WINAPI CreateControlCenter(IControlCenter** ret)
{
	*ret = (IControlCenter*)new CControlCenter;
}

EXTERN_C void WINAPI ReloadConfig()
{
	CControlCenter::ReloadConfig();
}

extern HINSTANCE g_dllInstance;
EXTERN_C void SafeUnload()
{
	static BOOL bInited = false;
	if (bInited)
		return;	//防重入
	bInited = true;
	while (CThreadCounter::Count())
		Sleep(0);
	CCriticalSectionLock * lock = new CCriticalSectionLock;
	BOOL last;
	if (last=InterlockedExchange(&g_bHookEnabled, FALSE)) {
		if (hook_term()!=NOERROR)
		{
			InterlockedExchange(&g_bHookEnabled, last);
			bInited = false;
			delete lock;
			ExitThread(ERROR_ACCESS_DENIED);
		}
	}
	delete lock;
	while (CThreadCounter::Count())
		Sleep(10);
	Sleep(0);
	do 
	{
		Sleep(10);
	} while (CThreadCounter::Count());	//double check for xp
		
	bInited = false; 
	FreeLibraryAndExitThread(g_dllInstance, 0);
}

#ifndef Assert
#include <crtdbg.h>
#define Assert	_ASSERTE
#endif	//!Assert

#include "array.h"
#include <strsafe.h>
#include <shlwapi.h>
#include "dll.h"

//kernel32愱梡GetProcAddress儌僪僉
FARPROC K32GetProcAddress(LPCSTR lpProcName)
{
#ifndef _WIN64
	//彉悢搉偟偵偼懳墳偟側偄
	Assert(!IS_INTRESOURCE(lpProcName));

	//kernel32偺儀乕僗傾僪儗僗庢摼
	LPBYTE pBase = (LPBYTE)GetModuleHandleA("kernel32.dll");

	//偙偺曈偼100%惉岟偡傞偼偢側偺偱僄儔乕僠僃僢僋偟側偄
	PIMAGE_DOS_HEADER pdosh = (PIMAGE_DOS_HEADER)pBase;
	Assert(pdosh->e_magic == IMAGE_DOS_SIGNATURE);
	PIMAGE_NT_HEADERS pnth = (PIMAGE_NT_HEADERS)(pBase + pdosh->e_lfanew);
	Assert(pnth->Signature == IMAGE_NT_SIGNATURE);

	const DWORD offs = pnth->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT].VirtualAddress;
	const DWORD size = pnth->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT].Size;
	if (offs == 0 || size == 0) {
		return NULL;
	}

	PIMAGE_EXPORT_DIRECTORY pdir = (PIMAGE_EXPORT_DIRECTORY)(pBase + offs);
	DWORD*	pFunc = (DWORD*)(pBase + pdir->AddressOfFunctions);
	WORD*	pOrd  = (WORD*)(pBase + pdir->AddressOfNameOrdinals);
	DWORD*	pName = (DWORD*)(pBase + pdir->AddressOfNames);

	for(DWORD i=0; i<pdir->NumberOfFunctions; i++) {
		for(DWORD j=0; j<pdir->NumberOfNames; j++) {
			if(pOrd[j] != i)
				continue;

			if(strcmp((LPCSTR)pBase + pName[j], lpProcName) != 0)
				continue;

			return (FARPROC)(pBase + pFunc[i]);
		}
	}
	return NULL;
#else
	Assert(!IS_INTRESOURCE(lpProcName));

	//kernel32偺儀乕僗傾僪儗僗庢摼
	WCHAR sysdir[MAX_PATH];
	GetWindowsDirectory(sysdir, MAX_PATH);
	if (GetModuleHandle(_T("kernelbase.dll")))	//查看自己是否加载了Kernelbase.dll文件，存在则说明是win7系统
		wcscat(sysdir, L"\\SysWow64\\kernelbase.dll");
	else
		wcscat(sysdir, L"\\SysWow64\\kernel32.dll");	//不存在就是vista
	HANDLE hFile = CreateFile(sysdir, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, NULL, NULL);
	if (hFile == INVALID_HANDLE_VALUE)
		return NULL;
	DWORD dwSize = GetFileSize(hFile, NULL);
	BYTE* pMem = new BYTE[dwSize];	//分配内存
	ReadFile(hFile, pMem, dwSize, &dwSize, NULL);//读取文件
	CloseHandle(hFile);

	CMemLoadDll MemDll;
	MemDll.MemLoadLibrary(pMem, dwSize, false, false);
	delete[] pMem;
	return FARPROC((DWORD_PTR)MemDll.MemGetProcAddress(lpProcName)-MemDll.GetImageBase());	//返回偏移值

#endif
}


#include <pshpack1.h>
class opcode_data {
private:
	BYTE	code[0x100];

	//拲: dllpath傪WORD嫬奅偵偟側偄偲応崌偵傛偭偰偼惓忢偵摦嶌偟側偄
	WCHAR	dllpath[MAX_PATH];

public:
	opcode_data()
	{
		//int 03h偱杽傔傞
		FillMemory(this, sizeof(*this), 0xcc);
	}
	bool initWow64(LPDWORD remoteaddr, LONG orgEIP)	//Wow64初始化
	{
		//WORD嫬奅僠僃僢僋
		C_ASSERT((offsetof(opcode_data, dllpath) & 1) == 0);

		register BYTE* p = code;

#define emit_(t,x)	*(t* UNALIGNED)p = (t)(x); p += sizeof(t)
#define emit_db(b)	emit_(BYTE, b)
#define emit_dw(w)	emit_(WORD, w)
#define emit_dd(d)	emit_(DWORD, d)

		//側偤偐GetProcAddress偱LoadLibraryW偺傾僪儗僗偑惓偟偔庢傟側偄偙偲偑偁傞偺偱
		//kernel32偺僿僢僟偐傜帺慜偱庢摼偡傞
		FARPROC pfn = K32GetProcAddress("LoadLibraryExW");
		if(!pfn)
			return false;

		emit_db(0x60);		//pushad

		/*
			mov eax,fs:[0x30]
			mov eax,[eax+0x0c]
			mov esi,[eax+0x1c]
			lodsd
			move ax,[eax+$08]//这个时候eax中保存的就是k32的基址了
			在win7获得的是KernelBase.dll的地址
		*/
		emit_db(0x64); 
		emit_db(0xA1); 
		emit_db(0x30); 
		emit_db(00); 
		emit_db(00); 
		emit_db(00); 
		emit_db(0x8B); 
		emit_db(0x40); 
		emit_db(0x0C); 
		emit_db(0x8B); 
		emit_db(0x70); 
		emit_db(0x1C); 
		emit_db(0xAD); 
		emit_db(0x8B); 
		emit_db(0x40);
		emit_db(0x08);		//use assemble to fetch kernel base

		emit_dw(0x006A);	//push 0
		emit_dw(0x006A);	//push 0
		emit_db(0x68);		//push dllpath
		emit_dd((LONG)remoteaddr + offsetof(opcode_data, dllpath));
		emit_db(0x05);		//add eax, LoadLibraryExW offset
		emit_dd(pfn);
		emit_dw(0xD0FF);	//call eax

		emit_db(0x61);		//popad
		emit_db(0xE9);		//jmp original_EIP
		emit_dd(orgEIP - (LONG)remoteaddr - (p - code) - sizeof(LONG));

		// gdi++.dll偺僷僗
		bool bDll = !!GetModuleFileNameW(GetDLLInstance(), dllpath, MAX_PATH);
		if (bDll && wcsstr(dllpath, L"64"))
			wcscpy(wcsstr(dllpath, L"64"), wcsstr(dllpath, L"64")+2);
		return bDll;
	}
	bool init32(LPDWORD remoteaddr, LONG orgEIP)	//32位程序初始化
	{
		//WORD嫬奅僠僃僢僋
		C_ASSERT((offsetof(opcode_data, dllpath) & 1) == 0);

		register BYTE* p = code;

#define emit_(t,x)	*(t* UNALIGNED)p = (t)(x); p += sizeof(t)
#define emit_db(b)	emit_(BYTE, b)
#define emit_dw(w)	emit_(WORD, w)
#define emit_dd(d)	emit_(DWORD, d)

		//側偤偐GetProcAddress偱LoadLibraryW偺傾僪儗僗偑惓偟偔庢傟側偄偙偲偑偁傞偺偱
		//kernel32偺僿僢僟偐傜帺慜偱庢摼偡傞
		FARPROC pfn = K32GetProcAddress("LoadLibraryW");
		if(!pfn)
			return false;

		emit_db(0x60);		//pushad
#if _DEBUG
emit_dw(0xC033);	//xor eax, eax
emit_db(0x50);		//push eax
emit_db(0x50);		//push eax
emit_db(0x68);		//push dllpath
emit_dd((LONG)remoteaddr + offsetof(opcode_data, dllpath));
emit_db(0x50);		//push eax
emit_db(0xB8);		//mov eax, MessageBoxW
emit_dd((LONG)MessageBoxW);
emit_dw(0xD0FF);	//call eax
#endif

		emit_db(0x68);		//push dllpath
		emit_dd((LONG)remoteaddr + offsetof(opcode_data, dllpath));
		emit_db(0xB8);		//mov eax, LoadLibraryW
		emit_dd(pfn);
		emit_dw(0xD0FF);	//call eax

		emit_db(0x61);		//popad
		emit_db(0xE9);		//jmp original_EIP
		emit_dd(orgEIP - (LONG)remoteaddr - (p - code) - sizeof(LONG));

		// gdi++.dll偺僷僗
		return !!GetModuleFileNameW(GetDLLInstance(), dllpath, MAX_PATH);
	}
	bool init64From32(DWORD64* remoteaddr, DWORD64 orgEIP)
	{
		C_ASSERT((offsetof(opcode_data, dllpath) & 1) == 0);

		register BYTE* p = code;

#define emit_(t,x)	*(t* UNALIGNED)p = (t)(x); p += sizeof(t)
#define emit_db(b)	emit_(BYTE, b)
#define emit_dw(w)	emit_(WORD, w)
#define emit_dd(d)	emit_(DWORD, d)
#define emit_ddp(dp) emit_(DWORD64, dp)

		//側偤偐GetProcAddress偱LoadLibraryW偺傾僪儗僗偑惓偟偔庢傟側偄偙偲偑偁傞偺偱
		//kernel32偺僿僢僟偐傜帺慜偱庢摼偡傞
		WCHAR x64Addr[30] = { 0 };
		if (!GetEnvironmentVariable(L"MACTYPE_X64ADDR", x64Addr, 29)) return false;
		DWORD64 pfn = wcstoull(x64Addr, NULL, 10);
		//DWORD64 pfn = getenv("MACTYPE_X64ADDR"); //GetProcAddress64(GetModuleHandle64(L"kernelbase.dll"), "LoadLibraryW");
		if (!pfn)
			return false;

		emit_db(0x50);		//push rax
		emit_db(0x51);		//push rcx
		emit_db(0x52);		//push rdx
		emit_db(0x53);		//push rbx
		emit_dd(0x28ec8348);	//sub rsp,28h
		emit_db(0x48);		//mov rcx, dllpath
		emit_db(0xB9);
		emit_ddp((DWORD64)remoteaddr + offsetof(opcode_data, dllpath));
		emit_db(0x48);		//mov rsi, LoadLibraryW
		emit_db(0xBE);
		emit_ddp(pfn);
		//emit_db(0x48);
		emit_db(0xFF);	//call rdi
		emit_db(0xD6);

		emit_dd(0x28c48348);	//add rsp,28h
		emit_db(0x5B);
		emit_db(0x5A);
		emit_db(0x59);
		emit_db(0x58);		//popad		

		emit_db(0x48);		//mov rdi, orgRip
		emit_db(0xBE);
		emit_ddp(orgEIP);
		emit_db(0xFF);		//jmp rdi
		emit_db(0xE6);

		// gdi++.dll偺僷僗

		bool bDll = !!GetModuleFileNameW(GetDLLInstance(), dllpath, MAX_PATH);
		if (bDll && wcsstr(dllpath, L".dll"))
			wcscpy(wcsstr(dllpath, L".dll"), L"64.dll");
		return bDll;
	}

	bool init64From32(DWORD64* remoteaddr, DWORD64 orgEIP, DWORD dwLoaderOffset)
	{
		C_ASSERT((offsetof(opcode_data, dllpath) & 1) == 0);

		register BYTE* p = code;

#define emit_(t,x)	*(t* UNALIGNED)p = (t)(x); p += sizeof(t)
#define emit_db(b)	emit_(BYTE, b)
#define emit_dw(w)	emit_(WORD, w)
#define emit_dd(d)	emit_(DWORD, d)
#define emit_ddp(dp) emit_(DWORD64, dp)

//get kernelbase.dll imagebase
//credit to http://www.52pojie.cn/thread-162625-1-1.html
/*asm:
	mov rsi, [gs:60h]   ;     peb from teb
	mov rsi, [rsi+18h]    ;_peb_ldr_data from peb
	mov rsi, [rsi+30h]   ;InInitializationOrderModuleList.Flink,
	mov rsi, [rsi]  ;kernelbase.dll
	;mov rsi, [rsi]      ;kernel32.dll (not used for win7+)
	mov rsi, [rsi+10h]
*/
		emit_db(0x65);
		emit_db(0x48);
		emit_db(0x8B);
		emit_db(0x34);
		emit_db(0x25);
		emit_db(0x60);
		emit_db(0x00);
		emit_db(0x00);
		emit_db(0x00);
		emit_db(0x48);
		emit_db(0x8B);
		emit_db(0x76);
		emit_db(0x18);
		emit_db(0x48);
		emit_db(0x8B);
		emit_db(0x76);
		emit_db(0x30);
		emit_db(0x48);
		emit_db(0x8B);
		emit_db(0x36);
		emit_db(0x48);
		emit_db(0x8B);
		emit_db(0x76);
		emit_db(0x10);
//rsi = kernelbase.dll baseaddress

		emit_db(0x50);		//push rax
		emit_db(0x51);		//push rcx
		emit_db(0x52);		//push rdx
		emit_db(0x53);		//push rbx
		emit_dd(0x28ec8348);	//sub rsp,28h
		emit_db(0x48);		//mov rcx, dllpath
		emit_db(0xB9);
		emit_ddp((DWORD64)remoteaddr + offsetof(opcode_data, dllpath));
		emit_db(0x45);
		emit_db(0x31);
		emit_db(0xC0);
		emit_db(0x31);
		emit_db(0xD2);	//xor r8d, r8d; xor edx,edx
		//emit_db(0x48);		//mov rsi, LoadLibraryExW
		//emit_db(0xBE);
		emit_db(0x48);
		emit_db(0x81);
		emit_db(0xC6);	//add rsi, offset LoadLibraryExW
		emit_dd(dwLoaderOffset);
		//emit_db(0x48);
		emit_db(0xFF);	//call rsi
		emit_db(0xD6);

		emit_dd(0x28c48348);	//add rsp,28h
		emit_db(0x5B);
		emit_db(0x5A);
		emit_db(0x59);
		emit_db(0x58);		//popad		

		emit_db(0x48);		//mov rdi, orgRip
		emit_db(0xBE);
		emit_ddp(orgEIP);
		emit_db(0xFF);		//jmp rdi
		emit_db(0xE6);

		// gdi++.dll偺僷僗

		bool bDll = !!GetModuleFileNameW(GetDLLInstance(), dllpath, MAX_PATH);
		if (bDll && wcsstr(dllpath, L".dll"))
			wcscpy(wcsstr(dllpath, L".dll"), L"64.dll");
		return bDll;
	}

	bool init(DWORD_PTR* remoteaddr, DWORD_PTR orgEIP)
	{
		//WORD嫬奅僠僃僢僋
		C_ASSERT((offsetof(opcode_data, dllpath) & 1) == 0);

		register BYTE* p = code;
#undef emit_ddp

#define emit_(t,x)	*(t* UNALIGNED)p = (t)(x); p += sizeof(t)
#define emit_db(b)	emit_(BYTE, b)
#define emit_dw(w)	emit_(WORD, w)
#define emit_dd(d)	emit_(DWORD, d)
#define emit_ddp(dp) emit_(DWORD_PTR, dp)

		//側偤偐GetProcAddress偱LoadLibraryW偺傾僪儗僗偑惓偟偔庢傟側偄偙偲偑偁傞偺偱
		//kernel32偺僿僢僟偐傜帺慜偱庢摼偡傞
		FARPROC pfn = GetProcAddress(GetModuleHandle(L"kernel32.dll"), "LoadLibraryW");
		//if(!pfn)
		//	return false;

		emit_db(0x50);		//push rax
		emit_db(0x51);		//push rcx
		emit_db(0x52);		//push rdx
		emit_db(0x53);		//push rbx
		emit_dd(0x28ec8348);	//sub rsp,28h
		emit_db(0x48);		//mov rcx, dllpath
		emit_db(0xB9);
		emit_ddp((DWORD_PTR)remoteaddr + offsetof(opcode_data, dllpath));
		emit_db(0x48);		//mov rsi, LoadLibraryW
		emit_db(0xBE);		
		emit_ddp(pfn);
		//emit_db(0x48);
		emit_db(0xFF);	//call rdi
		emit_db(0xD6);

		emit_dd(0x28c48348);	//add rsp,28h
		emit_db(0x5B);	
		emit_db(0x5A);	
		emit_db(0x59);	
		emit_db(0x58);		//popad		
		
		emit_db(0x48);		//mov rdi, orgRip
		emit_db(0xBE);
		emit_ddp(orgEIP);
		emit_db(0xFF);		//jmp rdi
		emit_db(0xE6);

		// gdi++.dll偺僷僗

		return !!GetModuleFileNameW(GetDLLInstance(), dllpath, MAX_PATH);
	}

};
#include <poppack.h>

// 安全的取得真实系统信息
VOID SafeGetNativeSystemInfo(__out LPSYSTEM_INFO lpSystemInfo)
{
	if (NULL == lpSystemInfo)    return;
	typedef VOID(WINAPI *LPFN_GetNativeSystemInfo)(LPSYSTEM_INFO lpSystemInfo);
	LPFN_GetNativeSystemInfo fnGetNativeSystemInfo = (LPFN_GetNativeSystemInfo)GetProcAddress(GetModuleHandle(_T("kernel32")), "GetNativeSystemInfo");;
	if (NULL != fnGetNativeSystemInfo)
	{
		fnGetNativeSystemInfo(lpSystemInfo);
	}
	else
	{
		GetSystemInfo(lpSystemInfo);
	}
}

// 获取操作系统位数
int GetSystemBits()
{
	SYSTEM_INFO si;
	SafeGetNativeSystemInfo(&si);
	if (si.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_AMD64 ||
		si.wProcessorArchitecture == PROCESSOR_ARCHITECTURE_IA64)
	{
		return 64;
	}
	return 32;
}

static bool bIsOS64 = GetSystemBits() == 64;	// check if running in a x64 system.

#ifdef _M_IX86
// 巭傔偰偄傞僾儘僙僗偵LoadLibrary偡傞僐乕僪傪拲擖
EXTERN_C BOOL WINAPI GdippInjectDLL(const PROCESS_INFORMATION* ppi)
{
	BOOL bIsX64Proc = false;
	if (bIsOS64 && IsWow64Process(ppi->hProcess, &bIsX64Proc) && !bIsX64Proc)
	{
		//x86 process launches a x64 process
		_CONTEXT64 ctx = { 0 };
		ctx.ContextFlags = CONTEXT_CONTROL;
		if (!GetThreadContext64(ppi->hThread, &ctx))
			return false;
		static bool bTryLoadDll64 = false;
		static DWORD dwLoaderOffset = 0;
		if (!bTryLoadDll64) {
			bTryLoadDll64 = true;
			GetEnvironmentVariable(L"MACTYPE_X64ADDR", NULL, 0);
			if (GetLastError() == ERROR_ENVVAR_NOT_FOUND) {
				// kernel32.dll not loaded
				DWORD64 hKernel32Dll = 0;
				if (IsWindows7OrGreater())
					hKernel32Dll = LoadLibraryW64(L"kernelbase.dll");
				else
					hKernel32Dll = LoadLibraryW64(L"kernel32.dll");

				if (hKernel32Dll) {
					DWORD64 pfnLdrAddr = GetProcAddress64(hKernel32Dll, "LoadLibraryExW");
					if (pfnLdrAddr) {
						dwLoaderOffset = (DWORD)(pfnLdrAddr - hKernel32Dll);
					}
				}
			}
		}

		opcode_data local;
		DWORD64 remote = VirtualAllocEx64(ppi->hProcess, NULL, sizeof(opcode_data), MEM_COMMIT, PAGE_EXECUTE_READWRITE);
		if (!remote)
			return false;
		bool basmIniter = dwLoaderOffset ? local.init64From32((DWORD64*)remote, ctx.Rip, dwLoaderOffset) : local.init64From32((DWORD64*)remote, ctx.Rip);
		if (!basmIniter	|| !WriteProcessMemory64(ppi->hProcess, remote, &local, sizeof(opcode_data), NULL)) {
			VirtualFreeEx64(ppi->hProcess, remote, 0, MEM_RELEASE);
			return false;
		}

		//FlushInstructionCache64(ppi->hProcess, remote, sizeof(opcode_data));
		//FARPROC a=(FARPROC)remote;
		//a();
		ctx.Rip = (DWORD64)remote;
		return !!SetThreadContext64(ppi->hThread, &ctx);
	}
	else {
		CONTEXT ctx = { 0 };
		ctx.ContextFlags = CONTEXT_CONTROL;
		if (!GetThreadContext(ppi->hThread, &ctx))
			return false;

		opcode_data local;
		opcode_data* remote = (opcode_data*)VirtualAllocEx(ppi->hProcess, NULL, sizeof(opcode_data), MEM_COMMIT, PAGE_EXECUTE_READWRITE);
		if (!remote)
			return false;

		if (!local.init32((LPDWORD)remote, ctx.Eip)
			|| !WriteProcessMemory(ppi->hProcess, remote, &local, sizeof(opcode_data), NULL)) {
			VirtualFreeEx(ppi->hProcess, remote, 0, MEM_RELEASE);
			return false;
		}

		FlushInstructionCache(ppi->hProcess, remote, sizeof(opcode_data));
		ctx.Eip = (DWORD)remote;
		return !!SetThreadContext(ppi->hThread, &ctx);
	}
}
#else
EXTERN_C BOOL WINAPI GdippInjectDLL(const PROCESS_INFORMATION* ppi)
{
	BOOL bWow64 = false;
	IsWow64Process(ppi->hProcess, &bWow64);
	if (bWow64)
	{
		WOW64_CONTEXT ctx = { 0 };
		ctx.ContextFlags = CONTEXT_CONTROL;
		//CREATE_SUSPENDED側偺偱婎杮揑偵惉岟偡傞偼偢
		if(!Wow64GetThreadContext(ppi->hThread, &ctx))
			return false;

		opcode_data local;
		LPVOID remote = VirtualAllocEx(ppi->hProcess, NULL, sizeof(opcode_data), MEM_COMMIT, PAGE_EXECUTE_READWRITE);
		if(!remote)
			return false;

		if(!local.initWow64((LPDWORD)remote, ctx.Eip)
			|| !WriteProcessMemory(ppi->hProcess, remote, &local, sizeof(opcode_data), NULL)) {
				VirtualFreeEx(ppi->hProcess, remote, 0, MEM_RELEASE);
				return false;
		}

		FlushInstructionCache(ppi->hProcess, remote, sizeof(opcode_data));
		//FARPROC a=(FARPROC)remote;
		//a();
		ctx.Eip = (DWORD)remote;
		return !!Wow64SetThreadContext(ppi->hThread, &ctx);
	}
	else
	{
		CONTEXT ctx = { 0 };
		ctx.ContextFlags = CONTEXT_CONTROL;
		//CREATE_SUSPENDED側偺偱婎杮揑偵惉岟偡傞偼偢
		if(!GetThreadContext(ppi->hThread, &ctx))
			return false;

		opcode_data local;
		LPVOID remote = VirtualAllocEx(ppi->hProcess, NULL, sizeof(opcode_data), MEM_COMMIT, PAGE_EXECUTE_READWRITE);
		if(!remote)
			return false;

		if(!local.init((DWORD_PTR*)remote, ctx.Rip)
			|| !WriteProcessMemory(ppi->hProcess, remote, &local, sizeof(opcode_data), NULL)) {
				VirtualFreeEx(ppi->hProcess, remote, 0, MEM_RELEASE);
				return false;
		}

		FlushInstructionCache(ppi->hProcess, remote, sizeof(opcode_data));
		//FARPROC a=(FARPROC)remote;
		//a();
		ctx.Rip = (DWORD_PTR)remote;
		return !!SetThreadContext(ppi->hThread, &ctx);
	}
}

#endif

template <typename _TCHAR>
int strlendb(const _TCHAR* psz)
{
	const _TCHAR* p = psz;
	while (*p) {
		for (; *p; p++);
		p++;
	}
	return p - psz + 1;
}

template <typename _TCHAR>
_TCHAR* strdupdb(const _TCHAR* psz, int pad)
{
	int len = strlendb(psz);
	_TCHAR* p = (_TCHAR*)calloc(sizeof(_TCHAR), len + pad);
	if(p) {
		memcpy(p, psz, sizeof(_TCHAR) * len);
	}
	return p;
}



bool MultiSzToArray(LPWSTR p, CArray<LPWSTR>& arr)
{
	for (; *p; ) {
		LPWSTR cp = _wcsdup(p);
		if(!cp || !arr.Add(cp)) {
			free(cp);
			return false;
		}
		for (; *p; p++);
		p++;
	}
	return true;
}

LPWSTR ArrayToMultiSz(CArray<LPWSTR>& arr)
{
	size_t cch = 1;
	for (int i=0; i<arr.GetSize(); i++) {
		cch += wcslen(arr[i]) + 1;
	}

	LPWSTR pmsz = (LPWSTR)calloc(sizeof(WCHAR), cch);
	if (!pmsz)
		return NULL;

	LPWSTR p = pmsz;
	for (int i=0; i<arr.GetSize(); i++) {
		StringCchCopyExW(p, cch, arr[i], &p, &cch, STRSAFE_NO_TRUNCATION);
		p++;
	}
	*p = 0;
	return pmsz;
}

bool AddPathEnv(CArray<LPWSTR>& arr, LPWSTR dir, int dirlen)
{
	for (int i=0; i<arr.GetSize(); i++) {
		LPWSTR env = arr[i];
		if (_wcsnicmp(env, L"PATH=", 5)) {
			continue;
		}

		LPWSTR p = env + 5;
		LPWSTR pp = p;
		for (; ;) {
			for (; *p && *p != L';'; p++);
			int len = p - pp;
			if (len == dirlen && !_wcsnicmp(pp, dir, dirlen)) {
				return false;
			}
			if (!*p)
				break;
			pp = p + 1;
			p++;
		}

		size_t cch = wcslen(env) + MAX_PATH + 4;
		env = (LPWSTR)realloc(env, sizeof(WCHAR) * cch);
		if(env) {
			StringCchCatW(env, cch, L";");
			StringCchCatW(env, cch, dir);
			arr[i] = env;
			return true;
		}
		return false;
	}

	size_t cch = dirlen + sizeof("PATH=") + 1;
	LPWSTR p = (LPWSTR)calloc(sizeof(WCHAR), cch);
	if(p) {
		StringCchCopyW(p, cch, L"PATH=");
		StringCchCatW(p, cch, dir);
		if (arr.Add(p)) {
			return true;
		}
		free(p);
	}
	return false;
}

bool AddX64Env(CArray<LPWSTR>& arr)
{
	FARPROC k32 = GetProcAddress(GetModuleHandle(L"kernel32.dll"), "LoadLibraryW");
	WCHAR szAddr[20] = { 0 };
	_ui64tow((DWORD64)k32, szAddr, 10);
	//wsprintf(szAddr, L"%Ld", (DWORD_PTR)k32);
	size_t cch = wcslen(szAddr) + sizeof("MACTYPE_X64ADDR=") + 1;
	LPWSTR p = (LPWSTR)calloc(sizeof(WCHAR), cch);
	if (p) {
		StringCchCopyW(p, cch, L"MACTYPE_X64ADDR=");
		StringCchCatW(p, cch, szAddr);
		if (arr.Add(p)) {
			return true;
		}
		free(p);
	}
	return false;
}

EXTERN_C LPWSTR WINAPI GdippEnvironment(DWORD& dwCreationFlags, LPVOID lpEnvironment)
{
	TCHAR dir[MAX_PATH];
	int dirlen = GetModuleFileName(GetDLLInstance(), dir, MAX_PATH);
	LPTSTR lpfilename=dir+dirlen;
	while (lpfilename>dir && *lpfilename!=_T('\\') && *lpfilename!=_T('/')) --lpfilename;
	*lpfilename = 0;
	dirlen = wcslen(dir);

	LPWSTR pEnvW = NULL;
	if (lpEnvironment) {
		if (dwCreationFlags & CREATE_UNICODE_ENVIRONMENT) {
			pEnvW = strdupdb((LPCWSTR)lpEnvironment, MAX_PATH + 1);
		} else {
			int alen = strlendb((LPCSTR)lpEnvironment);
			int wlen = MultiByteToWideChar(CP_ACP, 0, (LPCSTR)lpEnvironment, alen, NULL, 0) + 1;
			pEnvW = (LPWSTR)calloc(sizeof(WCHAR), wlen + MAX_PATH + 1);
			if (pEnvW) {
				MultiByteToWideChar(CP_ACP, 0, (LPCSTR)lpEnvironment, alen, pEnvW, wlen);
			}
		}
	} else {
		LPWSTR block = (LPWSTR)GetEnvironmentStringsW();
		if (block) {
			pEnvW = strdupdb(block, MAX_PATH + 1);
			FreeEnvironmentStrings(block);
		}
	}

	if (!pEnvW) {
		return NULL;
	}

	CArray<LPWSTR> envs;
	bool ret = MultiSzToArray(pEnvW, envs);
	free(pEnvW);
	pEnvW = NULL;
	
	if (ret) {
		ret = AddPathEnv(envs, dir, dirlen);
	}
#ifdef _WIN64
	{
		GetEnvironmentVariableW(L"MACTYPE_X64ADDR", NULL, 0);
		if (GetLastError() == ERROR_ENVVAR_NOT_FOUND) {
			ret = AddX64Env(envs);
		}
	}
#endif
	if (ret) {
		pEnvW = ArrayToMultiSz(envs);
	}

	for (int i=0; i<envs.GetSize(); free(envs[i++]));

	if (!pEnvW) {
		return NULL;
	}

#ifdef _DEBUG
	{
		LPWSTR tmp = strdupdb(pEnvW, 0);
		LPWSTR tmpe = tmp + strlendb(tmp);
		PathRemoveFileSpec(dir);
		for (LPWSTR z=tmp; z<tmpe; z++)if(!*z)*z=L'\n';
			StringCchCatW(dir,MAX_PATH,L"\\");
			StringCchCatW(dir,MAX_PATH,L"gdienv.txt");
			HANDLE hf = CreateFileW(dir,GENERIC_WRITE,0,NULL,CREATE_ALWAYS,0,NULL);
			if(hf) {
			DWORD cb;
			WORD w = 0xfeff;
			WriteFile(hf,&w, sizeof(WORD), &cb, 0);
			WriteFile(hf,tmp, sizeof(WCHAR) * (tmpe - tmp), &cb, 0);
			SetEndOfFile(hf);
			CloseHandle(hf);
			free(tmp);
		}
	}
#endif

	dwCreationFlags |= CREATE_UNICODE_ENVIRONMENT;
	return pEnvW;
}
