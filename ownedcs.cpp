#include "ownedcs.h"
#include "windows.h"

#define InterlockedIncrementInt(x) InterlockedIncrement((volatile LONG *)&(x))
#define InterlockedDecrementInt(x) InterlockedDecrement((volatile LONG *)&(x))
#define InterlockedExchangeInt(x, y) InterlockedExchange((volatile LONG *)&(x), LONG(y))

void WINAPI InitializeOwnedCritialSection(POWNED_CRITIAL_SECTION cs)
{
	cs->nOwner = -1;
	cs->nRecursiveCount = 0;
	cs->nRequests = -1;
	cs->hEvent = CreateEvent(NULL, false, false, NULL);
	InitializeCriticalSection(&cs->threadLock);
}

void WINAPI DeleteOwnedCritialSection(POWNED_CRITIAL_SECTION cs)
{
	CloseHandle(cs->hEvent);
	DeleteCriticalSection(&cs->threadLock);
}


void WINAPI EnterOwnedCritialSection(POWNED_CRITIAL_SECTION cs, WORD Owner)
{
	EnterCriticalSection(&cs->threadLock);
	if (cs->nOwner == Owner)
	{
		InterlockedIncrementInt(cs->nRecursiveCount);
		LeaveCriticalSection(&cs->threadLock);
	}
	else
	{
		if (InterlockedIncrementInt(cs->nRequests)>0)  //等待获取所有权
		{
			LeaveCriticalSection(&cs->threadLock);
			WaitForSingleObject(cs->hEvent, INFINITE);
		}
		else
			LeaveCriticalSection(&cs->threadLock);
		InterlockedExchangeInt(cs->nOwner, Owner);//更改所有者
		InterlockedExchangeInt(cs->nRecursiveCount, 1);//增加占用计数
	}
}

void WINAPI LeaveOwnedCritialSection(POWNED_CRITIAL_SECTION cs, WORD Owner)
{
	EnterCriticalSection(&cs->threadLock);
	if (cs->nOwner == Owner)
	{
		if (InterlockedDecrementInt(cs->nRecursiveCount)<=0)
		{
			InterlockedExchangeInt(cs->nOwner, -1);//�黹����Ȩ
			if (InterlockedDecrementInt(cs->nRequests)>=0)
				SetEvent(cs->hEvent);
		}
	}
	else
		InterlockedDecrementInt(cs->nRecursiveCount);
	LeaveCriticalSection(&cs->threadLock);
}
