import type { Chat } from "@/lib/db/types";

export type ChatHistory = {
  chats: Chat[];
  hasMore: boolean;
  nextCursor?: string | null;
};

export const CHAT_HISTORY_PAGE_SIZE = 20;

export function getChatHistoryPageKey() {
  return `${process.env.NEXT_PUBLIC_BASE_PATH ?? ""}/api/history?limit=${CHAT_HISTORY_PAGE_SIZE}`;
}

export function getChatHistoryPaginationKey(
  pageIndex: number,
  previousPageData: ChatHistory | null
) {
  if (previousPageData && previousPageData.hasMore === false) {
    return null;
  }

  if (pageIndex === 0) {
    return getChatHistoryPageKey();
  }

  if (!previousPageData || previousPageData.chats.length === 0) {
    return null;
  }

  return `${process.env.NEXT_PUBLIC_BASE_PATH ?? ""}/api/history?ending_before=${encodeURIComponent(previousPageData.nextCursor ?? "")}&limit=${CHAT_HISTORY_PAGE_SIZE}`;
}
