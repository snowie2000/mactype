#include "ftref.h"

#define InterlockedIncrementInt(x) InterlockedIncrement((volatile LONG *)&(x))
#define InterlockedDecrementInt(x) InterlockedDecrement((volatile LONG *)&(x))
#define InterlockedExchangeInt(x, y) InterlockedExchange((volatile LONG *)&(x), LONG(y))

FT_Error FT_Glyph_Ref_Copy( FT_Referenced_Glyph source,  FT_Referenced_Glyph *target )
{
	if (source->refcount<0)
		return 1;
	if (source->ft_glyph->format == FT_GLYPH_FORMAT_NONE)
		return 2;
	InterlockedIncrementInt(source->refcount);
	*target = source;
	return 0;
}

void FT_Done_Ref_Glyph( FT_Referenced_Glyph *glyph )
{
	if (InterlockedDecrementInt((*glyph)->refcount) == 0)
	{
		if ((*glyph)->ft_glyph && (*glyph)->ft_glyph->library)
			FT_Done_Glyph((*glyph)->ft_glyph);
		free(*glyph);
	}
	*glyph = NULL;
}

void FT_Glyph_To_Ref_Glyph( FT_Glyph source, FT_Referenced_Glyph *target)
{
	*target = (FT_Referenced_Glyph)malloc(sizeof(FT_Referenced_GlyphRec));
	(*target)->ft_glyph = source;
	(*target)->refcount = 1;
}

FT_Referenced_Glyph New_FT_Ref_Glyph()
{
	FT_Referenced_Glyph copy = (FT_Referenced_Glyph)malloc(sizeof(FT_Referenced_GlyphRec));
	copy->ft_glyph = NULL;
	copy->refcount = 1;
	return copy;
}
