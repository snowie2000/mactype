#include "hash_list.h"

#define _CRT_SECURE_NO_WARNINGS

#undef get16bits
#if (defined(__GNUC__) && defined(__i386__)) || defined(__WATCOMC__) \
	|| defined(_MSC_VER) || defined (__BORLANDC__) || defined (__TURBOC__)
#define get16bits(d) (*((const uint16_t *) (d)))
#endif

#if !defined (get16bits)
#define get16bits(d) ((((uint32_t)(((const uint8_t *)(d))[1])) << 8)\
	+(uint32_t)(((const uint8_t *)(d))[0]) )
#endif

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
	UINT32 hash = SuperFastHash(buff, wcslen(buff))%m_Size;	//获得模数
	CMHashItem * hashitem = m_hashitem[hash].next;
	CMHashItem * parent = &m_hashitem[hash];
	while (hashitem && _wcsicmp(hashitem->String, buff))
	{
		parent = hashitem;
		hashitem = hashitem->next;
	}
	if (!hashitem)
	{
		parent->next = (CMHashItem*)malloc(sizeof(CMHashItem));	//还不存在这一项
		hashitem = parent->next;
		if (!m_bCaseSense)
			hashitem->String = buff;
		else
			hashitem->String = _wcsdup(buff);
		hashitem->Value = _wcsdup(Value);
		hashitem->next = NULL;
	}
	else
		if (!m_bCaseSense)
			free(buff);		//已经存在这一项了
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
	UINT32 hash = SuperFastHash(buff, wcslen(buff))%m_Size;	//获得模数
	CMHashItem * hashitem = m_hashitem[hash].next;
	CMHashItem * parent = &m_hashitem[hash];
	while (hashitem && _wcsicmp(hashitem->String, buff))
	{
		parent = hashitem;
		hashitem = hashitem->next;
	}
	if (hashitem)	//找到了这一项
	{
		parent->next=hashitem->next;
		free(hashitem->String);
		free(hashitem->Value);
		free(hashitem);
	}
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
	UINT32 hash = SuperFastHash(buff, wcslen(buff))%m_Size;	//获得模数
	CMHashItem * hashitem = m_hashitem[hash].next;
	while (hashitem && _wcsicmp(hashitem->String, buff))
		hashitem = hashitem->next;
	if (hashitem)	//找到了这一项
	{
		if (!m_bCaseSense)
			free(buff);
		return hashitem->Value;
	}
	else
	{
		if (!m_bCaseSense)
			free(buff);
		return NULL;	//找不到
	}
}

unsigned int MurmurHash2 ( const void * key, int len, unsigned int seed )
{
	// 'm' and 'r' are mixing constants generated offline.
	// They're not really 'magic', they just happen to work well.

	const unsigned int m = 0x5bd1e995;
	const int r = 24;

	// Initialize the hash to a 'random' value

	unsigned int h = seed ^ len;

	// Mix 4 bytes at a time into the hash

	const unsigned char * data = (const unsigned char *)key;

	while(len >= 4)
	{
		unsigned int k = *(unsigned int *)data;

		k *= m; 
		k ^= k >> r; 
		k *= m; 

		h *= m; 
		h ^= k;

		data += 4;
		len -= 4;
	}

	// Handle the last few bytes of the input array

	switch(len)
	{
	case 3: h ^= data[2] << 16;
	case 2: h ^= data[1] << 8;
	case 1: h ^= data[0];
		h *= m;
	};

	// Do a few final mixes of the hash to ensure the last few
	// bytes are well-incorporated.

	h ^= h >> 13;
	h *= m;
	h ^= h >> 15;

	return h;
} 


UINT32 CHashedStringList::SuperFastHash (const TCHAR * data, int len) 
{
	return MurmurHash2(data, len*2, 0);
	/*
	uint32_t hash = len, tmp;
		int rem;
	
		if (len <= 0 || data == NULL) return 0;
	
		rem = len & 3;
		len >>= 2;
	
		/ * Main loop * /
		for (;len > 0; len--) {
			hash  += get16bits (data);
			tmp    = (get16bits (data+2) << 11) ^ hash;
			hash   = (hash << 16) ^ tmp;
			data  += 2*sizeof (uint16_t);
			hash  += hash >> 11;
		}
	
		/ * Handle end cases * /
		switch (rem) {
	case 3: hash += get16bits (data);
		hash ^= hash << 16;
		hash ^= data[sizeof (uint16_t)] << 18;
		hash += hash >> 11;
		break;
	case 2: hash += get16bits (data);
		hash ^= hash << 11;
		hash += hash >> 17;
		break;
	case 1: hash += *data;
		hash ^= hash << 10;
		hash += hash >> 1;
		}
	
		/ * Force "avalanching" of final 127 bits * /
		hash ^= hash << 3;
		hash += hash >> 5;
		hash ^= hash << 4;
		hash += hash >> 17;
		hash ^= hash << 25;
		hash += hash >> 6;
	
		return hash;*/
	
}

CHashedStringList::CHashedStringList():m_Size(100), m_bCaseSense(false), m_Count(0)
{
	m_hashitem = (CMHashItem*)malloc(m_Size * sizeof(CMHashItem));	//创建一个HashItem组
	memset(m_hashitem, 0, m_Size * sizeof(CMHashItem));
}

CHashedStringList::CHashedStringList(int nSize, BOOL bCaseSensative):m_Size(nSize), m_bCaseSense(bCaseSensative), m_Count(0)
{
	m_hashitem = (CMHashItem*)malloc(nSize * sizeof(CMHashItem));	//创建一个HashItem组
	memset(m_hashitem, 0, nSize * sizeof(CMHashItem));
}

CHashedStringList::~CHashedStringList()
{
	for	(int i=0; i<m_Size; i++)
	{
		CMHashItem* hashitem=m_hashitem[i].next;	//第一项不用释放，一会儿整体释放掉
		CMHashItem* parent = hashitem;
		while (hashitem)
		{
			parent=hashitem;
			hashitem = hashitem->next;
			free(parent->String);
			free(parent->Value);
			free(parent);
		}
		if (hashitem)
		{
			free(parent->String);
			free(parent->Value);
			free(parent);
		}
	}
	free(m_hashitem);
}