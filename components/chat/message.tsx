"use client";

import type { UseChatHelpers } from "@ai-sdk/react";
import type { Vote } from "@/lib/db/types";
import type { ChatMessage } from "@/lib/types";
import { cn, sanitizeText } from "@/lib/utils";
import { MessageContent, MessageResponse } from "../ai-elements/message";
import { Shimmer } from "../ai-elements/shimmer";
import { useDataStream } from "./data-stream-provider";
import { SparklesIcon } from "./icons";
import { MessageActions } from "./message-actions";
import { MessageReasoning } from "./message-reasoning";
import { PreviewAttachment } from "./preview-attachment";

function WaitingText() {
  const { waitingStatus } = useDataStream();

  return (
    <div className="flex min-h-[calc(15px*1.65)] min-w-0 items-center text-[15px] leading-[1.65]">
      <Shimmer
        as="span"
        className="font-medium whitespace-normal break-words"
        duration={1}
      >
        {waitingStatus?.message ?? "Waiting..."}
      </Shimmer>
    </div>
  );
}

export const PreviewMessage = ({
  chatId,
  message,
  vote,
  isLoading,
  setMessages: _setMessages,
  regenerate: _regenerate,
  isReadonly,
  requiresScrollPadding: _requiresScrollPadding,
  onEdit,
}: {
  chatId: string;
  message: ChatMessage;
  vote: Vote | undefined;
  isLoading: boolean;
  setMessages: UseChatHelpers<ChatMessage>["setMessages"];
  regenerate: UseChatHelpers<ChatMessage>["regenerate"];
  isReadonly: boolean;
  requiresScrollPadding: boolean;
  onEdit?: (message: ChatMessage) => void;
}) => {
  const isUser = message.role === "user";
  const isAssistant = message.role === "assistant";
  const hasContent = message.parts.some(
    (part) =>
      (part.type === "text" && part.text.trim().length > 0) ||
      (part.type === "reasoning" && part.text.trim().length > 0) ||
      part.type === "file"
  );
  const isThinking = isAssistant && isLoading && !hasContent;

  const parts = message.parts.map((part, index) => {
    const key = `message-${message.id}-part-${index}`;
    if (part.type === "file") {
      return (
        <PreviewAttachment
          attachment={{
            contentType: part.mediaType,
            name: part.filename ?? "image",
            url: part.url,
          }}
          key={key}
        />
      );
    }
    if (part.type === "reasoning" && part.text.trim()) {
      return (
        <MessageReasoning
          isLoading={isLoading}
          key={key}
          reasoning={part.text}
        />
      );
    }
    if (part.type === "text") {
      return (
        <MessageContent
          className={cn("text-[15px] leading-[1.65]", {
            "w-fit max-w-[min(80%,60ch)] overflow-hidden break-words rounded-2xl rounded-br-md border border-border bg-secondary px-4 py-2.5 shadow-[var(--shadow-card)]":
              isUser,
          })}
          data-testid="message-content"
          key={key}
        >
          <MessageResponse
            className={
              isUser
                ? "w-fit max-w-full [&>p]:m-0 [&>p]:w-fit [&>p]:leading-[1.65]"
                : undefined
            }
          >
            {sanitizeText(part.text)}
          </MessageResponse>
        </MessageContent>
      );
    }
    return null;
  });

  const content = isThinking ? (
    <WaitingText />
  ) : (
    <>
      <div className={cn("flex flex-wrap gap-2", isUser && "justify-end")}>
        {parts}
      </div>
      {!isReadonly && (
        <MessageActions
          chatId={chatId}
          isLoading={isLoading}
          message={message}
          onEdit={onEdit ? () => onEdit(message) : undefined}
          vote={vote}
        />
      )}
    </>
  );

  return (
    <div
      className={cn(
        "group/message w-full",
        !isAssistant && "animate-[fade-up_0.25s_cubic-bezier(0.22,1,0.36,1)]"
      )}
      data-role={message.role}
      data-testid={`message-${message.role}`}
    >
      <div
        className={cn(
          isUser ? "flex flex-col items-end gap-2" : "flex items-start gap-3"
        )}
      >
        {isAssistant && (
          <div className="flex h-[calc(15px*1.65)] shrink-0 items-center">
            <div className="flex size-8 items-center justify-center rounded-lg bg-secondary text-foreground ring-1 ring-border">
              <SparklesIcon size={15} />
            </div>
          </div>
        )}
        {isAssistant ? (
          <div className="flex min-w-0 flex-1 flex-col gap-2">{content}</div>
        ) : (
          content
        )}
      </div>
    </div>
  );
};

export const ThinkingMessage = () => (
  <div
    className="group/message w-full"
    data-role="assistant"
    data-testid="message-assistant-loading"
  >
    <div className="flex items-start gap-3">
      <div className="flex h-[calc(15px*1.65)] shrink-0 items-center">
        <div className="flex size-8 items-center justify-center rounded-lg bg-secondary text-foreground ring-1 ring-border">
          <SparklesIcon size={15} />
        </div>
      </div>
      <WaitingText />
    </div>
  </div>
);
