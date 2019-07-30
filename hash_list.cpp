#include "hash_list.h"

void CHashedStringList::Add(TCHAR * String, TCHAR * Value)
{
	TCHAR * buff;
	if (!m_bCaseSense)
	{
		buff = _wcsdup(String);
		_wcslwr(buff);
	}
	else
		 buff = String;

	strmap::iterator it = stringmap.find(buff);
	if (it == stringmap.end()) {
		stringmap[buff] = _wcsdup(Value);
	}
	else {
		if (!m_bCaseSense)
			free(buff);		//已经存在这一项了
	}
}

void CHashedStringList::Delete(TCHAR * String)
{
	TCHAR * buff;
	if (!m_bCaseSense)
	{
		buff = _wcsdup(String);
		_wcslwr(buff);
	}
	else
		buff = String;
	stringmap.erase(buff);
	if (!m_bCaseSense)
		free(buff);	
}

TCHAR * CHashedStringList::Find(TCHAR * String)
{
	TCHAR * buff;
	if (!m_bCaseSense)
	{
		buff = _wcsdup(String);
		_wcslwr(buff);
	}
	else
		buff = String;

	strmap::iterator it = stringmap.find(buff);
	if (!m_bCaseSense)
		free(buff);
	if (it != stringmap.end())
		return it->second;
	else
		return NULL;
}