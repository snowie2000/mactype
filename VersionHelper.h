#pragma once
#include <xstring>
#include <windows.h>
#include <algorithm>
#include <cctype>
using namespace std;

class CVersionHelper
{
public:
	CVersionHelper();
	virtual ~CVersionHelper();

public:
	wstring GetFileVersionX();
	wstring GetProductVersionX();
	wstring GetVersionInfo(HMODULE hLib, wstring csEntry);
	wstring FormatVersion(wstring cs);

private:
	wstring m_csFileVersion;
	wstring m_csProductVersion;

};


// cited: https://stackoverflow.com/questions/216823/whats-the-best-way-to-trim-stdstring
// trim from start (in place)
static inline void ltrim(std::wstring &s) {
	s.erase(s.begin(), std::find_if(s.begin(), s.end(), [](int ch) {
		return !std::isspace(ch);
	}));
}

// trim from end (in place)
static inline void rtrim(std::wstring &s) {
	s.erase(std::find_if(s.rbegin(), s.rend(), [](int ch) {
		return !std::isspace(ch);
	}).base(), s.end());
}