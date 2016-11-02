// EventLogging.h: interface for the EventLogging class.
//
//////////////////////////////////////////////////////////////////////
#include "common.h"
#if !defined(AFX_EVENTLOGGING_H__4AED0DCC_4C48_4312_BA6F_E6B90AC47F32__INCLUDED_)
#define AFX_EVENTLOGGING_H__4AED0DCC_4C48_4312_BA6F_E6B90AC47F32__INCLUDED_

#if _MSC_VER > 1000
#pragma once
#endif // _MSC_VER > 1000

class EventLogging 
{
public:
	EventLogging();
	virtual ~EventLogging();

	// Wrapper for ReportEvent that take care of Handle and EventType
	virtual void LogIt(WORD CategoryID, DWORD EventID, LPCTSTR ArrayOfStrings[] = NULL,
		UINT NumOfArrayStr = 0,LPVOID RawData = NULL,DWORD RawDataSize = 0);
	// data member to contain handle to registry
	HANDLE m_hEventLinker;


};

#endif // !defined(AFX_EVENTLOGGING_H__4AED0DCC_4C48_4312_BA6F_E6B90AC47F32__INCLUDED_)
