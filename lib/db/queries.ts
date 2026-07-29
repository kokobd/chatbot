import "server-only";

import { auth } from "@/app/(auth)/auth";
import type { VisibilityType } from "@/components/chat/visibility-selector";
import {
  createChat,
  deleteAllChats,
  deleteChat,
  deleteMessagesAfter,
  getChat,
  getChatHistory,
  getMessage,
  getMessageCount,
  getMessages,
  getVotes,
  getOrCreateIapUser as nativeGetOrCreateIapUser,
  saveMessages as nativeSaveMessages,
  updateMessage as nativeUpdateMessage,
  voteMessage as nativeVoteMessage,
  updateChatTitle,
  updateChatVisibility,
} from "@/lib/native";
import { ChatbotError, type ErrorCode, type Surface } from "../errors";
import type { Chat, DBMessage, User } from "./types";

type NativeError = {
  category: string;
  retryable: boolean;
  message: string;
  reconciliation?: NativeError | null;
};

function nativeError(error: unknown, surface: Surface): ChatbotError {
  if (error instanceof ChatbotError) {
    return error;
  }
  let detail: NativeError | undefined;
  if (error instanceof Error) {
    try {
      detail = JSON.parse(error.message) as NativeError;
    } catch {
      // Native setup errors can be plain messages.
    }
  }
  const type =
    detail?.category === "not_found"
      ? "not_found"
      : detail?.category === "permission_denied"
        ? "forbidden"
        : detail?.category === "unavailable" && detail.retryable
          ? "offline"
          : "bad_request";
  const wrapped = new ChatbotError(`${type}:${surface}` as ErrorCode, {
    cause:
      detail?.message ??
      (error instanceof Error ? error.message : String(error)),
  });
  wrapped.persistence = detail;
  return wrapped;
}

async function database<T>(operation: Promise<T>): Promise<T> {
  try {
    return await operation;
  } catch (error) {
    throw nativeError(error, "database");
  }
}

export async function getOrCreateIapUser({
  email,
  subject,
}: {
  email: string;
  subject: string;
}): Promise<User> {
  return await database(nativeGetOrCreateIapUser(subject, email));
}

export async function saveChat({
  id,
  userId,
  title,
  visibility,
}: {
  id: string;
  userId: string;
  title: string;
  visibility: VisibilityType;
}): Promise<Chat> {
  return await database(
    createChat({
      createdAt: new Date().toISOString(),
      id,
      title,
      userId,
      visibility,
    })
  );
}

export async function deleteChatById({ id }: { id: string }): Promise<Chat> {
  const session = await auth();
  if (!session?.user) {
    throw new ChatbotError("unauthorized:chat");
  }
  return await database(deleteChat(session.user.id, id));
}

export async function deleteAllChatsByUserId({
  userId,
}: {
  userId: string;
}): Promise<{ deletedCount: number }> {
  return await database(deleteAllChats(userId)).then((deletedCount) => ({
    deletedCount,
  }));
}

export async function getChatsByUserId({
  id,
  limit,
  startingAfter,
  endingBefore,
}: {
  id: string;
  limit: number;
  startingAfter: string | null;
  endingBefore: string | null;
}) {
  return await database(
    getChatHistory({ endingBefore, limit, startingAfter, userId: id })
  );
}

export async function getChatById({
  id,
  userId,
}: {
  id: string;
  userId?: string;
}): Promise<Chat | null> {
  const session = userId ? undefined : await auth();
  const owner = userId ?? session?.user?.id;
  if (!owner) {
    return null;
  }
  return database(getChat(owner, id));
}

export async function saveMessages({
  messages,
}: {
  messages: DBMessage[];
}): Promise<DBMessage[]> {
  return await database(nativeSaveMessages(messages));
}

export async function updateMessage({
  id,
  parts,
  userId,
  chatId,
}: {
  id: string;
  parts: DBMessage["parts"];
  userId: string;
  chatId: string;
}): Promise<DBMessage> {
  const existing = await database(getMessage(userId, chatId, id));
  if (!existing) {
    throw new ChatbotError("not_found:database");
  }
  return await database(nativeUpdateMessage({ ...existing, parts }));
}

export async function getMessagesByChatId({
  id,
  userId,
}: {
  id: string;
  userId: string;
}): Promise<DBMessage[]> {
  return await database(getMessages(userId, id));
}

export async function getMessageById({
  id,
  userId,
  chatId,
}: {
  id: string;
  userId: string;
  chatId: string;
}): Promise<DBMessage[]> {
  const message = await database(getMessage(userId, chatId, id));
  return message ? [message] : [];
}

export async function deleteMessagesByChatIdAfterTimestamp({
  chatId,
  timestamp,
  userId,
}: {
  chatId: string;
  timestamp: Date;
  userId: string;
}): Promise<DBMessage[]> {
  return await database(
    deleteMessagesAfter(userId, chatId, timestamp.toISOString())
  );
}

export async function getMessageCountByUserId({
  id,
  differenceInHours,
}: {
  id: string;
  differenceInHours: number;
}): Promise<number> {
  return await database(
    getMessageCount(
      id,
      new Date(Date.now() - differenceInHours * 60 * 60 * 1000).toISOString()
    )
  );
}

export async function updateChatVisibilityById({
  chatId,
  visibility,
  userId,
}: {
  chatId: string;
  visibility: VisibilityType;
  userId: string;
}): Promise<Chat> {
  return await database(updateChatVisibility(userId, chatId, visibility));
}

export async function updateChatTitleById({
  chatId,
  title,
  userId,
}: {
  chatId: string;
  title: string;
  userId: string;
}): Promise<Chat | undefined> {
  try {
    return await database(updateChatTitle(userId, chatId, title));
  } catch {
    // Best effort title update.
  }
}

// Keep these legacy query names as a compatibility adapter while callers move
// to the native aggregate operations.
export async function voteMessage({
  chatId,
  messageId,
  type,
}: {
  chatId: string;
  messageId: string;
  type: "up" | "down";
}): Promise<void> {
  const session = await auth();
  if (!session?.user) {
    throw new ChatbotError("unauthorized:vote");
  }
  await database(
    nativeVoteMessage(session.user.id, chatId, messageId, type === "up")
  );
}

export async function getVotesByChatId({
  id,
}: {
  id: string;
}): Promise<Awaited<ReturnType<typeof getVotes>>> {
  const session = await auth();
  if (!session?.user) {
    throw new ChatbotError("unauthorized:vote");
  }
  return database(getVotes(session.user.id, id));
}
