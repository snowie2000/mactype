/*
 * "ft2vert.c"
 * 
 * Converter to vertical glyph ID by handling GSUB vrt2/vert feature
 * requires FreeType-2.1.10 or latter
 *
 * (C) 2005 Nobuyuki TSUCHIMURA
 *
 * using such Lookup
 *   ScriptTag == 'kana'
 *   DefaultLangSys or LangSysTag == 'JAN '
 *   FeatureTag == 'vrt2' or 'vert'
 *
 * [reference]
 * http://partners.adobe.com/public/developer/opentype/index_table_formats1.html
 * http://partners.adobe.com/public/developer/opentype/index_table_formats.html
 * http://partners.adobe.com/public/developer/opentype/index_tag9.html#vrt2
 */

//#include "xdvi-config.h"
//#include "xdvi.h"
//#ifdef        USE_ZEIT
#include <windows.h>
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OPENTYPE_VALIDATE_H

#include <stdio.h>
#include <stdlib.h>
#include "ft2vert.h"
const struct ivs_otft_desc {
	int baseChar;
	int otft_index;
} ivs_otft[] = {
#define OTFT_DEF_BEGIN(index) /**/
#define OTFT_DEF(baseChar, otft_index) { baseChar, otft_index },
#define OTFT_DEF_END(count) /**/
#include "ivs_otft.h"
#undef OTFT_DEF_BEGIN
#undef OTFT_DEF
#undef OTFT_DEF_END
};
const int ivs_otft_index[] = {
#define OTFT_DEF_BEGIN(index) index,
#define OTFT_DEF(baseChar, otft_index) /**/
#define OTFT_DEF_END(count) /**/
#include "ivs_otft.h"
#undef OTFT_DEF_BEGIN
#undef OTFT_DEF
#undef OTFT_DEF_END
};
const int ivs_otft_count[] = {
#define OTFT_DEF_BEGIN(index) /**/
#define OTFT_DEF(baseChar, otft_index) /**/
#define OTFT_DEF_END(count) count,
#include "ivs_otft.h"
#undef OTFT_DEF_BEGIN
#undef OTFT_DEF
#undef OTFT_DEF_END
};

#define TAG_KANA FT_MAKE_TAG('k', 'a', 'n', 'a')
#define TAG_HANI FT_MAKE_TAG('h', 'a', 'n', 'i')
#define TAG_JAN  FT_MAKE_TAG('J', 'A', 'N', ' ')
#define TAG_VERT FT_MAKE_TAG('v', 'e', 'r', 't')
#define TAG_VRT2 FT_MAKE_TAG('v', 'r', 't', '2')
#define TAG_JP78 FT_MAKE_TAG('j', 'p', '7', '8')
#define TAG_JP90 FT_MAKE_TAG('j', 'p', '9', '0')
#define TAG_JP04 FT_MAKE_TAG('j', 'p', '0', '4')

#define VERT_LOOKUP_INDEX 0
#define JP78_LOOKUP_INDEX 1
#define JP90_LOOKUP_INDEX 2
#define JP04_LOOKUP_INDEX 3

#define MALLOC(ptr, size) ptr = malloc(sizeof((ptr)[0]) * (size))
#define BYTE2(p) ((p) += 2, (int)(p)[-2] << 8  | (p)[-1])
#define BYTE4(p) ((p) += 4, (int)(p)[-4] << 24 | (int)(p)[-3] << 16 | \
                  (int)(p)[-2] << 8 | (p)[-1])

struct ft2vert_st {
    struct Lookup_st {
        int SubTableCount;
        struct SubTable_st {
            struct SingleSubst_st {
                FT_UInt SubstFormat;
                FT_UInt DeltaGlyphID; /* SubstFormat == 1 */
                int     GlyphCount;   /* SubstFormat == 2 */
                FT_UInt *Substitute;  /* SubstFormat == 2 */
            } SingleSubst;
            struct Coverage_st {
                FT_UInt CoverageFormat;
                int     GlyphCount;   /* CoverageFormat == 1 */
                FT_UInt *GlyphArray;  /* CoverageFormat == 1 */
                int     RangeCount;   /* CoverageFormat == 2 */
                struct  RangeRecord_st
                       *RangeRecord;  /* CoverageFormat == 2 */
            } Coverage;
        } *SubTable;
    } Lookup[4];
    FT_Bytes GSUB_table;
    FT_Bytes kanaFeature, haniFeature;
    FT_Bytes vertLookup, vrt2Lookup;
    FT_Bytes jp78Lookup, jp90Lookup, jp04Lookup;
    FT_UInt32* variantSelectors;
};

struct RangeRecord_st {
    FT_UInt Start;
    FT_UInt End;
};


int isInIndex(FT_Bytes s, int index) {
    int i, count;

    if (s == NULL) return FALSE;
    count = BYTE2(s);
    for (i = 0; i < count; i++) {
        if (index == BYTE2(s)) return TRUE;
    }
    return FALSE;
}


/**********  Debug ***************/

#ifdef DEBUG
static FT_Bytes gsub_top;

void print_offset(char *message, const FT_Bytes ptr) {
    printf("%s offset = %x\n", message, ptr - gsub_top);
}

char *tag_to_string(FT_Tag tag) {
    static char str[5];
    str[0] = tag >> 24;
    str[1] = tag >> 16;
    str[2] = tag >> 8;
    str[3] = tag;
    return str;
}

void hex_dump(const FT_Bytes top) {
    int i, j;
    FT_Bytes s = top;

    for (j = 0; j < 100; j++) {
        printf("%04x : ", j*8);
        for (i = 0; i < 8; i++) {
            printf("%02x ", s[i+j*8]);
        }
        printf("\n");
    }
}
#endif /* DEBUG */

/**********  Lookup part ***************/

void scan_Coverage(struct ft2vert_st *ret, const FT_Bytes top, const int l) {
    int i;
    FT_Bytes s = top;
    struct Coverage_st *t;

    t = &ret->Lookup[l].SubTable[ret->Lookup[l].SubTableCount].Coverage;
    t->CoverageFormat = BYTE2(s);
    switch (t->CoverageFormat) {
    case 1: 
        t->GlyphCount = BYTE2(s);
        MALLOC(t->GlyphArray, t->GlyphCount);
        memset(t->GlyphArray, 0, sizeof(t->GlyphArray[0]) * t->GlyphCount);
        for (i = 0; i < t->GlyphCount; i++) {
            t->GlyphArray[i] = BYTE2(s);
        }
        break;
    case 2:
        t->RangeCount = BYTE2(s);
        MALLOC(t->RangeRecord, t->RangeCount);
        memset(t->RangeRecord, 0, sizeof(t->RangeRecord[0]) * t->RangeCount);
        for (i = 0; i < t->RangeCount; i++) {
            t->RangeRecord[i].Start = BYTE2(s);
            t->RangeRecord[i].End   = BYTE2(s);
            s += 2; /* drop StartCoverageIndex */
        }
        break;
    default:
#ifdef _DEBUG
        fprintf(stderr, "scan_Coverage: unknown CoverageFormat (%d).",
                t->CoverageFormat);
#endif
        exit(1);
    }
    ret->Lookup[l].SubTableCount++;
}

void scan_SubTable(struct ft2vert_st *ret, const FT_Bytes top, const int l) {
    int i;
    FT_Bytes s = top;
    FT_Offset Coverage;
    struct SingleSubst_st *t;

    t = &ret->Lookup[l].SubTable[ret->Lookup[l].SubTableCount].SingleSubst;
    t->SubstFormat = BYTE2(s);
    Coverage       = BYTE2(s);
    scan_Coverage(ret, top + Coverage, l);
    switch (t->SubstFormat) {
    case 1: /* SingleSubstFormat1 */
        t->DeltaGlyphID = BYTE2(s);
        break;
    case 2: /* SingleSubstFormat2 */
        t->GlyphCount   = BYTE2(s);
        MALLOC(t->Substitute, t->GlyphCount);
        memset(t->Substitute, 0, sizeof(t->Substitute[0]) * t->GlyphCount);
        for (i = 0; i < t->GlyphCount; i++) {
            t->Substitute[i] = BYTE2(s);
        }
        break;
    default:
#ifdef _DEBUG
        fprintf(stderr, "scan_SubTable: unknown SubstFormat (%d).",
                t->SubstFormat);
#endif
        exit(1);
    }
}

void scan_Lookup(struct ft2vert_st *ret, const FT_Bytes top, const int l) {
    int i;
    FT_Bytes s = top;
    FT_UShort LookupType;
    FT_UShort LookupFlag;
    FT_UShort SubTableCount;
    FT_UShort SubTable;

    LookupType    = BYTE2(s);
    LookupFlag    = BYTE2(s);
    SubTableCount = BYTE2(s);
    SubTable      = BYTE2(s);

    MALLOC(ret->Lookup[l].SubTable, SubTableCount);
    memset(ret->Lookup[l].SubTable, 0, sizeof(ret->Lookup[l].SubTable[0]) * SubTableCount);
    for (i = 0; i < SubTableCount; i++) {
        scan_SubTable(ret, top + SubTable, l);
    }
    if (ret->Lookup[l].SubTableCount != SubTableCount) {

#ifdef _DEBUG
        fprintf(stderr, "warning (scan_Lookup): "
                "SubTableCount (=%d) is not expected (=%d).\n",
                ret->Lookup[l].SubTableCount, SubTableCount);
#endif
    }
}


void scan_LookupList(struct ft2vert_st *ret, const FT_Bytes top) {
    int i;
    FT_Bytes s = top;
    int LookupCount;

    LookupCount = BYTE2(s);

    for (i = 0; i < LookupCount; i++) {
        FT_Bytes t = top + BYTE2(s);
        if (isInIndex(ret->vertLookup, i)) {
            scan_Lookup(ret, t, VERT_LOOKUP_INDEX);
        } else if (isInIndex(ret->jp78Lookup, i)) {
            scan_Lookup(ret, t, JP78_LOOKUP_INDEX);
        } else if (isInIndex(ret->jp90Lookup, i)) {
            scan_Lookup(ret, t, JP90_LOOKUP_INDEX);
        } else if (isInIndex(ret->jp04Lookup, i)) {
            scan_Lookup(ret, t, JP04_LOOKUP_INDEX);
        }
    }
}

/********** Feature part ****************/

void scan_FeatureList(struct ft2vert_st *ret, const FT_Bytes top) {
    int i;
    FT_Bytes s = top;
    int FeatureCount;

    FeatureCount = BYTE2(s);

    for (i = 0; i < FeatureCount; i++) {
        FT_Tag FeatureTag = BYTE4(s);
        FT_Offset Feature = BYTE2(s);
        if (isInIndex(ret->kanaFeature, i)) {
            switch (FeatureTag) {
            case TAG_VERT:
                ret->vertLookup = top + Feature + 2;
                break;
            case TAG_VRT2:
                ret->vrt2Lookup = top + Feature + 2;
                break;
            }
        } else if (isInIndex(ret->haniFeature, i)) {
            switch (FeatureTag) {
            case TAG_JP78:
                ret->jp78Lookup = top + Feature + 2;
                break;
            case TAG_JP90:
                ret->jp90Lookup = top + Feature + 2;
                break;
            case TAG_JP04:
                ret->jp04Lookup = top + Feature + 2;
                break;
            }
        }
    }
}

/********** Script part ****************/

void scan_LangSys(struct ft2vert_st *ret, const FT_Bytes top, const FT_Tag ScriptTag) {
    if (ScriptTag == TAG_KANA && ret->kanaFeature == NULL) ret->kanaFeature = top + 4;
    if (ScriptTag == TAG_HANI && ret->haniFeature == NULL) ret->haniFeature = top + 4;
}

void scan_Script(struct ft2vert_st *ret, const FT_Bytes top, const FT_Tag ScriptTag) {
    int i;
    FT_Bytes s = top;
    FT_Offset DefaultLangSys;
    int LangSysCount;

    DefaultLangSys = BYTE2(s);
    if (DefaultLangSys != 0) {
        scan_LangSys(ret, top + DefaultLangSys, ScriptTag);
    }
    LangSysCount = BYTE2(s);

    for (i = 0; i < LangSysCount; i++) {
        FT_Tag LangSysTag = BYTE4(s);
        FT_Bytes t = top + BYTE2(s);
        if (LangSysTag == TAG_JAN) {
            scan_LangSys(ret, t, ScriptTag);
        }
    }
}

void scan_ScriptList(struct ft2vert_st *ret, const FT_Bytes top) {
    int i;
    FT_Bytes s = top;
    int ScriptCount;

    ScriptCount = BYTE2(s);

    for (i = 0; i < ScriptCount; i++) {
        FT_Tag ScriptTag = BYTE4(s);
        FT_Bytes t = top + BYTE2(s);
        if (ScriptTag == TAG_KANA || ScriptTag == TAG_HANI) {
            scan_Script(ret, t, ScriptTag);
        }
    }
}

/********** header part *****************/

void scan_GSUB_Header(struct ft2vert_st *ret, const FT_Bytes top) {
    FT_Bytes s = top;
    FT_Fixed  Version;
    FT_Offset ScriptList;
    FT_Offset FeatureList;
    FT_Offset LookupList;

#ifdef DEBUG
    gsub_top    = top;
#endif /* DEBUG */
    Version     = BYTE4(s);
    ScriptList  = BYTE2(s);
    FeatureList = BYTE2(s);
    LookupList  = BYTE2(s);

    if (Version != 0x00010000) {
#ifdef _DEBUG
        fprintf(stderr, "warning: GSUB Version (=%.1f) is not 1.0\n",
                (double)Version / 0x10000);
#endif
    }

    scan_ScriptList(ret, top + ScriptList);
    scan_FeatureList(ret, top + FeatureList);
    /* vrt2 has higher priority over vert */
    if (ret->vrt2Lookup != NULL) ret->vertLookup = ret->vrt2Lookup;
    scan_LookupList(ret, top + LookupList);
}

struct ft2vert_st *ft2vert_init(FT_Face face) {
    struct ft2vert_st *ret;
    int ft_error, i;
    FT_Bytes base = NULL;
    FT_Bytes gdef = NULL;
    FT_Bytes gpos = NULL;
    FT_Bytes jstf = NULL;

    MALLOC(ret, 1);
    memset(ret, 0, sizeof(ret[0]));
    for (i = 0; i < sizeof(ret->Lookup) / sizeof(ret->Lookup[0]); i++) {
        ret->Lookup[i].SubTableCount = 0;
    }
    ret->vertLookup  = NULL;
    ret->vrt2Lookup  = NULL;
    ret->jp78Lookup  = NULL;
    ret->jp90Lookup  = NULL;
    ret->jp04Lookup  = NULL;
    ret->kanaFeature = NULL;
    ret->haniFeature = NULL;
    ret->GSUB_table  = NULL;
    ret->variantSelectors = FT_Face_GetVariantSelectors(face);
    ft_error = FT_OpenType_Validate(
                face, FT_VALIDATE_GSUB,
                &base, &gdef, &gpos, &ret->GSUB_table, &jstf);
    if (ft_error == FT_Err_Unimplemented_Feature) {
#ifdef _DEBUG
		fprintf(stderr, "warning: FT_OpenType_Validate is disabled. "
                "Replace FreeType2 with otvalid-enabled version.\n");
#endif        
		return ret;
    } else if (ft_error != 0 || ret->GSUB_table == 0) {
#ifdef _DEBUG
        fprintf(stderr, "warning: %s has no GSUB table.\n",
                face->family_name);
#endif  
        return ret;
    }
    scan_GSUB_Header(ret, ret->GSUB_table);
    if (ret->Lookup[VERT_LOOKUP_INDEX].SubTableCount == 0) {
#ifdef _DEBUG
        fprintf(stderr, "warning: %s has no vrt2/vert feature.\n",
                face->family_name);
#endif
    }
//    free((void*)GSUB_table);
    return ret;
}

void ft2vert_final(FT_Face face, struct ft2vert_st *vert){
    int i, j;
    for (i = 0; i < sizeof(vert->Lookup) / sizeof(vert->Lookup[0]); i++) {
        for (j = 0; j < vert->Lookup[i].SubTableCount; j++) {
            free(vert->Lookup[i].SubTable[j].SingleSubst.Substitute);
            free(vert->Lookup[i].SubTable[j].Coverage.GlyphArray);
            free(vert->Lookup[i].SubTable[j].Coverage.RangeRecord);
        }
        free(vert->Lookup[i].SubTable);
    }
    FT_OpenType_Free(face, vert->GSUB_table);
    free(vert);
//    FT_Bytes kanaFeature;
//    FT_Bytes haniFeature;
//    FT_Bytes vertLookup;
//    FT_Bytes vrt2Lookup;
//    FT_Bytes jp78Lookup;
//    FT_Bytes jp90Lookup;
//    FT_Bytes jp04Lookup;
//    FT_UInt32* variantSelectors;
}

/********** converting part *****************/

static FT_UInt get_vert_nth_gid(struct SubTable_st *t, FT_UInt gid, int n) {
    switch (t->SingleSubst.SubstFormat) {
    case 1:
        return gid + t->SingleSubst.DeltaGlyphID;
    case 2:
        return t->SingleSubst.Substitute[n];
    }
#ifdef _DEBUG
    fprintf(stderr, "get_vert_nth_gid: internal error");
#endif
    exit(1);
    //return 0;
}


FT_UInt ft2gsub_get_gid(const struct ft2vert_st *ft2vert, const FT_UInt gid, const int l) {
    int i, k;
    int j = 0; /* StartCoverageIndex */

    for (k = 0; k < ft2vert->Lookup[l].SubTableCount; k++) {
        struct SubTable_st *t = &ft2vert->Lookup[l].SubTable[k];
        switch (t->Coverage.CoverageFormat) {
        case 1:
            for (i = 0; i < t->Coverage.GlyphCount; i++) {
                if (t->Coverage.GlyphArray[i] == gid) {
                    return get_vert_nth_gid(t, gid, i);
                }
            }
            break;
        case 2:
            for (i = 0; i < t->Coverage.RangeCount; i++) {
                struct RangeRecord_st *r = &t->Coverage.RangeRecord[i];
                if (r->Start <= gid && gid <= r->End) {
                    return get_vert_nth_gid(t, gid, gid - r->Start + j);
                }
                j += r->End - r->Start + 1;
            }
            break;
        default:
#ifdef _DEBUG
            fprintf(stderr, "ft2vert_get_gid: internal error");
#endif
            exit(1);
        }
    }
    return 0;
}

FT_UInt ft2vert_get_gid(const struct ft2vert_st *ft2vert, const FT_UInt gid) {
    FT_UInt tmp = ft2gsub_get_gid(ft2vert, gid, VERT_LOOKUP_INDEX);
    return tmp ? tmp : gid;
}

int glyphs_comp(const void *_a, const void *_b) {
	const struct ivs_otft_desc *a = (const struct ivs_otft_desc *)_a;
	const struct ivs_otft_desc *b = (const struct ivs_otft_desc *)_b;

	if (a->baseChar < b->baseChar)
		return -1;
	else if (a->baseChar > b->baseChar)
		return 1;
	return 0;
}

FT_UInt ft2_subst_uvs(const FT_Face face, const FT_UInt gid, const FT_UInt vsindex, const FT_UInt baseChar)
{
	FT_UInt newglyph;
	const struct ivs_otft_desc key = { baseChar }, *found;
	struct ft2vert_st *ft2vert = (struct ft2vert_st *)face->generic.data;
	if (!ft2vert)
		return 0;

	// 存在するならUVS Subtableから探す
	if (ft2vert->variantSelectors)
		return FT_Face_GetCharVariantIndex(face, baseChar, 0xE0100 + vsindex);

	// GSUBテーブルのOpenType featureによりシミュレートする
	if (vsindex >= sizeof ivs_otft_index / sizeof ivs_otft_index[0])
		return 0;
	found = (const struct ivs_otft_desc *)bsearch(&key, ivs_otft + ivs_otft_index[vsindex], ivs_otft_count[vsindex], sizeof(struct ivs_otft_desc), glyphs_comp);
	if (!found)
		return 0;

	// シミュレートできるfeatureが見つかったので置換を試みる。
	newglyph = ft2gsub_get_gid(ft2vert, gid, found->otft_index);
	// 置換に成功したらそれを返す
	if (newglyph)
		return newglyph;
	// フォントがGSUBテーブルに置換定義を持っていない。
	// 'jp04'を持っているが'jp90'を持っていないときはJIS90フォントとみなし、
	// 'jp90'を持っているが'jp04'を持っていないときはJIS2004フォントとみなす。
	// JIS90フォントに'jp90'を要求された場合とJIS2004フォント'jp04'を要求された場合は
	// デフォルト字形が要求された字形であるとみなしてそのまま返す。
	if (ft2vert->jp04Lookup && !ft2vert->jp90Lookup && found->otft_index == JP90_LOOKUP_INDEX
		|| ft2vert->jp90Lookup && !ft2vert->jp04Lookup && found->otft_index == JP04_LOOKUP_INDEX)
		return gid;
	// どちらでもなければフォントは要求された字形を持っていないとみなす。
	return 0;
}

//#endif        /* USE_ZEIT */
