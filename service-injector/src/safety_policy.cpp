#include "safety_policy.h"

namespace mactype::injector {

bool protection_state_allows_injection(const bool query_succeeded,
                                       const bool is_unprotected) noexcept {
    return query_succeeded && is_unprotected;
}

}  // namespace mactype::injector
