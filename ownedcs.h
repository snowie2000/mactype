#include <windows.h>

typedef struct _OWNED_CRITIAL_SECTION 
{
	int nOwner,	nRequests,	nRecursiveCount;
	HANDLE hEvent;
	CRITICAL_SECTION threadLock;
}OWNED_CRITIAL_SECTION, *POWNED_CRITIAL_SECTION;
	//用于自定义临界区

void WINAPI InitializeOwnedCritialSection(POWNED_CRITIAL_SECTION cs);
void WINAPI DeleteOwnedCritialSection(POWNED_CRITIAL_SECTION cs);
void WINAPI EnterOwnedCritialSection(POWNED_CRITIAL_SECTION cs, WORD Owner);
void WINAPI LeaveOwnedCritialSection(POWNED_CRITIAL_SECTION cs, WORD Owner);