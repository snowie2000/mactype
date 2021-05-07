// EventLogging.cpp: implementation of the EventLogging class.
//
//////////////////////////////////////////////////////////////////////

#include "EventLogging.h"

#ifdef _DEBUG
#undef THIS_FILE
static char THIS_FILE[]=__FILE__;
#define new DEBUG_NEW
#endif

//////////////////////////////////////////////////////////////////////
// Construction/Destruction
//////////////////////////////////////////////////////////////////////

EventLogging::EventLogging()
//*******************************************************************************
//	Default Constructor is used register the event source
//******************************************************************************
{
	// returns a handle that links the source to the registry 
	this->m_hEventLinker = RegisterEventSource(NULL,L"MacType");

}

EventLogging::~EventLogging()
//*******************************************************************************
//	Destructor is used deregister the event source
//*******************************************************************************
{
	// Releases the handle to the registry
	DeregisterEventSource(m_hEventLinker);
}



void EventLogging::LogIt(WORD CategoryID, DWORD EventID, LPCTSTR ArrayOfStrings[],
						 UINT NumOfArrayStr,LPVOID RawData,DWORD RawDataSize)
//*******************************************************************************
//	Function is used to log the event into the .evt file.
//	Input:	CategoryID is the events category classification
//			EventID is the events event classification
//			ArrayOfStrings is an array of pointers to strings that are passed for additional information gathering
//			NumOfArrayStr is the number of of strings in ArrayOfStrings
//			RawData is a void pointer to hold additional raw data for event reporting
//			RawDataSize is the size of RawData in bytes
//*******************************************************************************
{

	// Writes data to the event log
	ReportEvent(m_hEventLinker,EVENTLOG_INFORMATION_TYPE,CategoryID,
		EventID,NULL,NumOfArrayStr,RawDataSize,ArrayOfStrings,RawData);	

}
