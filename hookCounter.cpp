#include "hookCounter.h"
#include <atomic>

std::atomic<int> HCounter::ref(0);