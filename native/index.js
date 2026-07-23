const path = require("node:path");

const packageDirectory = path.dirname(require.resolve("./package.json"));

process.dlopen(module, path.join(packageDirectory, "chatbot_native.node"));
