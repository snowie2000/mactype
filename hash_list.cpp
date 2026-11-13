#include "hash_list.h"
#include <cwctype>
#include <algorithm>
#include <memory>

void CHashedStringList::Add(TCHAR * String, TCHAR * Value)
{
	std::wstring buff = String;
	if (!m_bCaseSense)
		std::transform(buff.begin(), buff.end(), buff.begin(), ::towlower);

	std::unique_ptr<wchar_t, decltype(&free)> value(_wcsdup(Value), &free);
	if (!value)
		return;
	AcquireSRWLockExclusive(&m_lock);
	const std::unique_ptr<SRWLOCK, decltype(&ReleaseSRWLockExclusive)> lock(
		&m_lock, &ReleaseSRWLockExclusive);
	if (stringmap.emplace(buff, value.get()).second)
		value.release();
}

TCHAR * CHashedStringList::Find(TCHAR * String)
{
	std::wstring buff = String;
	if (!m_bCaseSense)
		std::transform(buff.begin(), buff.end(), buff.begin(), ::towlower);
	AcquireSRWLockShared(&m_lock);
	const std::unique_ptr<SRWLOCK, decltype(&ReleaseSRWLockShared)> lock(
		&m_lock, &ReleaseSRWLockShared);
	const auto it = stringmap.find(buff);
	return it == stringmap.end() ? NULL : it->second;
}
