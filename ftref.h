#include <freetype/ftglyph.h>
#include <Windows.h>

#ifdef __cplusplus
extern "C"{
#endif

typedef struct  
{
	FT_Glyph ft_glyph;
	int refcount;
}FT_Referenced_GlyphRec, *FT_Referenced_Glyph;

typedef struct  
{
	FT_BitmapGlyph ft_glyph;
	int refcount;
}FT_Referenced_BitmapGlyphRec, *FT_Referenced_BitmapGlyph;

FT_Error FT_Glyph_Ref_Copy( FT_Referenced_Glyph source,  FT_Referenced_Glyph *target );
void FT_Done_Ref_Glyph( FT_Referenced_Glyph  *glyph );
void FT_Glyph_To_Ref_Glyph( FT_Glyph source, FT_Referenced_Glyph *target);
FT_Referenced_Glyph New_FT_Ref_Glyph();

#ifdef __cplusplus
}
#endif