import type { UseChatHelpers } from "@ai-sdk/react";
import { ArrowDownIcon } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { useMessages } from "@/hooks/use-messages";
import type { Vote } from "@/lib/db/types";
import type { ChatMessage } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Greeting } from "./greeting";
import { PreviewMessage, ThinkingMessage } from "./message";

type MessagesProps = {
  chatId: string;
  status: UseChatHelpers<ChatMessage>["status"];
  votes: Vote[] | undefined;
  messages: ChatMessage[];
  setMessages: UseChatHelpers<ChatMessage>["setMessages"];
  regenerate: UseChatHelpers<ChatMessage>["regenerate"];
  isReadonly: boolean;
  isLoading?: boolean;
  onEditMessage?: (message: ChatMessage) => void;
};

export function Messages({
  chatId,
  status,
  votes,
  messages,
  setMessages,
  regenerate,
  isReadonly,
  isLoading,
  onEditMessage,
}: MessagesProps) {
  const {
    containerRef,
    endRef,
    isAtBottom,
    scrollToBottom,
    hasSentMessage,
    reset,
  } = useMessages({ status });
  const previousChatId = useRef(chatId);
  useEffect(() => {
    if (previousChatId.current !== chatId) {
      previousChatId.current = chatId;
      reset();
    }
  }, [chatId, reset]);

  const handleScrollToBottom = useCallback(
    () => scrollToBottom("smooth"),
    [scrollToBottom]
  );
  return (
    <div className="relative flex-1 bg-background">
      {messages.length === 0 && !isLoading && (
        <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center">
          <Greeting />
        </div>
      )}
      <div
        className={cn(
          "absolute inset-0 touch-pan-y overflow-y-auto",
          messages.length > 0 ? "bg-background" : "bg-transparent"
        )}
        ref={containerRef}
      >
        <div className="mx-auto flex min-h-full min-w-0 max-w-4xl flex-col gap-7 px-4 py-8 md:gap-8 md:px-6 md:py-10">
          {messages.map((message, index) => (
            <PreviewMessage
              chatId={chatId}
              isLoading={
                status === "streaming" && messages.length - 1 === index
              }
              isReadonly={isReadonly}
              key={message.id}
              message={message}
              onEdit={onEditMessage}
              regenerate={regenerate}
              requiresScrollPadding={
                hasSentMessage && index === messages.length - 1
              }
              setMessages={setMessages}
              vote={votes?.find((vote) => vote.messageId === message.id)}
            />
          ))}
          {status === "submitted" && messages.at(-1)?.role !== "assistant" && (
            <ThinkingMessage />
          )}
          <div className="min-h-[24px] min-w-[24px] shrink-0" ref={endRef} />
        </div>
      </div>
      <button
        aria-label="Scroll to bottom"
        className={`absolute bottom-5 left-1/2 z-10 flex h-9 -translate-x-1/2 items-center rounded-full border border-border bg-card px-4 text-sm shadow-[var(--shadow-float)] transition-all duration-200 ${isAtBottom ? "pointer-events-none scale-90 opacity-0" : "pointer-events-auto scale-100 opacity-100"}`}
        onClick={handleScrollToBottom}
        type="button"
      >
        <ArrowDownIcon className="size-4 text-muted-foreground" />
      </button>
    </div>
  );
}
