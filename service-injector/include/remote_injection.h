#pragma once

#include "broker_request.h"
#include "result.h"

#include <windows.h>

#include <filesystem>

namespace mactype::injector {

[[nodiscard]] Result inject_module(HANDLE process, const BrokerRequest& request,
                                   const std::filesystem::path& module_path) noexcept;

}  // namespace mactype::injector
