/*
    EasyHook - The reinvention of Windows API hooking
 
    Copyright (C) 2009 Christoph Husse

    This library is free software; you can redistribute it and/or
    modify it under the terms of the GNU Lesser General Public
    License as published by the Free Software Foundation; either
    version 2.1 of the License, or (at your option) any later version.

    This library is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
    Lesser General Public License for more details.

    You should have received a copy of the GNU Lesser General Public
    License along with this library; if not, write to the Free Software
    Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301  USA

    Please visit http://www.codeplex.com/easyhook for more information
    about the project and latest updates.
*/

#ifndef _EASYHOOK_H_
#define _EASYHOOK_H_

#ifdef DRIVER

    #include <ntddk.h>
    #include <ntstrsafe.h>

	typedef int BOOL;
	typedef void* HMODULE;

#else

//    #define NTDDI_VERSION           NTDDI_WIN2KSP4
  //  #define _WIN32_WINNT            0x500
    #define _WIN32_IE_              _WIN32_IE_WIN2KSP4

    #include <windows.h>
    #include <winnt.h>
    #include <winternl.h>

#endif


#ifdef __cplusplus
extern "C"{
#endif

#ifdef EASYHOOK_EXPORTS
    #define EASYHOOK_API						__declspec(dllexport) __stdcall
	#define DRIVER_SHARED_API(type, decl)		EXTERN_C type EASYHOOK_API decl
#else
    #ifndef DRIVER
        #define EASYHOOK_API					__declspec(dllimport) __stdcall
		#define DRIVER_SHARED_API(type, decl)	EXTERN_C type EASYHOOK_API decl
    #else
        #define EASYHOOK_API					__stdcall
		#define DRIVER_SHARED_API(type, decl)	typedef type EASYHOOK_API PROC_##decl; EXTERN_C type EASYHOOK_API decl
    #endif
#endif

/* 
    This is the typical sign that a defined method is exported...

    Methods marked with this attribute need special attention
    during parameter validation and documentation.
*/
#define EASYHOOK_NT_EXPORT          EXTERN_C NTSTATUS EASYHOOK_API
#define EASYHOOK_BOOL_EXPORT        EXTERN_C BOOL EASYHOOK_API

#define MAX_HOOK_COUNT              128
#define MAX_ACE_COUNT               128
#define MAX_THREAD_COUNT            128
#define MAX_PASSTHRU_SIZE           1024 * 64

typedef struct _HOOK_ACL_
{
	ULONG                   Count;
	BOOL                    IsExclusive;
	ULONG                   Entries[MAX_ACE_COUNT];
}HOOK_ACL;

typedef struct _LOCAL_HOOK_INFO_* PLOCAL_HOOK_INFO;

typedef struct _HOOK_TRACE_INFO_
{
    PLOCAL_HOOK_INFO        Link;
}HOOK_TRACE_INFO, *TRACED_HOOK_HANDLE;

typedef struct _LOCAL_HOOK_INFO_
{
	PLOCAL_HOOK_INFO        Next;
	ULONG					NativeSize;
	UCHAR*					TargetProc;
	ULONGLONG				TargetBackup;
	ULONGLONG				TargetBackup_x64;
	ULONGLONG				HookCopy;
	ULONG					EntrySize;
	UCHAR*					Trampoline;
	ULONG					HLSIndex;
	ULONG					HLSIdent;
	void*					Callback;
	HOOK_ACL				LocalACL;
	ULONG                   Signature;
	TRACED_HOOK_HANDLE      Tracking;

	void*					RandomValue; // fixed
	void*					HookIntro; // fixed
	UCHAR*					OldProc; // fixed
	UCHAR*					HookProc; // fixed
	void*					HookOutro; // fixed
	int*					IsExecutedPtr; // fixed
}LOCAL_HOOK_INFO, *PLOCAL_HOOK_INFO;

DRIVER_SHARED_API(NTSTATUS, RtlGetLastError());

DRIVER_SHARED_API(PWCHAR, RtlGetLastErrorString());

DRIVER_SHARED_API(NTSTATUS, LhInstallHook(
            void* InEntryPoint,
            void* InHookProc,
            void* InCallback,
            TRACED_HOOK_HANDLE OutHandle));

DRIVER_SHARED_API(NTSTATUS, LhUninstallAllHooks());

DRIVER_SHARED_API(NTSTATUS, LhUninstallHook(TRACED_HOOK_HANDLE InHandle));

DRIVER_SHARED_API(NTSTATUS, LhWaitForPendingRemovals());

/*
    Setup the ACLs after hook installation. Please note that every
    hook starts suspended. You will have to set a proper ACL to
    make it active!
*/
#ifdef DRIVER

	DRIVER_SHARED_API(NTSTATUS, LhSetInclusiveACL(
				ULONG* InProcessIdList,
				ULONG InProcessCount,
				TRACED_HOOK_HANDLE InHandle));

	DRIVER_SHARED_API(NTSTATUS, LhSetExclusiveACL(
				ULONG* InProcessIdList,
				ULONG InProcessCount,
				TRACED_HOOK_HANDLE InHandle));

	DRIVER_SHARED_API(NTSTATUS, LhSetGlobalInclusiveACL(
				ULONG* InProcessIdList,
				ULONG InProcessCount));

	DRIVER_SHARED_API(NTSTATUS, LhSetGlobalExclusiveACL(
				ULONG* InProcessIdList,
				ULONG InProcessCount));

	DRIVER_SHARED_API(NTSTATUS, LhIsProcessIntercepted(
				TRACED_HOOK_HANDLE InHook,
				ULONG InProcessID,
				BOOL* OutResult));

#else

	EASYHOOK_NT_EXPORT LhSetInclusiveACL(
				ULONG* InThreadIdList,
				ULONG InThreadCount,
				TRACED_HOOK_HANDLE InHandle);

	EASYHOOK_NT_EXPORT LhSetExclusiveACL(
				ULONG* InThreadIdList,
				ULONG InThreadCount,
				TRACED_HOOK_HANDLE InHandle);

	EASYHOOK_NT_EXPORT LhSetGlobalInclusiveACL(
				ULONG* InThreadIdList,
				ULONG InThreadCount);

	EASYHOOK_NT_EXPORT LhSetGlobalExclusiveACL(
				ULONG* InThreadIdList,
				ULONG InThreadCount);

	EASYHOOK_NT_EXPORT LhIsThreadIntercepted(
				TRACED_HOOK_HANDLE InHook,
				ULONG InThreadID,
				BOOL* OutResult);

#endif // !DRIVER

/*
    The following barrier methods are meant to be used in hook handlers only!

    They will all fail with STATUS_NOT_SUPPORTED if called outside a
    valid hook handler...
*/
DRIVER_SHARED_API(NTSTATUS, LhBarrierGetCallback(PVOID* OutValue));

DRIVER_SHARED_API(NTSTATUS, LhBarrierGetReturnAddress(PVOID* OutValue));

DRIVER_SHARED_API(NTSTATUS, LhBarrierGetAddressOfReturnAddress(PVOID** OutValue));

DRIVER_SHARED_API(NTSTATUS, LhBarrierBeginStackTrace(PVOID* OutBackup));

DRIVER_SHARED_API(NTSTATUS, LhBarrierEndStackTrace(PVOID InBackup));

typedef struct _MODULE_INFORMATION_* PMODULE_INFORMATION;

typedef struct _MODULE_INFORMATION_
{	
	PMODULE_INFORMATION		Next;
	UCHAR*					BaseAddress;
	ULONG					ImageSize;
	CHAR					Path[256];
	PCHAR					ModuleName;
}MODULE_INFORMATION;

EASYHOOK_NT_EXPORT LhUpdateModuleInformation();

DRIVER_SHARED_API(NTSTATUS, LhBarrierPointerToModule(
			PVOID InPointer,
			MODULE_INFORMATION* OutModule));

DRIVER_SHARED_API(NTSTATUS, LhEnumModules(
			HMODULE* OutModuleArray, 
            ULONG InMaxModuleCount,
            ULONG* OutModuleCount));

DRIVER_SHARED_API(NTSTATUS, LhBarrierGetCallingModule(MODULE_INFORMATION* OutModule));

DRIVER_SHARED_API(NTSTATUS, LhBarrierCallStackTrace(
            PVOID* OutMethodArray, 
            ULONG InMaxMethodCount,
            ULONG* OutMethodCount));

#ifdef DRIVER

	#define DRIVER_EXPORT(proc)				PROC_##proc * proc

	#define EASYHOOK_INTERFACE_v_1			0x0001

	#define EASYHOOK_WIN32_DEVICE_NAME		L"\\\\.\\EasyHook"
	#define EASYHOOK_DEVICE_NAME			L"\\Device\\EasyHook"
	#define EASYHOOK_DOS_DEVICE_NAME		L"\\DosDevices\\EasyHook"
	#define FILE_DEVICE_EASYHOOK			0x893F

	typedef struct _EASYHOOK_INTERFACE_API_v_1_
	{
		DRIVER_EXPORT(RtlGetLastError);
		DRIVER_EXPORT(RtlGetLastErrorString);
		DRIVER_EXPORT(LhInstallHook);
		DRIVER_EXPORT(LhUninstallHook);
		DRIVER_EXPORT(LhWaitForPendingRemovals);
		DRIVER_EXPORT(LhBarrierGetCallback);
		DRIVER_EXPORT(LhBarrierGetReturnAddress);
		DRIVER_EXPORT(LhBarrierGetAddressOfReturnAddress);
		DRIVER_EXPORT(LhBarrierBeginStackTrace);
		DRIVER_EXPORT(LhBarrierEndStackTrace);
		DRIVER_EXPORT(LhBarrierPointerToModule);
		DRIVER_EXPORT(LhBarrierGetCallingModule);
		DRIVER_EXPORT(LhBarrierCallStackTrace);
		DRIVER_EXPORT(LhSetGlobalExclusiveACL);
		DRIVER_EXPORT(LhSetGlobalInclusiveACL);
		DRIVER_EXPORT(LhSetExclusiveACL);
		DRIVER_EXPORT(LhSetInclusiveACL);
		DRIVER_EXPORT(LhIsProcessIntercepted);
	}EASYHOOK_INTERFACE_API_v_1, *PEASYHOOK_INTERFACE_API_v_1;

	typedef struct _EASYHOOK_DEVICE_EXTENSION_
	{
		ULONG								MaxVersion;
		// enumeration of APIs
		EASYHOOK_INTERFACE_API_v_1			API_v_1;
	}EASYHOOK_DEVICE_EXTENSION, *PEASYHOOK_DEVICE_EXTENSION;

	static NTSTATUS EasyHookQueryInterface(
		ULONG InInterfaceVersion,
		PVOID OutInterface,
		PFILE_OBJECT* OutEasyHookDrv)
	{
	/*
		Description:
			
			Provides a convenient way to load the desired EasyHook interface.
			The method will only work if the EasyHook support driver is loaded, of course.
			If you don't need the interface anymore, you have to release the
			file object with ObDereferenceObject().

		Parameters:
			
			- InInterfaceVersion

				The desired interface version. Any future EasyHook driver will ALWAYS
				be backward compatible. This is the reason why I provide such a flexible
				interface mechanism. 

			- OutInterface

				A pointer to the interface structure to be filled with data. If you specify
				EASYHOOK_INTERFACE_v_1 as InInterfaceVersion, you will have to provide a
				pointer to a EASYHOOK_INTERFACE_API_v_1 structure, for example...

			- OutEasyHookDrv

				A reference to the EasyHook driver. Make sure that you dereference it if
				you don't need the interface any longer! As long as you keep this handle,
				the EasyHook driver CAN'T be unloaded...

	*/
		UNICODE_STRING				DeviceName;
		PDEVICE_OBJECT				hEasyHookDrv = NULL;
		NTSTATUS					NtStatus = STATUS_INTERNAL_ERROR;
		EASYHOOK_DEVICE_EXTENSION*	DevExt;

		/*
			Open log file...
		*/
		RtlInitUnicodeString(&DeviceName, EASYHOOK_DEVICE_NAME);

		if(!NT_SUCCESS(NtStatus = IoGetDeviceObjectPointer(&DeviceName, FILE_READ_DATA, OutEasyHookDrv, &hEasyHookDrv)))
			return NtStatus;

		__try
		{
			DevExt = (EASYHOOK_DEVICE_EXTENSION*)hEasyHookDrv->DeviceExtension;

			if(DevExt->MaxVersion < InInterfaceVersion)
				return STATUS_NOT_SUPPORTED;

			switch(InInterfaceVersion)
			{
			case EASYHOOK_INTERFACE_v_1: memcpy(OutInterface, &DevExt->API_v_1, sizeof(DevExt->API_v_1)); break;
			default: 
				return STATUS_INVALID_PARAMETER_1;
			}

			return STATUS_SUCCESS;
		}
		__except(EXCEPTION_EXECUTE_HANDLER)
		{
			ObDereferenceObject(*OutEasyHookDrv);

			return NtStatus;
		}
	}


#endif // DRIVER

#ifndef DRIVER
	/*
		Debug helper API.
	*/
	EASYHOOK_BOOL_EXPORT DbgIsAvailable();

	EASYHOOK_BOOL_EXPORT DbgIsEnabled();

	EASYHOOK_NT_EXPORT DbgAttachDebugger();

	EASYHOOK_NT_EXPORT DbgDetachDebugger();

	EASYHOOK_NT_EXPORT DbgGetThreadIdByHandle(
				HANDLE InThreadHandle,
				ULONG* OutThreadId);

	EASYHOOK_NT_EXPORT DbgGetProcessIdByHandle(
				HANDLE InProcessHandle,
				ULONG* OutProcessId);

	EASYHOOK_NT_EXPORT DbgHandleToObjectName(
				HANDLE InNamedHandle,
				UNICODE_STRING* OutNameBuffer,
				ULONG InBufferSize,
				ULONG* OutRequiredSize);


	/*
		Injection support API.
	*/
	typedef struct _REMOTE_ENTRY_INFO_
	{
		ULONG           HostPID;
		UCHAR*          UserData;
		ULONG           UserDataSize;
	}REMOTE_ENTRY_INFO;

	typedef void __stdcall REMOTE_ENTRY_POINT(REMOTE_ENTRY_INFO* InRemoteInfo);

	#define EASYHOOK_INJECT_DEFAULT				0x00000000
	#define EASYHOOK_INJECT_STEALTH				0x10000000 // (experimental)
	#define EASYHOOK_INJECT_NET_DEFIBRILLATOR	0x20000000 // USE THIS ONLY IN UNMANAGED CODE AND ONLY WITH CreateAndInject() FOR MANAGED PROCESSES!!

	EASYHOOK_NT_EXPORT RhCreateStealthRemoteThread(
				ULONG InTargetPID,
				LPTHREAD_START_ROUTINE InRemoteRoutine,
				PVOID InRemoteParam,
				HANDLE* OutRemoteThread);

	EASYHOOK_NT_EXPORT RhInjectLibrary(
				ULONG InTargetPID,
				ULONG InWakeUpTID,
				ULONG InInjectionOptions,
				WCHAR* InLibraryPath_x86,
				WCHAR* InLibraryPath_x64,
				PVOID InPassThruBuffer,
				ULONG InPassThruSize);

	EASYHOOK_NT_EXPORT RhCreateAndInject(
				WCHAR* InEXEPath,
				WCHAR* InCommandLine,
				ULONG InProcessCreationFlags,
				ULONG InInjectionOptions,
				WCHAR* InLibraryPath_x86,
				WCHAR* InLibraryPath_x64,
				PVOID InPassThruBuffer,
				ULONG InPassThruSize,
				ULONG* OutProcessId);

	EASYHOOK_BOOL_EXPORT RhIsX64System();

	EASYHOOK_NT_EXPORT RhIsX64Process(
				ULONG InProcessId,
				BOOL* OutResult);

	EASYHOOK_BOOL_EXPORT RhIsAdministrator();

	EASYHOOK_NT_EXPORT RhWakeUpProcess();

	EASYHOOK_NT_EXPORT RhInstallSupportDriver();

	EASYHOOK_NT_EXPORT RhInstallDriver(
			WCHAR* InDriverPath,
			WCHAR* InDriverName);

	typedef struct _GACUTIL_INFO_* HGACUTIL;


#endif // !DRIVER

#ifdef __cplusplus
};
#endif

#endif