#pragma once

#include "broker_request.h"
#include "result.h"

namespace mactype::injector {

[[nodiscard]] Result inject_fixed_adjacent_module(const BrokerRequest& request) noexcept;

}  // namespace mactype::injector
