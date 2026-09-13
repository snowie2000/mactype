#pragma once

// MacType's legacy resource script only needs Windows resource definitions
// from afxres.h; the program itself does not use MFC. Keeping this compatibility
// header beside gdidll.rc avoids requiring the optional MFC build workload.
#include <windows.h>
#include <winres.h>
