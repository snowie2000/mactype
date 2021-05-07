#pragma once

template <class T>
class CTlsData
{
private:
	DWORD tlsindex;
	CPtrArray<T>* pArray;
	enum { INVALID_TLS_VALUE = 0xffffffff };

public:
	T* GetPtr() const
	{
		if(tlsindex == INVALID_TLS_VALUE)
			return NULL;

		T* pT = reinterpret_cast<T*>(::TlsGetValue(tlsindex));
		if(pT)
			return pT;

		pT = new T;
 		if(!pT)
 			return NULL;

		if(!::TlsSetValue(tlsindex, pT)) {
			delete pT;
			return NULL;
		}
		if(pArray) {
			pArray->Add(pT);
		}
		return pT;
	}
	bool ProcessInit()
	{
		tlsindex = ::TlsAlloc();
		if (tlsindex == INVALID_TLS_VALUE) {
			return false;
		}

		pArray = new CPtrArray<T>;
		if (!pArray) {
			::TlsFree(tlsindex);
			tlsindex = INVALID_TLS_VALUE;
			return false;
		}
		return true;
	}
	void ProcessTerm()
	{
		if(tlsindex == INVALID_TLS_VALUE) {
			return;
		}
		ThreadTerm();	//これ入れないとリークする
		::TlsFree(tlsindex);
		tlsindex = INVALID_TLS_VALUE;

		if (pArray) {
			for(int i = 0; i < pArray->GetSize(); i++) {
				delete pArray->operator[](i);
			}
			delete pArray;
			pArray = NULL;
		}
	}
//	bool ThreadInit()
//	{
//	}
	void ThreadTerm()
	{
		T* pT = reinterpret_cast<T*>(::TlsGetValue(tlsindex));
		if(pT) {
			if (pArray) {
				pArray->Remove(pT);
			}
			delete pT;
		}
		::TlsSetValue(tlsindex, NULL);
	}
};
