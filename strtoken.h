#pragma once

//—á
// 'a,b,c,d'		{ "a", "b", "c", "d" }
// 'a,,b,c,'		{ "a", "", "b", "c", "" }
// '"a,b",c'		{ ""a,b"", "c" }
class CStringTokenizer
{
private:
	enum _Const {
		MAX_ARGUMENTS	= 16,
	};
	int		m_nNumArgs;
	LPTSTR	m_ppszArgv[MAX_ARGUMENTS + 1];
	LPTSTR	m_pszBuffer;

	void Clear()
	{
		m_nNumArgs = 0;
		ZeroMemory(m_ppszArgv, sizeof(m_ppszArgv));
		free(m_pszBuffer);
		m_pszBuffer = NULL;
	}

public:
	CStringTokenizer()
		: m_pszBuffer(NULL)
	{
		Clear();
	}
	~CStringTokenizer()
	{
		free(m_pszBuffer);
	}

	int GetCount() const
	{
		return m_nNumArgs;
	}
	LPCTSTR GetArgument(int x) const
	{
		Assert(0 <= x && x <= GetCount());
		return m_ppszArgv[x];
	}

	int Parse(LPCTSTR pszStr)
	{
		Clear();

		if (!pszStr || !*pszStr) {
			return 0;
		}

		m_pszBuffer = _tcsdup(pszStr);
		if (!m_pszBuffer) {
			return 0;
		}
		LPTSTR p = m_pszBuffer, ps;
		int nNumArgs = 0;
		LPTSTR* ppszArgv = m_ppszArgv;

#define _isspace(ch)	(ch == _T('\t') || ch == _T(' '))
		for (;;) {
CONT:
			if (nNumArgs >= MAX_ARGUMENTS) {
				TRACE(_T("Too many arguments (> %d)\n"), MAX_ARGUMENTS);
				break;
			}

			if (*p == _T('\0')) {
				ppszArgv[nNumArgs++] = p;
				goto EXIT;
			}
			for (; _isspace(*p); *p++);
			if (*p == _T('\0')) {
				break;
			}

			switch (*p) {
			case _T(','):
				*p = _T('\0');
				ppszArgv[nNumArgs++] = p++;
				goto CONT;
			case _T('"'):
				p++;
				ppszArgv[nNumArgs++] = p++;
				for (; *p && *p != _T('"'); p++);
				if (*p == _T('\0')) {
					TRACE(_T("Unterminated string\n"));
					goto EXIT;
				}
				*p++ = _T('\0');
				break;
			default:
				ppszArgv[nNumArgs++] = p++;
				ps = p;
				for (; *p && *p != _T(','); p++);
				if (*p == _T('\0')) {
					goto EXIT;
				}
				for (p--; _isspace(*p) && p >= ps; p--);
				p++;
				break;
			}

			if (_isspace(*p)) {
				for (*p++ = _T('\0'); *p && _isspace(*p); p++);
				if (*p == _T('\0')) {
					break;
				}
			}
			for (; *p && *p != _T(','); p++);
			if (*p == _T('\0')) {
				break;
			}
			*p++ = _T('\0');
		}
#undef _isspace

EXIT:
		m_nNumArgs = nNumArgs;
		return nNumArgs;
	}
};
