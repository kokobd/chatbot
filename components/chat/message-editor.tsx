"use client";

import type { UseChatHelpers } from "@ai-sdk/react";
import { deleteTrailingMessages } from "@/app/(chat)/actions";
import type { ChatMessage } from "@/lib/types";

export async function submitEditedMessage({
  chatId,
  message,
  text,
  setMessages,
  regenerate,
}: {
  chatId: string;
  message: ChatMessage;
  text: string;
  setMessages: UseChatHelpers<ChatMessage>["setMessages"];
  regenerate: UseChatHelpers<ChatMessage>["regenerate"];
}) {
  await deleteTrailingMessages({ chatId, id: message.id });

  setMessages((messages) => {
    const index = messages.findIndex((m) => m.id === message.id);
    if (index === -1) {
      return messages;
    }

    return [
      ...messages.slice(0, index),
      { ...message, parts: [{ text, type: "text" as const }] },
    ];
  });

  regenerate();
}
