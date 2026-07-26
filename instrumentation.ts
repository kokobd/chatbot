import { OpenTelemetry } from "@ai-sdk/otel";
import { registerTelemetry } from "ai";

export function register() {
  registerTelemetry(new OpenTelemetry());
}
