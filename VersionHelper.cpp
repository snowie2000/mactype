#include "VersionHelper.h"


//////////////////////////////////////////////////////////////////////
// Konstruktion/Destruktion
//////////////////////////////////////////////////////////////////////

CVersionHelper::CVersionHelper()
{

}

CVersionHelper::~CVersionHelper()
{

}

/***********************************************************************************/
/*                                                                                 */
/* Class:   CVersionHelper                                                       */
/* Method:  GetVersionInfo                                                         */
/*                                                                                 */
/* Parameters:                                                                     */
/* -----------                                                                     */
/*   HMODULE hLib                                                                  */
/*                Handle to the module that contains the resource (EXE or DLL)     */
/*                A value of NULL specifies the current applications resources     */
/*                                                                                 */
/*   CString csEntry                                                               */
/*                Specifies the name of the resource. For more information,        */
/*                see the Remarks section.                                         */
/*                                                                                 */
/* Return Values:                                                                  */
/* --------------                                                                  */
/* If the function succeeds, the return value is a string containing the value     */
/* of the specified resource.                                                      */
/* If the function fails, the returned string is empty. To get extended error      */
/* information, call GetLastError.                                                 */
/*                                                                                 */
/* Remarks:                                                                        */
/* --------                                                                        */
/* Since the Win32 API resource information is encoded in Unicode, this method     */
/* also strips the strings from Unicode.                                           */
/*                                                                                 */
/* The following valid values for csEntry, as specified by Microsoft are:          */
/*   CompanyName, FileDescription, FileVersion, InternalName, LegalCopyright,      */
/*   OriginalFilename, ProductName, ProductVersion, Comments, LegalTrademarks,     */
/*   PrivateBuild, SpecialBuild                                                    */
/*                                                                                 */
/* Opening the rc-file as "text" or with a text-editor allows you to add further   */
/* entries to your version information structure and it is retrievable using       */
/* this same method.                                                               */
/*                                                                                 */
/***********************************************************************************/

wstring CVersionHelper::GetVersionInfo(HMODULE hLib, wstring csEntry)
{
	wstring csRet;

	if (hLib == NULL)
		return TEXT("");

	HRSRC hVersion = FindResource(hLib, MAKEINTRESOURCE(VS_VERSION_INFO), RT_VERSION);
	if (hVersion != NULL)
	{
		HGLOBAL hGlobal = LoadResource(hLib, hVersion);
		if (hGlobal != NULL)
		{

			LPVOID versionInfo = LockResource(hGlobal);
			if (versionInfo != NULL)
			{

				char    *pchVI = (char*)versionInfo;
				int dwSize = SizeofResource(hLib, hVersion);
				if (IsBadReadPtr(versionInfo, dwSize))
					dwSize -= 4;
				char    *pchVIcopy = new char[dwSize * 2 + 4];

				memcpy(pchVIcopy, pchVI, (int)(dwSize * 2 + 4));

				DWORD vLen, langD;
				BOOL retVal;

				LPVOID retbuf = NULL;

				static TCHAR fileEntry[256];

				wsprintf(fileEntry, TEXT("\\VarFileInfo\\Translation"));
				retVal = VerQueryValue(pchVIcopy, fileEntry, &retbuf, (UINT *)&vLen);
				if (retVal && vLen == 4)
				{
					memcpy(&langD, retbuf, 4);
					wsprintf(fileEntry, TEXT("\\StringFileInfo\\%02X%02X%02X%02X\\%s"),
						(langD & 0xff00) >> 8, langD & 0xff, (langD & 0xff000000) >> 24,
						(langD & 0xff0000) >> 16, csEntry);
				}
				else
					wsprintf(fileEntry, TEXT("\\StringFileInfo\\%04X04B0\\%s"), GetUserDefaultLangID(), csEntry);

				if (VerQueryValue(pchVIcopy, fileEntry, &retbuf, (UINT *)&vLen))
					csRet = (TCHAR*)retbuf;
				delete pchVIcopy;
			}
		}

		UnlockResource(hGlobal);
		FreeResource(hGlobal);
	}

	return csRet;
}

/***********************************************************************************/
/*                                                                                 */
/* Class:   CGlobalFunctions                                                       */
/* Method:  FormatVersion                                                          */
/*                                                                                 */
/* Parameters:                                                                     */
/* -----------                                                                     */
/*   CString cs                                                                    */
/*                Specifies a version number such as "FileVersion" or              */
/*                "ProductVersion" in the format "m, n, o, p"                      */
/*                (e.g. "1, 2, 3, a")                                              */
/*                                                                                 */
/* Return Values:                                                                  */
/* --------------                                                                  */
/* If the function succeeds, the return value is a string containing the version   */
/* in the format "m.nop" (e.g. "1.23a")                                            */
/*                                                                                 */
/* If the function fails, the returned string is empty.                            */
/*                                                                                 */
/***********************************************************************************/
wstring CVersionHelper::FormatVersion(wstring cs)
{
	wstring csRet;
	if (!cs.length())
	{
		rtrim(cs);
		int iPos = cs.find(',');
		if (iPos == -1)
			return TEXT("");
		ltrim(cs);
		rtrim(cs);
		csRet.Format(TEXT("%s."), cs.copy(iPos));

		while (1)
		{
			cs = cs.Mid(iPos + 1);
			ltrim(cs);
			iPos = cs.find(',');
			if (iPos == -1)
			{
				csRet += cs;
				break;
			}
			csRet += cs.Left(iPos);
		}
	}

	return csRet;
}

/***********************************************************************************/
/*                                                                                 */
/* Class:   CGlobalFunctions                                                       */
/* Method:  GetFileVersionX                                                        */
/*                                                                                 */
/* Parameters:                                                                     */
/* -----------                                                                     */
/*                                                                                 */
/* Return Values:                                                                  */
/* --------------                                                                  */
/* If the function succeeds, the return value is a wstring containing the           */
/* "FileVersion" in the format "m.nop" (e.g. "1.23a")                              */
/*                                                                                 */
/* If the function fails, the returned wstring is empty.                            */
/*                                                                                 */
/***********************************************************************************/
wstring CVersionHelper::GetFileVersionX()
{
	if (!m_csFileVersion.length())
	{
		wstring csVersion = FormatVersion(GetVersionInfo(NULL, TEXT("FileVersion")));
		m_csFileVersion.Format(TEXT("Version %s (Build %s)"), csVersion, GetVersionInfo(NULL, TEXT("SpecialBuild")));
	}

	return m_csFileVersion;
}

/***********************************************************************************/
/*                                                                                 */
/* Class:   CGlobalFunctions                                                       */
/* Method:  GetFileVersionX                                                        */
/*                                                                                 */
/* Parameters:                                                                     */
/* -----------                                                                     */
/*                                                                                 */
/* Return Values:                                                                  */
/* --------------                                                                  */
/* If the function succeeds, the return value is a string containing the           */
/* "ProductVersion" in the format "m.nop" (e.g. "1.23a")                           */
/*                                                                                 */
/* If the function fails, the returned string is empty.                            */
/*                                                                                 */
/***********************************************************************************/
wstring CVersionHelper::GetProductVersionX()
{
	if (!m_csProductVersion.length())
		m_csProductVersion = FormatVersion(GetVersionInfo(NULL, TEXT("ProductVersion")));

	return m_csProductVersion;
}