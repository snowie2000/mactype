#include <windows.h>

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    static_cast<void>(reserved);
    if (reason == DLL_PROCESS_ATTACH) {
        DisableThreadLibraryCalls(instance);
#ifdef MACTYPE_MARKER_DELAY_MS
        Sleep(MACTYPE_MARKER_DELAY_MS);
#endif
    }
    return TRUE;
}
