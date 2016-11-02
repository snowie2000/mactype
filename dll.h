#include <windows.h>

typedef   BOOL (__stdcall *ProcDllMain)(HINSTANCE, DWORD,  LPVOID );

class CMemLoadDll
{
public:
	CMemLoadDll();
	~CMemLoadDll();
	BOOL    MemLoadLibrary( void* lpFileData , int DataLength, bool bInitDllMain, bool bFreeOnRavFail);  // Dll file data buffer
	FARPROC MemGetProcAddress(LPCSTR lpProcName);
	DWORD_PTR	GetImageBase() {return pImageBase;};
private:
	BOOL isLoadOk;
	BOOL CheckDataValide(void* lpFileData, int DataLength);
	int  CalcTotalImageSize();
	void CopyDllDatas(void* pDest, void* pSrc);
	BOOL FillRavAddress(void* pBase);
	void DoRelocation(void* pNewBase);
	int  GetAlignedSize(int Origin, int Alignment);
private:
	ProcDllMain pDllMain;


private:
	DWORD_PTR  pImageBase;
	bool   m_bInitDllMain;
	PIMAGE_DOS_HEADER pDosHeader;
	PIMAGE_NT_HEADERS32 pNTHeader;
	PIMAGE_SECTION_HEADER pSectionHeader;
};