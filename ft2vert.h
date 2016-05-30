
#ifdef __cplusplus
extern "C"{
#endif

/* store GSUB feature vert/vrt2 */
struct ft2vert_st *ft2vert_init(FT_Face face);
void ft2vert_final(FT_Face face, struct ft2vert_st *vert);

/* convert horizontal glyph index to vertical glyph index
 */
FT_UInt ft2vert_get_gid(const struct ft2vert_st *ft2vert, const FT_UInt gid);
FT_UInt ft2_subst_uvs(const FT_Face face, const FT_UInt gid, const FT_UInt vsindex, const FT_UInt baseChar);

#ifdef __cplusplus
}
#endif

