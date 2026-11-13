#include "json_document.h"

#include <cctype>
#include <cmath>
#include <cstdlib>
#include <utility>

namespace mactype {
namespace json_detail {

enum class Type { null_value, string, number, boolean, object, array };

struct Value {
  Type type{Type::null_value};
  std::string string_value;
  double number_value{};
  bool bool_value{};
  std::vector<std::pair<std::string, Value>> object_value;
  std::vector<Value> array_value;
};

struct Storage {
  std::string source;
  Value root;
};

}  // namespace json_detail
namespace {

using json_detail::Type;
using json_detail::Value;

class Parser {
 public:
  Parser(const std::string& input, std::string& error) : input_(input), error_(error) {}

  bool parse(Value& value) {
    skip_space();
    if (!parse_value(value, 1U)) return false;
    skip_space();
    if (position_ != input_.size()) return fail("unexpected data after JSON value");
    if (value.type != Type::object) return fail("JSON document root must be an object");
    return true;
  }

 private:
  bool parse_value(Value& value, std::size_t depth) {
    if (depth > kJsonDocumentMaxDepth) return fail("JSON nesting exceeds the supported depth");
    if (position_ >= input_.size()) return fail("unexpected end of JSON input");
    switch (input_[position_]) {
      case '{': return parse_object(value, depth);
      case '[': return parse_array(value, depth);
      case '"':
        value.type = Type::string;
        return parse_string(value.string_value);
      case 't': return parse_literal("true", Type::boolean, value, true);
      case 'f': return parse_literal("false", Type::boolean, value, false);
      case 'n': return parse_literal("null", Type::null_value, value, false);
      default: return parse_number(value);
    }
  }

  bool parse_object(Value& value, std::size_t depth) {
    value.type = Type::object;
    ++position_;
    skip_space();
    if (consume('}')) return true;
    for (;;) {
      if (position_ >= input_.size() || input_[position_] != '"') {
        return fail("JSON object key must be a string");
      }
      std::string key;
      if (!parse_string(key)) return false;
      skip_space();
      if (!consume(':')) return fail("JSON object key is missing a colon");
      skip_space();
      Value child;
      if (!parse_value(child, depth + 1U)) return false;
      value.object_value.emplace_back(std::move(key), std::move(child));
      skip_space();
      if (consume('}')) return true;
      if (!consume(',')) return fail("JSON object entries must be separated by commas");
      skip_space();
    }
  }

  bool parse_array(Value& value, std::size_t depth) {
    value.type = Type::array;
    ++position_;
    skip_space();
    if (consume(']')) return true;
    for (;;) {
      Value child;
      if (!parse_value(child, depth + 1U)) return false;
      value.array_value.push_back(std::move(child));
      skip_space();
      if (consume(']')) return true;
      if (!consume(',')) return fail("JSON array entries must be separated by commas");
      skip_space();
    }
  }

  bool parse_string(std::string& output) {
    ++position_;
    while (position_ < input_.size()) {
      const unsigned char character = static_cast<unsigned char>(input_[position_++]);
      if (character == '"') return true;
      if (character < 0x20U) return fail("JSON string contains an unescaped control character");
      if (character != '\\') {
        output.push_back(static_cast<char>(character));
        continue;
      }
      if (position_ >= input_.size()) return fail("JSON string ends after an escape marker");
      switch (input_[position_++]) {
        case '"': output.push_back('"'); break;
        case '\\': output.push_back('\\'); break;
        case '/': output.push_back('/'); break;
        case 'b': output.push_back('\b'); break;
        case 'f': output.push_back('\f'); break;
        case 'n': output.push_back('\n'); break;
        case 'r': output.push_back('\r'); break;
        case 't': output.push_back('\t'); break;
        default: return fail("JSON string contains an unsupported escape");
      }
    }
    return fail("unterminated JSON string");
  }

  bool parse_number(Value& value) {
    const std::size_t start = position_;
    if (consume('-') && position_ >= input_.size()) return fail("incomplete JSON number");
    if (consume('0')) {
      if (position_ < input_.size() && std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        return fail("JSON number has a leading zero");
      }
    } else {
      if (position_ >= input_.size() ||
          !std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        return fail("invalid JSON value");
      }
      while (position_ < input_.size() &&
             std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        ++position_;
      }
    }
    if (consume('.')) {
      if (position_ >= input_.size() ||
          !std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        return fail("JSON fraction is missing digits");
      }
      while (position_ < input_.size() &&
             std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        ++position_;
      }
    }
    if (position_ < input_.size() &&
        (input_[position_] == 'e' || input_[position_] == 'E')) {
      ++position_;
      if (position_ < input_.size() &&
          (input_[position_] == '+' || input_[position_] == '-')) {
        ++position_;
      }
      if (position_ >= input_.size() ||
          !std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        return fail("JSON exponent is missing digits");
      }
      while (position_ < input_.size() &&
             std::isdigit(static_cast<unsigned char>(input_[position_]))) {
        ++position_;
      }
    }
    char* ending{};
    const double parsed = std::strtod(input_.c_str() + start, &ending);
    if (!std::isfinite(parsed) ||
        ending != input_.c_str() + static_cast<std::ptrdiff_t>(position_)) {
      return fail("JSON number is outside the supported range");
    }
    value.type = Type::number;
    value.number_value = parsed;
    return true;
  }

  bool parse_literal(std::string_view literal, Type type, Value& value, bool boolean) {
    if (input_.compare(position_, literal.size(), literal) != 0) return fail("invalid JSON value");
    position_ += literal.size();
    value.type = type;
    value.bool_value = boolean;
    return true;
  }

  void skip_space() {
    while (position_ < input_.size() &&
           std::isspace(static_cast<unsigned char>(input_[position_])) != 0) {
      ++position_;
    }
  }

  bool consume(char character) {
    if (position_ >= input_.size() || input_[position_] != character) return false;
    ++position_;
    return true;
  }

  bool fail(const char* message) {
    error_ = message;
    return false;
  }

  const std::string& input_;
  std::string& error_;
  std::size_t position_{};
};

const Value* object_value(const Value& object, std::string_view key) {
  if (object.type != Type::object) return nullptr;
  for (const auto& [candidate, value] : object.object_value) {
    if (candidate == key) return &value;
  }
  return nullptr;
}

const Value* recursive_value(const Value& value, std::string_view key) {
  if (value.type == Type::object) {
    for (const auto& [candidate, child] : value.object_value) {
      if (candidate == key) return &child;
      if (const Value* nested = recursive_value(child, key)) return nested;
    }
  } else if (value.type == Type::array) {
    for (const auto& child : value.array_value) {
      if (const Value* nested = recursive_value(child, key)) return nested;
    }
  }
  return nullptr;
}

std::optional<std::string> string_value(const Value* value) {
  if (!value || value->type != Type::string) return std::nullopt;
  return value->string_value;
}

std::optional<double> number_value(const Value* value) {
  if (!value || value->type != Type::number) return std::nullopt;
  return value->number_value;
}

std::optional<bool> bool_value(const Value* value) {
  if (!value || value->type != Type::boolean) return std::nullopt;
  return value->bool_value;
}

}  // namespace

JsonObject::JsonObject(std::shared_ptr<const json_detail::Storage> storage,
                       const json_detail::Value* value)
    : storage_(std::move(storage)), value_(value) {}

std::optional<std::string> JsonObject::json_string(std::string_view key) const {
  return string_value(object_value(*value_, key));
}

std::optional<double> JsonObject::json_number(std::string_view key) const {
  return number_value(object_value(*value_, key));
}

std::optional<bool> JsonObject::json_bool(std::string_view key) const {
  return bool_value(object_value(*value_, key));
}

bool JsonObject::contains(std::string_view key) const {
  return object_value(*value_, key) != nullptr;
}

JsonDocument::JsonDocument(std::shared_ptr<const json_detail::Storage> storage)
    : storage_(std::move(storage)) {}

std::optional<JsonDocument> JsonDocument::parse(std::string json, std::string& error) {
  error.clear();
  if (json.size() > kJsonDocumentMaxLength) {
    error = "JSON document exceeds the supported length";
    return std::nullopt;
  }
  auto storage = std::make_shared<json_detail::Storage>();
  storage->source = std::move(json);
  Parser parser(storage->source, error);
  if (!parser.parse(storage->root)) return std::nullopt;
  return JsonDocument(std::move(storage));
}

std::optional<std::string> JsonDocument::json_string(std::string_view key) const {
  return string_value(recursive_value(storage_->root, key));
}

std::optional<double> JsonDocument::json_number(std::string_view key) const {
  return number_value(recursive_value(storage_->root, key));
}

std::optional<bool> JsonDocument::json_bool(std::string_view key) const {
  return bool_value(recursive_value(storage_->root, key));
}

std::optional<std::string> JsonDocument::root_string(std::string_view key) const {
  return string_value(object_value(storage_->root, key));
}

std::optional<double> JsonDocument::root_number(std::string_view key) const {
  return number_value(object_value(storage_->root, key));
}

std::optional<bool> JsonDocument::root_bool(std::string_view key) const {
  return bool_value(object_value(storage_->root, key));
}

std::optional<std::vector<double>> JsonDocument::root_number_array(std::string_view key) const {
  const Value* value = object_value(storage_->root, key);
  if (!value || value->type != Type::array) return std::nullopt;
  std::vector<double> result;
  result.reserve(value->array_value.size());
  for (const auto& item : value->array_value) {
    if (item.type != Type::number) return std::nullopt;
    result.push_back(item.number_value);
  }
  return result;
}

std::optional<JsonObject> JsonDocument::object(std::string_view key) const {
  const Value* value = object_value(storage_->root, key);
  if (!value || value->type != Type::object) return std::nullopt;
  return JsonObject(storage_, value);
}

bool JsonDocument::contains_root(std::string_view key) const {
  return object_value(storage_->root, key) != nullptr;
}

}  // namespace mactype
