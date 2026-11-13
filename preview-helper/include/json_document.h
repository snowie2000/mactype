#pragma once

#include <cstddef>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace mactype {

inline constexpr std::size_t kJsonDocumentMaxLength = 64U * 1024U;
inline constexpr std::size_t kJsonDocumentMaxDepth = 32U;

namespace json_detail {
struct Storage;
struct Value;
}

class JsonObject {
 public:
  std::optional<std::string> json_string(std::string_view key) const;
  std::optional<double> json_number(std::string_view key) const;
  std::optional<bool> json_bool(std::string_view key) const;
  bool contains(std::string_view key) const;

 private:
  friend class JsonDocument;
  JsonObject(std::shared_ptr<const json_detail::Storage> storage,
             const json_detail::Value* value);

  std::shared_ptr<const json_detail::Storage> storage_;
  const json_detail::Value* value_{};
};

class JsonDocument {
 public:
  static std::optional<JsonDocument> parse(std::string json, std::string& error);

  std::optional<std::string> json_string(std::string_view key) const;
  std::optional<double> json_number(std::string_view key) const;
  std::optional<bool> json_bool(std::string_view key) const;
  std::optional<std::string> root_string(std::string_view key) const;
  std::optional<double> root_number(std::string_view key) const;
  std::optional<bool> root_bool(std::string_view key) const;
  std::optional<std::vector<double>> root_number_array(std::string_view key) const;
  std::optional<JsonObject> object(std::string_view key) const;
  bool contains_root(std::string_view key) const;

 private:
  explicit JsonDocument(std::shared_ptr<const json_detail::Storage> storage);

  std::shared_ptr<const json_detail::Storage> storage_;
};

}  // namespace mactype
