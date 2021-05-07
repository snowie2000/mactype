#include "common.h"

class AutoEnableDynamicCodeGen
{
public:
	AutoEnableDynamicCodeGen(bool enable = true);
	~AutoEnableDynamicCodeGen();

private:
	bool enabled;

	typedef
		BOOL
		(WINAPI *PSET_THREAD_INFORMATION_PROC)(
		_In_ HANDLE                   hThread,
		_In_ THREAD_INFORMATION_CLASS ThreadInformationClass,
		_In_reads_bytes_(ThreadInformationSize) PVOID ThreadInformation,
		_In_ DWORD                    ThreadInformationSize
		);

	typedef
		BOOL
		(WINAPI *PGET_THREAD_INFORMATION_PROC)(
		_In_ HANDLE                   hThread,
		_In_ THREAD_INFORMATION_CLASS ThreadInformationClass,
		_Out_writes_bytes_(ThreadInformationSize) PVOID ThreadInformation,
		_In_ DWORD                    ThreadInformationSize
		);

	static PSET_THREAD_INFORMATION_PROC SetThreadInformationProc;
	static PGET_THREAD_INFORMATION_PROC GetThreadInformationProc;
	static PROCESS_MITIGATION_DYNAMIC_CODE_POLICY processPolicy;
	static volatile bool processPolicyObtained;
};
