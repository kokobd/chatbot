"use server";

import { generateText, type UIMessage } from "ai";
import { cookies } from "next/headers";
import { auth } from "@/app/(auth)/auth";
import type { VisibilityType } from "@/components/chat/visibility-selector";
import { titlePrompt } from "@/lib/ai/prompts";
import { getTitleModel } from "@/lib/ai/providers";
import { createFallbackTitle, normalizeGeneratedTitle } from "@/lib/ai/title";
import {
  deleteMessagesByChatIdAfterTimestamp,
  getChatById,
  getMessageById,
  updateChatVisibilityById,
} from "@/lib/db/queries";
import { getTextFromMessage } from "@/lib/utils";

export async function saveChatModelAsCookie(model: string) {
  const cookieStore = await cookies();
  cookieStore.set("chat-model", model);
}

export async function generateTitleFromUserMessage({
  message,
}: {
  message: UIMessage;
}) {
  const messageText = getTextFromMessage(message);
  const fallback = createFallbackTitle(messageText);

  try {
    const { text } = await generateText({
      instructions: titlePrompt,
      maxRetries: 0,
      model: getTitleModel(),
      prompt: messageText,
      timeout: 10_000,
    });

    return normalizeGeneratedTitle(text, fallback);
  } catch {
    return fallback;
  }
}

export async function deleteTrailingMessages({
  id,
  chatId,
}: {
  id: string;
  chatId?: string;
}) {
  const session = await auth();
  if (!session?.user?.id) {
    throw new Error("Unauthorized");
  }

  if (!chatId) {
    throw new Error("Message chat is unavailable");
  }
  const [message] = await getMessageById({
    chatId,
    id,
    userId: session.user.id,
  });
  if (!message) {
    throw new Error("Message not found");
  }

  const chat = await getChatById({
    id: message.chatId,
    userId: session.user.id,
  });
  if (!chat || chat.userId !== session.user.id) {
    throw new Error("Unauthorized");
  }

  await deleteMessagesByChatIdAfterTimestamp({
    chatId: message.chatId,
    timestamp: new Date(message.createdAt),
    userId: session.user.id,
  });
}

export async function updateChatVisibility({
  chatId,
  visibility,
}: {
  chatId: string;
  visibility: VisibilityType;
}) {
  const session = await auth();
  if (!session?.user?.id) {
    throw new Error("Unauthorized");
  }

  const chat = await getChatById({ id: chatId, userId: session.user.id });
  if (!chat || chat.userId !== session.user.id) {
    throw new Error("Unauthorized");
  }

  await updateChatVisibilityById({
    chatId,
    userId: session.user.id,
    visibility,
  });
}
