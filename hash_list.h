//#include "stdint.h"
#include "malloc.h"
#include "string.h"
#include "windows.h"

struct CMHashItem
{
	TCHAR* String;
	TCHAR* Value;
	CMHashItem* next;
};

class CHashedStringList
{
public:
	void Add(TCHAR * String, TCHAR * Value);
	void Delete(TCHAR * String);
	TCHAR * Find(TCHAR * String);
	CHashedStringList(int nSize, BOOL bCaseSensative = FALSE);
	CHashedStringList();
	~CHashedStringList();
protected:
	UINT32 SuperFastHash (const TCHAR * data, int len);
	//TCHAR * FindParent(TCHAR * String);	//查找这一项的前一项
private:
	int m_Count, m_Len, m_Size;
	CMHashItem* m_hashitem;
	BOOL m_bCaseSense;
};