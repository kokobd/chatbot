"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useActiveChat } from "@/hooks/use-active-chat";
import type { Attachment, ChatMessage } from "@/lib/types";
import { ChatHeader } from "./chat-header";
import { submitEditedMessage } from "./message-editor";
import { Messages } from "./messages";
import { MultimodalInput } from "./multimodal-input";

export function ChatShell() {
  const {
    chatId,
    messages,
    setMessages,
    sendMessage,
    status,
    stop,
    regenerate,
    input,
    setInput,
    visibilityType,
    isReadonly,
    isLoading,
    votes,
    currentModelId,
    setCurrentModelId,
  } = useActiveChat();
  const [editingMessage, setEditingMessage] = useState<ChatMessage | null>(
    null
  );
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const stopRef = useRef(stop);
  stopRef.current = stop;

  const previousChatId = useRef(chatId);
  useEffect(() => {
    if (previousChatId.current !== chatId) {
      previousChatId.current = chatId;
      stopRef.current();
      setEditingMessage(null);
      setAttachments([]);
    }
  }, [chatId]);

  const handleEditMessage = useCallback(
    (message: ChatMessage) => {
      setInput(
        message.parts
          .filter((part) => part.type === "text")
          .map((part) => part.text)
          .join("")
      );
      setEditingMessage(message);
    },
    [setInput]
  );

  const handleCancelEdit = useCallback(() => {
    setEditingMessage(null);
    setInput("");
  }, [setInput]);

  const handleSendEditedMessage = useCallback(async () => {
    if (!editingMessage) {
      return;
    }
    setEditingMessage(null);
    await submitEditedMessage({
      message: editingMessage,
      regenerate,
      setMessages,
      text: input,
    });
    setInput("");
  }, [editingMessage, input, regenerate, setInput, setMessages]);

  return (
    <div className="flex h-dvh w-full min-w-0 flex-col bg-sidebar">
      <ChatHeader
        chatId={chatId}
        isReadonly={isReadonly}
        selectedVisibilityType={visibilityType}
      />
      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-background md:rounded-tl-[12px] md:border-t md:border-l md:border-border/40">
        <Messages
          chatId={chatId}
          isLoading={isLoading}
          isReadonly={isReadonly}
          messages={messages}
          onEditMessage={handleEditMessage}
          regenerate={regenerate}
          setMessages={setMessages}
          status={status}
          votes={votes}
        />
        <div className="sticky bottom-0 z-1 mx-auto flex w-full max-w-4xl gap-2 border-t-0 bg-background px-2 pb-3 md:px-4 md:pb-4">
          {!isReadonly && (
            <MultimodalInput
              attachments={attachments}
              chatId={chatId}
              editingMessage={editingMessage}
              input={input}
              onCancelEdit={handleCancelEdit}
              onModelChange={setCurrentModelId}
              selectedModelId={currentModelId}
              sendMessage={
                editingMessage ? handleSendEditedMessage : sendMessage
              }
              setAttachments={setAttachments}
              setInput={setInput}
              setMessages={setMessages}
              status={status}
              stop={stop}
            />
          )}
        </div>
      </div>
    </div>
  );
}
