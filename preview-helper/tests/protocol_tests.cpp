#include "protocol.h"

#include <cassert>
#include <sstream>

int main() {
  mtpc::Frame original;
  original.kind = mtpc::MessageKind::render_preview;
  original.request_id = 41;
  original.json = R"({"test":true})";
  original.binary = {1, 2, 3, 4};

  std::stringstream stream(std::ios::in | std::ios::out | std::ios::binary);
  assert(mtpc::write_frame(stream, original));
  stream.seekg(0);

  mtpc::Frame decoded;
  std::string error;
  assert(mtpc::read_frame(stream, decoded, error));
  assert(decoded.kind == original.kind);
  assert(decoded.request_id == original.request_id);
  assert(decoded.json == original.json);
  assert(decoded.binary == original.binary);
  assert(mtpc::placeholder_png().size() > 32U);
  return 0;
}
