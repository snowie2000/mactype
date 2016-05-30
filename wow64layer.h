/*
#ifndef CONTEXT_i386
#define CONTEXT_i386    0x00010000    // this assumes that i386 and
#define CONTEXT_i486    0x00010000    // i486 have identical context records
#endif

#define WOW64_CS32   0x23*/
#include <Windows.h>

BOOL XP_GetThreadWow64Context(
					  __in HANDLE Thread,
					  __in_opt HANDLE Process,
					  __inout PWOW64_CONTEXT Ctx32
					  )
{
	THREAD_BASIC_INFORMATION ThreadInfo;
	PWOW64_CONTEXT           RemoteCtx;
	CONTEXT                  Ctx64;
	SIZE_T                   Transferred;
	ULONG                    ContextFlags;
	BOOLEAN                  CloseProcess;
	ULONG                    LastError;

	CloseProcess       = FALSE;

	Ctx64.ContextFlags = CONTEXT_ALL;

	//
	// Determine whether the thread is running in 64-bit mode or not.  If it is
	// running in 64-bit mode then we'll get the saved 32-bit context out of
	// the pointer in the 64-bit TEB's TLS slot.
	//
	// If, however, the thread is running in 32-bit mode, then we'll need to be
	// converting the 32-bit halves of the 64-bit registers into the
	// appropriate fields of the WOW64_CONTEXT structure.
	//

	if (!GetThreadContext(
		Thread,
		&Ctx64))
		return FALSE;

	if (Ctx64.SegCs != WOW64_CS32)
	{
		wprintf(L" -- Target is in 64-bit mode, returning saved 32-bit context\n");

		//
		// The thread is running in 64-bit mode.  We'll need to find the base
		// address of the thread's TEB and from there, read the remote TLS
		// slots and then the actual WOW64_CONTEXT structure residing within
		// the target.
		//
		// Also, if the caller didn't supply a process handle, then we'll
		// attempt to open one now using the thread's associated process ID in
		// the THREAD_BASIC_INFORMATION structure.  This is the typical
		// behavior of the Wow64 implementation of GetThreadContext.
		//

		if (!NT_SUCCESS(NtQueryInformationThread(
			Thread,
			ThreadBasicInformation,
			&ThreadInfo,
			sizeof( THREAD_BASIC_INFORMATION ),
			0
			)))
		{
			SetLastError( ERROR_INVALID_HANDLE );

			return FALSE;
		}

		//
		// If we don't have a process handle then we'll need to be opening one
		// ourselves for the VM read operation.
		//

		if (!Process)
		{
			Process = OpenProcess(
				PROCESS_VM_READ,
				FALSE,
				HandleToUlong(ThreadInfo.ClientId.UniqueProcess)
				);

			if (!Process)
				return FALSE;

			CloseProcess = TRUE;
		}

		//
		// Fetch the second TLS slot.  Wow64 assumes (hardcoded) that the
		// second TLS slot is the WOW64_CONTEXT.  This is a bit sloppy, but it
		// works out as there's not a 64-bit kernel32.dll loaded in a Wow64
		// process, so there won't be anyone calling TlsAlloc anyway.
		//

		if (!ReadProcessMemory(
			Process,
			&((PTEB)ThreadInfo.TebBaseAddress)->TlsSlots[ 1 ],
			&RemoteCtx,
			sizeof( PWOW64_CONTEXT ),
			&Transferred))
		{
			if (CloseProcess)
			{
				LastError = GetLastError();

				CloseHandle( Process );

				SetLastError( LastError );
			}

			return FALSE;
		}

		if (Transferred != sizeof( PWOW64_CONTEXT ))
		{
			if (CloseProcess)
				CloseHandle( Process );

			SetLastError( ERROR_PARTIAL_COPY );

			return FALSE;
		}

		//
		// Now that we've got the PWOW64_CONTEXT pointer, let's read the memory
		// where the actual context is stored.
		//
		// Note that this is unreliable if the remote thread is still running
		// when we make the call.
		//

		//
		// TODO: Pay attention to the caller's requested context flags and
		// filter the returned context appropriately.
		//

		if (!ReadProcessMemory(
			Process,
			(PUCHAR)(RemoteCtx) + sizeof( ULONG ),
			Ctx32,
			sizeof( WOW64_CONTEXT ),
			&Transferred
			))
		{
			if (CloseProcess)
			{
				LastError = GetLastError();

				CloseHandle( Process );

				SetLastError( LastError );
			}

			return FALSE;
		}

		if (Transferred != sizeof( WOW64_CONTEXT ))
		{
			if (CloseProcess)
				CloseHandle( Process );

			SetLastError( ERROR_PARTIAL_COPY );

			return FALSE;
		}

		//
		// All done.
		//
	}
	else
	{
		wprintf(L" -- Target is in 32-bit mode, truncating 64-bit registers\n");

		//
		// The requested thread is executing 32-bit instructions and is not in
		// the Wow64 layer (e.g. a system call).  This means that the 64-bit
		// thread context is actually reflective of the Wow64 context, except
		// that we need to convert from an x64 context strucuture to the
		// WOW64_CONTEXT structure (the high halves of registers are discarded
		// here).
		//

		ContextFlags = Ctx32->ContextFlags | CONTEXT_i386;

		ZeroMemory(
			Ctx32,
			sizeof( WOW64_CONTEXT )
			);

		Ctx32->ContextFlags = ContextFlags;

		if (ContextFlags & CONTEXT_CONTROL)
		{
			Ctx32->Eip    = (ULONG)Ctx64.Rip;
			Ctx32->EFlags = (ULONG)Ctx64.EFlags;
			Ctx32->Esp    = (ULONG)Ctx64.Rsp;
			Ctx32->Ebp    = (ULONG)Ctx64.Rbp;
		}

		if (ContextFlags & CONTEXT_INTEGER)
		{
			Ctx32->Eax = (ULONG)Ctx64.Rax;
			Ctx32->Ebx = (ULONG)Ctx64.Rbx;
			Ctx32->Ecx = (ULONG)Ctx64.Rcx;
			Ctx32->Edx = (ULONG)Ctx64.Rdx;
			Ctx32->Edi = (ULONG)Ctx64.Rdi;
			Ctx32->Esi = (ULONG)Ctx64.Rsi;
		}

		//
		// TODO: Convert other registers, pay attention to the caller's
		// requested context flags.  For example, floating point registers
		// would need to be handled here if support for them is desired.
		//

		//
		// All done.
		//
	}

	if (CloseProcess)
		CloseHandle( Process );

	return TRUE;
}