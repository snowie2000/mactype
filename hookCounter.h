#pragma once
#include <atomic>
#include <thread>
#include <chrono>

class HCounter {
	static std::atomic<int> ref;

public:
	HCounter() {
		++this->ref;
	}

	~HCounter() {
		--this->ref;
	}

	static bool wait(int nCount) {
		while (--nCount && HCounter::ref.load()) {
			std::this_thread::sleep_for(std::chrono::milliseconds(1));
		}
		return !HCounter::ref;
	}
};
