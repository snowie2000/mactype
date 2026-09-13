#include "broker_request.h"
#include "injector.h"
#include "result.h"

#include <iostream>
#include <string_view>
#include <vector>

int wmain(const int count, wchar_t** values) {
    std::vector<std::wstring_view> arguments;
    arguments.reserve(static_cast<std::size_t>(count));
    for (int index = 0; index < count; ++index) {
        arguments.emplace_back(values[index]);
    }

    const auto request = mactype::injector::parse_broker_request(arguments);
    if (!request) {
#ifdef MACTYPE_FIXED_MODULE_BASENAME
        constexpr std::string_view module = MACTYPE_FIXED_MODULE_UTF8;
#elif defined(_WIN64)
        constexpr std::string_view module = "MacType64.dll";
#else
        constexpr std::string_view module = "MacType.dll";
#endif
        const mactype::injector::BrokerRequest empty{};
        const auto result = mactype::injector::make_result(
            empty, mactype::injector::ResultStatus::rejected, "invalid-request", module);
        std::cout << mactype::injector::to_json(result) << '\n';
        return mactype::injector::exit_code(result.status);
    }

    const auto result = mactype::injector::inject_fixed_adjacent_module(*request);
    std::cout << mactype::injector::to_json(result) << '\n';
    return mactype::injector::exit_code(result.status);
}
