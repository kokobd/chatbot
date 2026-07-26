import { connection } from "next/server";
import { auth } from "@/app/(auth)/auth";
import { getMessageCountByUserId } from "@/lib/db/queries";

const READINESS_TIMEOUT_MS = 15_000;

export async function GET() {
  await connection();

  // This endpoint is deliberately unavailable outside the local real-resource
  // test process. It never returns configuration values or credentials.
  if (
    process.env.E2E_REAL_TESTS !== "1" ||
    process.env.IAP_AUTH_PROVIDER !== "test"
  ) {
    return new Response(null, { status: 404 });
  }

  try {
    const readiness = Promise.race([
      (async () => {
        const session = await auth();
        if (!session?.user) {
          throw new Error("test identity was not authenticated");
        }

        await getMessageCountByUserId({
          differenceInHours: 1,
          id: session.user.id,
        });
      })(),
      new Promise<never>((_, reject) => {
        setTimeout(
          () => reject(new Error("readiness timed out")),
          READINESS_TIMEOUT_MS
        );
      }),
    ]);

    await readiness;
    return Response.json({ ready: true });
  } catch (error) {
    console.error(
      "Real e2e readiness failed",
      error instanceof Error ? error.message : String(error)
    );
    return Response.json({ ready: false }, { status: 503 });
  }
}
