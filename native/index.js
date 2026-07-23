const path = require("node:path");

process.dlopen(
  module,
  path.join(process.cwd(), "native", "chatbot_native.node")
);
