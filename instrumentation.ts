import { OpenTelemetry } from "@ai-sdk/otel";
import { registerTelemetry } from "ai";

export async function register() {
  if (process.env.NEXT_RUNTIME !== "edge") {
    const { initializeNativeService } = await import("./lib/native");
    await initializeNativeService();
  }

  registerTelemetry(new OpenTelemetry());
}
