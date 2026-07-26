import { auth } from "@/app/(auth)/auth";
import { getChatById, getMessagesByChatId } from "@/lib/db/queries";
import { convertToUIMessages } from "@/lib/utils";

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url);
  const chatId = searchParams.get("chatId");

  if (!chatId) {
    return Response.json({ error: "chatId required" }, { status: 400 });
  }

  const session = await auth();
  // Read the parent first. A new-chat request persists the parent before it
  // returns its stream response, but the client can request messages while
  // that response is starting. Reading children concurrently turns that
  // expected transient state into a database not-found error.
  const chat = await getChatById({ id: chatId, userId: session?.user?.id });

  if (!chat) {
    return Response.json({
      isReadonly: false,
      messages: [],
      userId: null,
      visibility: "private",
    });
  }

  const messages = session?.user
    ? await getMessagesByChatId({ id: chatId, userId: session.user.id })
    : [];

  if (
    chat.visibility === "private" &&
    (!session?.user || session.user.id !== chat.userId)
  ) {
    return Response.json({ error: "forbidden" }, { status: 403 });
  }

  const isReadonly = !session?.user || session.user.id !== chat.userId;

  return Response.json({
    isReadonly,
    messages: convertToUIMessages(messages),
    userId: chat.userId,
    visibility: chat.visibility,
  });
}
