#include "hash_list.h"
#include <cwctype>
#include <algorithm>

void CHashedStringList::Add(TCHAR * String, TCHAR * Value)
{
	std::wstring buff = String;
	if (!m_bCaseSense)
		std::transform(buff.begin(), buff.end(), buff.begin(), ::towlower);

	strmap::iterator it = stringmap.find(buff);
	if (it == stringmap.end()) {
		stringmap[buff] = _wcsdup(Value);
	}
}

void CHashedStringList::Delete(TCHAR * String)
{
	std::wstring buff = String;
	if (!m_bCaseSense)
		std::transform(buff.begin(), buff.end(), buff.begin(), ::towlower);
	stringmap.erase(buff);
}

TCHAR * CHashedStringList::Find(TCHAR * String)
{
	std::wstring buff = String;
	if (!m_bCaseSense)
		std::transform(	buff.begin(), buff.end(),buff.begin(),::towlower);
	strmap::iterator it = stringmap.find(buff);
	if (it != stringmap.end())
		return it->second;
	else
		return NULL;
}