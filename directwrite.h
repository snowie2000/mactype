#include "common.h"
#include <VersionHelpers.h>
#include "easyhook.h"

#pragma once

#define HOOK_MANUALLY HOOK_DEFINE
#define HOOK_DEFINE(rettype, name, argtype) \
	extern rettype(WINAPI * ORIG_##name) argtype;  \
	extern HOOK_TRACE_INFO HOOK_##name;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY

#define HOOK_MANUALLY(rettype, name, argtype) \
	extern LONG hook_demand_##name(bool bForce = false);
#define HOOK_DEFINE(rettype, name, argtype) ;
#include "hooklist.h"
#undef HOOK_DEFINE
#undef HOOK_MANUALLY