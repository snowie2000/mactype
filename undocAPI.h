#pragma once

typedef struct _UNICODE_STRING2 {
	USHORT Length;
	USHORT MaximumLength;
	PWSTR  Buffer;
} UNICODE_STRING2;
typedef UNICODE_STRING2 *PUNICODE_STRING2;
typedef const UNICODE_STRING2 *PCUNICODE_STRING2;

const DWORD GT_DEVICE_TO_WORLD = 0x0402;
const DWORD GT_WORLD_TO_DEVICE = 0x0204;
const DWORD GT_WORLD_TO_PAGE = 0x0203;
const DWORD GT_PAGE_TO_DEVICE = 0x0304;

typedef int(__stdcall * TGetTextFA)(HDC, int, LPWSTR);
typedef BOOL (__stdcall *PFNCreateProcessInternalW) 
( 
 HANDLE hToken, 
 LPCTSTR lpApplicationName,        
 LPTSTR lpCommandLine,        
 LPSECURITY_ATTRIBUTES lpProcessAttributes, 
 LPSECURITY_ATTRIBUTES lpThreadAttributes,        
 BOOL bInheritHandles,        
 DWORD dwCreationFlags, 
 LPVOID lpEnvironment,        
 LPCTSTR lpCurrentDirectory,        
 LPSTARTUPINFO lpStartupInfo,        
 LPPROCESS_INFORMATION lpProcessInformation , 
 PHANDLE hNewToken 
 ); 
typedef BOOL
(WINAPI *PFNCreateProcessW)(
							__in_opt    LPCWSTR lpApplicationName,
							__inout_opt LPWSTR lpCommandLine,
							__in_opt    LPSECURITY_ATTRIBUTES lpProcessAttributes,
							__in_opt    LPSECURITY_ATTRIBUTES lpThreadAttributes,
							__in        BOOL bInheritHandles,
							__in        DWORD dwCreationFlags,
							__in_opt    LPVOID lpEnvironment,
							__in_opt    LPCWSTR lpCurrentDirectory,
							__in        LPSTARTUPINFOW lpStartupInfo,
							__out       LPPROCESS_INFORMATION lpProcessInformation
							);
typedef BOOL
(WINAPI *PFNCreateProcessA)(
							__in_opt    LPCSTR lpApplicationName,
							__inout_opt LPSTR lpCommandLine,
							__in_opt    LPSECURITY_ATTRIBUTES lpProcessAttributes,
							__in_opt    LPSECURITY_ATTRIBUTES lpThreadAttributes,
							__in        BOOL bInheritHandles,
							__in        DWORD dwCreationFlags,
							__in_opt    LPVOID lpEnvironment,
							__in_opt    LPCSTR lpCurrentDirectory,
							__in        LPSTARTUPINFOA lpStartupInfo,
							__out       LPPROCESS_INFORMATION lpProcessInformation
							);
typedef BOOL (WINAPI *PFNIsWow64Process)(HANDLE hProcess, PBOOL Wow64Process );
typedef int (WINAPI * TGdiGetCodePage)(HDC);
typedef BOOL (WINAPI * TGetTransform)(HDC, DWORD, XFORM*);
typedef BOOL (WINAPI * PFNGetFontResourceInfo)(LPCWSTR, DWORD*, VOID*, DWORD);
typedef LONG (WINAPI * PFNLdrLoadDll)(
						 IN PWCHAR               PathToFile OPTIONAL,
						 IN ULONG                Flags OPTIONAL,
						 IN UNICODE_STRING2*      ModuleFileName,
						 OUT HANDLE*             ModuleHandle 
						 );
typedef BOOL(WINAPI * PFNSetProcessMitigationPolicy)(
	_In_ PROCESS_MITIGATION_POLICY MitigationPolicy,
	_In_ PVOID                     lpBuffer,
	_In_ SIZE_T                    dwLength);
static TGetTransform GetTransform = (TGetTransform)GetProcAddress(LoadLibrary(_T("gdi32.dll")), "GetTransform");
 /***********************************************************************
  *           GetTransform    (GDI32.@)
+ *
+ * Undocumented
+ *
+ * Returns one of the co-ordinate space transforms
+ *
+ * PARAMS
+ *    hdc   [I] Device context.
+ *    which [I] Which xform to return:
+ *                  0x203 World -> Page transform (that set by SetWorldTransform).
+ *                  0x304 Page -> Device transform (the mapping mode transform).
+ *                  0x204 World -> Device transform (the combination of the above two).
+ *                  0x402 Device -> World transform (the inversion of the above).
+ *    xform [O] The xform.
+ *
  ************************************************************************/

static TGdiGetCodePage GdiGetCodePage = (TGdiGetCodePage)GetProcAddress(LoadLibrary(_T("gdi32.dll")),"GdiGetCodePage");
static TGetTextFA GetTextFaceAliasW= (TGetTextFA)GetProcAddress(LoadLibrary(_T("gdi32.dll")),"GetTextFaceAliasW");
static PFNCreateProcessInternalW CreateProcessInternalW_KernelBase = (PFNCreateProcessInternalW)GetProcAddress(GetModuleHandle(_T("kernelbase.dll")),"CreateProcessInternalW");
static PFNCreateProcessInternalW CreateProcessInternalW = CreateProcessInternalW_KernelBase ? CreateProcessInternalW_KernelBase:(PFNCreateProcessInternalW)GetProcAddress(GetModuleHandle(_T("kernel32.dll")),"CreateProcessInternalW");
//static PFNIsWow64Process IsWow64Process=(PFNIsWow64Process)GetProcAddress(LoadLibrary(L"Kernel32.dll"), "IsWow64Process");
static PFNGetFontResourceInfo GetFontResourceInfo=(PFNGetFontResourceInfo)GetProcAddress(LoadLibrary(L"gdi32.dll"), "GetFontResourceInfoW");