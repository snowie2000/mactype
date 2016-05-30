/*  stdint.h

    Integer types - c99 7.18
*/

/*
 *      C/C++ Run Time Library - Version 13.0
 *
 *      Copyright (c) 2002, 2007 by CodeGear
 *      All Rights Reserved.
 *
 */


#ifndef __STDINT_H
#define __STDINT_H

/* 7.18.1.1 Exact-width integer types */

typedef __int8 int8_t;
typedef __int16 int16_t;
typedef __int32 int32_t;
typedef __int64 int64_t;

typedef unsigned __int8 uint8_t;
typedef unsigned __int16 uint16_t;
typedef unsigned __int32 uint32_t;
typedef unsigned __int64 uint64_t;



/* 7.18.1.2 Minimum-width integer types */

typedef __int8 int_least8_t;
typedef __int16 int_least16_t;
typedef __int32 int_least32_t;
typedef __int64 int_least64_t;

typedef unsigned __int8 uint_least8_t;
typedef unsigned __int16 uint_least16_t;
typedef unsigned __int32 uint_least32_t;
typedef unsigned __int64 uint_least64_t;



/* 7.18.1.3 Fastest minimum-width integer types */

typedef __int8 int_fast8_t;
typedef __int16 int_fast16_t;
typedef __int32 int_fast32_t;
typedef __int64 int_fast64_t;

typedef unsigned __int8 uint_fast8_t;
typedef unsigned __int16 uint_fast16_t;
typedef unsigned __int32 uint_fast32_t;
typedef unsigned __int64 uint_fast64_t;



/* 7.18.1.4 Integer types capable of holding object pointers */

typedef int32_t intptr_t;
typedef uint32_t uintptr_t;



/* 7.18.1.5 Greatest-width integer types */

typedef int64_t intmax_t;
typedef uint64_t uintmax_t;



/* 7.18.2.1 Limits of exact-width integer types */

#define INT8_MIN ((int8_t) -128)
#define INT16_MIN ((int16_t) -32768)
#define INT32_MIN ((int32_t) -2147483647L - 1) 
#define INT64_MIN ((int64_t) -9223372036854775807LL - 1)

#define INT8_MAX ((int8_t) 127)
#define INT16_MAX ((int16_t) 32767)
#define INT32_MAX ((int32_t) 2147483647L)
#define INT64_MAX ((int64_t) 9223372036854775807LL)

#define UINT8_MAX ((uint8_t) 255)
#define UINT16_MAX ((uint16_t) 65535)
#define UINT32_MAX ((uint32_t) 4294967295UL)
#define UINT64_MAX ((uint64_t) 18446744073709551615ULL)



/* 7.18.2.2 Limits of minimum-width integer types */

#define INT_LEAST8_MIN ((int_least8_t) -128)
#define INT_LEAST16_MIN ((int_least16_t) -32768)
#define INT_LEAST32_MIN ((int_least32_t) -2147483647L - 1)
#define INT_LEAST64_MIN ((int_least64_t) -9223372036854775807LL - 1)

#define INT_LEAST8_MAX ((int_least8_t) 127)
#define INT_LEAST16_MAX ((int_least16_t) 32767)
#define INT_LEAST32_MAX ((int_least32_t) 2147483647L)
#define INT_LEAST64_MAX ((int_least64_t) 9223372036854775807LL)

#define UINT_LEAST8_MAX ((uint_least8_t) 255)
#define UINT_LEAST16_MAX ((uint_least16_t) 65535)
#define UINT_LEAST32_MAX ((uint_least32_t) 4294967295UL)
#define UINT_LEAST64_MAX ((uint_least64_t) 18446744073709551615ULL)



/* 7.18.2.3 Limits of fastest minimum-width integer types */

#define INT_FAST8_MIN ((int_fast8_t) -128)
#define INT_FAST16_MIN ((int_fast16_t) -32768)
#define INT_FAST32_MIN ((int_fast32_t) -2147483647L - 1)
#define INT_FAST64_MIN ((int_fast64_t) -9223372036854775807LL - 1)

#define INT_FAST8_MAX ((int_fast8_t) 127)
#define INT_FAST16_MAX ((int_fast16_t) 32767)
#define INT_FAST32_MAX ((int_fast32_t) 2147483647L)
#define INT_FAST64_MAX ((int_fast64_t) 9223372036854775807LL)

#define UINT_FAST8_MAX ((uint_fast8_t) 255)
#define UINT_FAST16_MAX ((uint_fast16_t) 65535)
#define UINT_FAST32_MAX ((uint_fast32_t) 4294967295UL)
#define UINT_FAST64_MAX ((uint_fast64_t) 18446744073709551615ULL)



/* 7.18.2.4 Limits of integer types capable of holding object pointers */

#define INTPTR_MIN ((intptr_t) -2147483647L - 1)
#define INTPTR_MAX ((intptr_t) 2147483647L)
#define UINTPTR_MAX ((intptr_t) 4294967295UL)



/* 7.18.2.5 Limits of greatest-width integer types */

#define INTMAX_MIN ((intmax_t) -9223372036854775807LL - 1)
#define INTMAX_MAX ((intmax_t) 9223372036854775807LL)
#define UINTMAX_MAX ((uintmax_t) 18446744073709551615ULL)



/* 7.18.3 Limits of other integer types */

#define PTRDIFF_MIN ((int32_t) -2147483647L - 1)
#define PTRDIFF_MAX ((int32_t) 2147483647L)

#ifdef __STDC_LIMIT_MACROS
#define SIG_ATOMIC_MIN INT32_MIN
#define SIG_ATOMIC_MAX INT32_MAX
#endif

#define SIZE_MAX UINT32_MAX

#ifdef __STDC_CONSTANT_MACROS
#define WCHAR_MIN 0
#define WCHAR_MAX UINT16_MAX
#define WINT_MIN 0
#define WINT_MAX UINT16_MAX
#endif



/* 7.18.4.1 Macros for minimum-width integer constants */

#define INT8_C(x) ((int8_t) x)
#define INT16_C(x) ((int16_t) x)
#define INT32_C(x) ((int32_t) x)
#define INT64_C(x) ((int64_t) x)

#define UINT8_C(x) ((uint8_t) x)
#define UINT16_C(x) ((uint16_t) x)
#define UINT32_C(x) ((uint32_t) x)
#define UINT64_C(x) ((uint64_t) x)



/* 7.18.4.2 Macros for greatest-width integer constants */

#define INTMAX_C(x) ((intmax_t) x)
#define UINTMAX_C(x) ((uintmax_t) x)

#endif /* __STDINT_H */
