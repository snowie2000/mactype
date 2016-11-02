﻿//
// DelayHlp.cpp
//
//  Copyright (c) Microsoft Corporation.  All rights reserved.
//
//  Implement the delay load helper routines.
//

// Build instructions
// cl -c -O1 -Z7 -Zl -W3 delayhlp.cpp
//
// For ISOLATION_AWARE_ENABLED calls to LoadLibrary(), you will need to add
// a definition for ISOLATION_AWARE_ENABLED to the command line above, eg:
// cl -c -O1 -Z7 -Zl -W3 -DISOLATION_AWARE_ENABLED=1 delayhlp.cpp
//
//
// Then, you can either link directly with this new object file, or replace the one in
// delayimp.lib with your new one, eg:
// lib /out:delayimp.lib delayhlp.obj
//


#define WIN32_LEAN_AND_MEAN
#define STRICT
#include <windows.h>

#include "DelayImp.h"

//
// Local copies of strlen, memcmp, and memcpy to make sure we do not need the CRT
//

static inline size_t
__strlen(const char * sz) {
    const char *szEnd = sz;

    while( *szEnd++ ) {
        ;
        }

    return szEnd - sz - 1;
    }

static inline int
__memcmp(const void * pv1, const void * pv2, size_t cb) {
    if (!cb) {
        return 0;
        }

    while ( --cb && *(char *)pv1 == *(char *)pv2 ) {
        pv1 = (char *)pv1 + 1;
        pv2 = (char *)pv2 + 1;
        }

    return  *((unsigned char *)pv1) - *((unsigned char *)pv2);
    }

static inline void *
__memcpy(void * pvDst, const void * pvSrc, size_t cb) {

    void * pvRet = pvDst;

    //
    // copy from lower addresses to higher addresses
    //
    while (cb--) {
        *(char *)pvDst = *(char *)pvSrc;
        pvDst = (char *)pvDst + 1;
        pvSrc = (char *)pvSrc + 1;
        }

    return pvRet;
    }


// utility function for calculating the index of the current import
// for all the tables (INT, BIAT, UIAT, and IAT).
inline unsigned
IndexFromPImgThunkData(PCImgThunkData pitdCur, PCImgThunkData pitdBase) {
    return (unsigned) (pitdCur - pitdBase);
    }

// C++ template utility function for converting RVAs to pointers
//
#if defined(_M_IA64)
#pragma section(".base", long, read)
extern "C"
__declspec(allocate(".base"))
const IMAGE_DOS_HEADER __ImageBase;
#else
extern "C"
const IMAGE_DOS_HEADER __ImageBase;
#endif

template <class X>
X PFromRva(RVA rva) {
    return X(PBYTE(&__ImageBase) + rva);
    }

// structure definitions for the list of unload records
typedef struct UnloadInfo * PUnloadInfo;
typedef struct UnloadInfo {
    PUnloadInfo     puiNext;
    PCImgDelayDescr pidd;
    } UnloadInfo;

// utility function for calculating the count of imports given the base
// of the IAT.  NB: this only works on a valid IAT!
inline unsigned
CountOfImports(PCImgThunkData pitdBase) {
    unsigned        cRet = 0;
    PCImgThunkData  pitd = pitdBase;
    while (pitd->u1.Function) {
        pitd++;
        cRet++;
        }
    return cRet;
    }

extern "C"
PUnloadInfo __puiHead = 0;

struct ULI : public UnloadInfo {
    ULI(PCImgDelayDescr pidd_) {
        pidd = pidd_;
        Link();
        }

    ~ULI() {
        Unlink();
        }

    void *
    operator new(size_t cb) {
        return ::LocalAlloc(LPTR, cb);
        }

    void
    operator delete(void * pv) {
        ::LocalFree(pv);
        }

    void
    Unlink() {
        PUnloadInfo *   ppui = &__puiHead;

        while (*ppui && *ppui != this) {
            ppui = &((*ppui)->puiNext);
            }
        if (*ppui == this) {
            *ppui = puiNext;
            }
        }

    void
    Link() {
        puiNext = __puiHead;
        __puiHead = this;
        }
    };

// For our own internal use, we convert to the old
// format for convenience.
//
struct InternalImgDelayDescr {
    DWORD           grAttrs;        // attributes
    LPCSTR          szName;         // pointer to dll name
    HMODULE *       phmod;          // address of module handle
    PImgThunkData   pIAT;           // address of the IAT
    PCImgThunkData  pINT;           // address of the INT
    PCImgThunkData  pBoundIAT;      // address of the optional bound IAT
    PCImgThunkData  pUnloadIAT;     // address of optional copy of original IAT
    DWORD           dwTimeStamp;    // 0 if not bound,
                                    // O.W. date/time stamp of DLL bound to (Old BIND)
    };

typedef InternalImgDelayDescr *         PIIDD;
typedef const InternalImgDelayDescr *   PCIIDD;

static inline
PIMAGE_NT_HEADERS WINAPI
PinhFromImageBase(HMODULE hmod) {
    return PIMAGE_NT_HEADERS(PBYTE(hmod) + PIMAGE_DOS_HEADER(hmod)->e_lfanew);
    }

static inline
void WINAPI
OverlayIAT(PImgThunkData pitdDst, PCImgThunkData pitdSrc) {
    __memcpy(pitdDst, pitdSrc, CountOfImports(pitdDst) * sizeof IMAGE_THUNK_DATA);
    }

static inline
DWORD WINAPI
TimeStampOfImage(PIMAGE_NT_HEADERS pinh) {
    return pinh->FileHeader.TimeDateStamp;
    }

static inline
bool WINAPI
FLoadedAtPreferredAddress(PIMAGE_NT_HEADERS pinh, HMODULE hmod) {
    return UINT_PTR(hmod) == pinh->OptionalHeader.ImageBase;
    }


// Do the InterlockedExchange magic
//
#ifdef  _M_IX86

#undef  InterlockedExchangePointer
#define InterlockedExchangePointer(Target, Value) \
    (PVOID)(LONG_PTR)InterlockedExchange((PLONG)(Target), (LONG)(LONG_PTR)(Value))

#if (_MSC_VER >= 1300)
typedef __w64 unsigned long *PULONG_PTR;
#else
typedef unsigned long *PULONG_PTR;
#endif

#endif

extern "C"
FARPROC WINAPI
__delayLoadHelper2(
    PCImgDelayDescr     pidd,
    FARPROC *           ppfnIATEntry
    ) {

    // Set up some data we use for the hook procs but also useful for
    // our own use
    //
    InternalImgDelayDescr   idd = {
        pidd->grAttrs,
        PFromRva<LPCSTR>(pidd->rvaDLLName),
        PFromRva<HMODULE*>(pidd->rvaHmod),
        PFromRva<PImgThunkData>(pidd->rvaIAT),
        PFromRva<PCImgThunkData>(pidd->rvaINT),
        PFromRva<PCImgThunkData>(pidd->rvaBoundIAT),
        PFromRva<PCImgThunkData>(pidd->rvaUnloadIAT),
        pidd->dwTimeStamp
        };

    DelayLoadInfo   dli = {
        sizeof DelayLoadInfo,
        pidd,
        ppfnIATEntry,
        idd.szName,
            { 0 },
        0,
        0,
        0
        };

    if (0 == (idd.grAttrs & dlattrRva)) {
        PDelayLoadInfo  rgpdli[1] = { &dli };

        RaiseException(
            VcppException(ERROR_SEVERITY_ERROR, ERROR_INVALID_PARAMETER),
            0,
            1,
            PULONG_PTR(rgpdli)
            );
        return 0;
        }

    HMODULE hmod = *idd.phmod;

    // Calculate the index for the IAT entry in the import address table
    // N.B. The INT entries are ordered the same as the IAT entries so
    // the calculation can be done on the IAT side.
    //
    const unsigned  iIAT = IndexFromPImgThunkData(PCImgThunkData(ppfnIATEntry), idd.pIAT);
    const unsigned  iINT = iIAT;

    PCImgThunkData  pitd = &(idd.pINT[iINT]);

    dli.dlp.fImportByName = !IMAGE_SNAP_BY_ORDINAL(pitd->u1.Ordinal);

    if (dli.dlp.fImportByName) {
        dli.dlp.szProcName = LPCSTR(PFromRva<PIMAGE_IMPORT_BY_NAME>(RVA(UINT_PTR(pitd->u1.AddressOfData)))->Name);
        }
    else {
        dli.dlp.dwOrdinal = DWORD(IMAGE_ORDINAL(pitd->u1.Ordinal));
        }

    // Call the initial hook.  If it exists and returns a function pointer,
    // abort the rest of the processing and just return it for the call.
    //
    FARPROC pfnRet = NULL;

    if (__pfnDliNotifyHook2) {
        pfnRet = ((*__pfnDliNotifyHook2)(dliStartProcessing, &dli));

        if (pfnRet != NULL) {
            goto HookBypass;
            }
        }

    // Check to see if we need to try to load the library.
    //
    if (hmod == 0) {
        if (__pfnDliNotifyHook2) {
            hmod = HMODULE(((*__pfnDliNotifyHook2)(dliNotePreLoadLibrary, &dli)));
            }
        if (hmod == 0) {
            hmod = ::LoadLibraryA(dli.szDll);
            }
        if (hmod == 0) {
            dli.dwLastError = ::GetLastError();
            if (__pfnDliFailureHook2) {
                // when the hook is called on LoadLibrary failure, it will
                // return 0 for failure and an hmod for the lib if it fixed
                // the problem.
                //
                hmod = HMODULE((*__pfnDliFailureHook2)(dliFailLoadLib, &dli));
                }

            if (hmod == 0) {
                PDelayLoadInfo  rgpdli[1] = { &dli };

                RaiseException(
                    VcppException(ERROR_SEVERITY_ERROR, ERROR_MOD_NOT_FOUND),
                    0,
                    1,
                    PULONG_PTR(rgpdli)
                    );
                
                // If we get to here, we blindly assume that the handler of the exception
                // has magically fixed everything up and left the function pointer in 
                // dli.pfnCur.
                //
                return dli.pfnCur;
                }
            }

        // Store the library handle.  If it is already there, we infer
        // that another thread got there first, and we need to do a
        // FreeLibrary() to reduce the refcount
        //
        HMODULE hmodT = HMODULE(InterlockedExchangePointer((PVOID *) idd.phmod, PVOID(hmod)));
        if (hmodT != hmod) {
            // add lib to unload list if we have unload data
            if (pidd->rvaUnloadIAT) {
// suppress prefast warning 6014, the object is saved in a link list in the constructor of ULI
#pragma warning(suppress:6014)
                new ULI(pidd);
                }
            }
        else {
            ::FreeLibrary(hmod);
            }
        
        }

    // Go for the procedure now.
    //
    dli.hmodCur = hmod;
    if (__pfnDliNotifyHook2) {
        pfnRet = (*__pfnDliNotifyHook2)(dliNotePreGetProcAddress, &dli);
        }
    if (pfnRet == 0) {
        if (pidd->rvaBoundIAT && pidd->dwTimeStamp) {
            // bound imports exist...check the timestamp from the target image
            //
            PIMAGE_NT_HEADERS   pinh(PinhFromImageBase(hmod));

            if (pinh->Signature == IMAGE_NT_SIGNATURE &&
                TimeStampOfImage(pinh) == idd.dwTimeStamp &&
                FLoadedAtPreferredAddress(pinh, hmod)) {

                // Everything is good to go, if we have a decent address
                // in the bound IAT!
                //
                pfnRet = FARPROC(UINT_PTR(idd.pBoundIAT[iIAT].u1.Function));
                if (pfnRet != 0) {
                    goto SetEntryHookBypass;
                    }
                }
            }

        pfnRet = ::GetProcAddress(hmod, dli.dlp.szProcName);
        }

    if (pfnRet == 0) {
        dli.dwLastError = ::GetLastError();
        if (__pfnDliFailureHook2) {
            // when the hook is called on GetProcAddress failure, it will
            // return 0 on failure and a valid proc address on success
            //
            pfnRet = (*__pfnDliFailureHook2)(dliFailGetProc, &dli);
            }
        if (pfnRet == 0) {
            PDelayLoadInfo  rgpdli[1] = { &dli };

            RaiseException(
                VcppException(ERROR_SEVERITY_ERROR, ERROR_PROC_NOT_FOUND),
                0,
                1,
                PULONG_PTR(rgpdli)
                );

            // If we get to here, we blindly assume that the handler of the exception
            // has magically fixed everything up and left the function pointer in 
            // dli.pfnCur.
            //
            pfnRet = dli.pfnCur;
            }
        }

SetEntryHookBypass:
    *ppfnIATEntry = pfnRet;

HookBypass:
    if (__pfnDliNotifyHook2) {
        dli.dwLastError = 0;
        dli.hmodCur = hmod;
        dli.pfnCur = pfnRet;
        (*__pfnDliNotifyHook2)(dliNoteEndProcessing, &dli);
        }
    return pfnRet;
    }

extern "C"
BOOL WINAPI
__FUnloadDelayLoadedDLL2(LPCSTR szDll) {
    
    BOOL        fRet = FALSE;
    PUnloadInfo pui = __puiHead;
    
    for (pui = __puiHead; pui; pui = pui->puiNext) {
        LPCSTR  szName = PFromRva<LPCSTR>(pui->pidd->rvaDLLName);
        size_t  cbName = __strlen(szName);

        // Intentionally case sensitive to avoid complication of using the CRT
        // for those that don't use the CRT...the user can replace this with
        // a variant of a case insenstive comparison routine
        //
        if (cbName == __strlen(szDll) && stricmp(szDll, szName) == 0) {
            break;
            }
        }

    if (pui && pui->pidd->rvaUnloadIAT) {
        PCImgDelayDescr     pidd = pui->pidd;
        HMODULE *           phmod = PFromRva<HMODULE*>(pidd->rvaHmod);
        HMODULE             hmod = *phmod;

        OverlayIAT(
            PFromRva<PImgThunkData>(pidd->rvaIAT),
            PFromRva<PCImgThunkData>(pidd->rvaUnloadIAT)
            );
        ::FreeLibrary(hmod);
        *phmod = NULL;
        
        delete reinterpret_cast<ULI*> (pui);

        fRet = TRUE;
        }

    return fRet;
    }

extern "C"
HRESULT WINAPI
__HrLoadAllImportsForDll(LPCSTR szDll) {
    HRESULT             hrRet = HRESULT_FROM_WIN32(ERROR_MOD_NOT_FOUND);
    PIMAGE_NT_HEADERS   pinh = PinhFromImageBase(HMODULE(&__ImageBase));

    // Scan the Delay load IAT/INT for the dll in question
    //
    if (pinh->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT].Size) {
        PCImgDelayDescr     pidd;

        pidd = PFromRva<PCImgDelayDescr>(
            pinh->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT].VirtualAddress
            );

        // Check all of the dlls listed up to the NULL one.
        //
        while (pidd->rvaDLLName) {
            // Check to see if it is the DLL we want to load.
            // Intentionally case sensitive to avoid complication of using the CRT
            // for those that don't use the CRT...the user can replace this with
            // a variant of a case insenstive comparison routine.
            //
            LPCSTR  szDllCur = PFromRva<LPCSTR>(pidd->rvaDLLName);
            size_t  cchDllCur = __strlen(szDllCur);
            if (cchDllCur == __strlen(szDll) && __memcmp(szDll, szDllCur, cchDllCur) == 0) {
                // We found it, so break out with pidd and szDllCur set appropriately
                //
                break;
                }
            
            // Advance to the next delay import descriptor
            //
            pidd++;
            }
        
        if (pidd->rvaDLLName) {
            // Found a matching DLL name, now process it.
            //
            // Set up the internal structure
            //
            FARPROC *   ppfnIATEntry = PFromRva<FARPROC*>(pidd->rvaIAT);
            size_t      cpfnIATEntries = CountOfImports(PCImgThunkData(ppfnIATEntry));
            FARPROC *   ppfnIATEntryMax = ppfnIATEntry + cpfnIATEntries;

            for (;ppfnIATEntry < ppfnIATEntryMax; ppfnIATEntry++) {
                __delayLoadHelper2(pidd, ppfnIATEntry);
                }

            // Done, indicate some semblance of success
            //
            hrRet = S_OK;
            }
        }
    return hrRet;
}
