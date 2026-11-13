#pragma once

namespace mactype::injector {

[[nodiscard]] bool protection_state_allows_injection(bool query_succeeded,
                                                     bool is_unprotected) noexcept;

}  // namespace mactype::injector
