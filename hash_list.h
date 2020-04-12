//#include "stdint.h"
#include "malloc.h"
#include "string.h"
#include "windows.h"
#include <map>
#include <string>

typedef std::map<std::wstring, LPTSTR> strmap;

class CHashedStringList
{
public:
	void Add(TCHAR * String, TCHAR * Value);
	void Delete(TCHAR * String);
	TCHAR * Find(TCHAR * String);
	CHashedStringList() : m_bCaseSense(false){}
	CHashedStringList(BOOL bCaseSensative) : m_bCaseSense(bCaseSensative){}
	~CHashedStringList(){
		strmap::iterator it = stringmap.begin();
		while (it != stringmap.end()) {
			free(it->second);
			++it;
		}
	}
protected:
private:
	strmap stringmap;
	BOOL m_bCaseSense;
};