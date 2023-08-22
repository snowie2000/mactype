#include "common.h"
#include <VersionHelpers.h>

#pragma once

#ifdef EASYHOOK

#include "easyhook.h"
#define HOOK_MANUALLY HOOK_DEFINE
#define HOOK_DEFINE(rettype, name, argtype, arglist) \
	extern rettype(WINAPI * ORIG_##name) argtype;  \
	extern HOOK_TRACE_INFO HOOK_##name;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

#define HOOK_MANUALLY(rettype, name, argtype, arglist) \
	extern LONG hook_demand_##name(bool bForce = false);
#define HOOK_DEFINE(rettype, name, argtype, arglist) ;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

#else

#define HOOK_MANUALLY HOOK_DEFINE
#define HOOK_DEFINE(rettype, name, argtype, arglist) \
	extern rettype(WINAPI * ORIG_##name) argtype;  \
	extern BOOL IsHooked_##name;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

#define HOOK_MANUALLY(rettype, name, argtype, arglist) \
	extern LONG hook_demand_##name(bool bForce = false);
#define HOOK_DEFINE(rettype, name, argtype, arglist) ;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

#endif