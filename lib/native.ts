import "server-only";

import {
  type AuthenticatedIdentity,
  createService,
  type ExternalObject,
  type IapRequestHeaders,
  type MessageDto,
  authenticateIapRequest as nativeAuthenticateIapRequest,
  createChat as nativeCreateChat,
  createDocument as nativeCreateDocument,
  createStream as nativeCreateStream,
  deleteAllChats as nativeDeleteAllChats,
  deleteChat as nativeDeleteChat,
  deleteDocumentsAfter as nativeDeleteDocumentsAfter,
  deleteMessagesAfter as nativeDeleteMessagesAfter,
  getChat as nativeGetChat,
  getChatHistory as nativeGetChatHistory,
  getDocument as nativeGetDocument,
  getDocuments as nativeGetDocuments,
  getMessage as nativeGetMessage,
  getMessageCount as nativeGetMessageCount,
  getMessages as nativeGetMessages,
  getOrCreateIapUser as nativeGetOrCreateIapUser,
  getStreams as nativeGetStreams,
  getSuggestions as nativeGetSuggestions,
  getVotes as nativeGetVotes,
  saveMessages as nativeSaveMessages,
  saveSuggestions as nativeSaveSuggestions,
  updateChatTitle as nativeUpdateChatTitle,
  updateChatVisibility as nativeUpdateChatVisibility,
  updateMessage as nativeUpdateMessage,
  uploadObject as nativeUploadObject,
  voteMessage as nativeVoteMessage,
  type Service,
  type UploadResult,
} from "@chatbot/native";
import type {
  Chat,
  DBMessage,
  Document,
  Suggestion,
  Timestamp,
  User,
} from "./db/types";

type NativeService = ExternalObject<Service>;
let servicePromise: Promise<NativeService> | undefined;

function getService() {
  servicePromise ??= createService();
  return servicePromise;
}

export function authenticateIapRequest(headers: {
  authenticatedUserEmail: string | null;
  authenticatedUserId: string | null;
  jwtAssertion: string | null;
}): Promise<AuthenticatedIdentity | null> {
  const input: IapRequestHeaders = {
    authenticatedUserEmail: headers.authenticatedUserEmail ?? undefined,
    authenticatedUserId: headers.authenticatedUserId ?? undefined,
    jwtAssertion: headers.jwtAssertion ?? undefined,
  };
  return getService().then((service) =>
    nativeAuthenticateIapRequest(service, input)
  );
}

export function uploadObject(
  data: Buffer,
  filename: string,
  contentType: string
): Promise<UploadResult> {
  return getService().then((service) =>
    nativeUploadObject(service, data, filename, contentType)
  );
}

export function getOrCreateIapUser(
  subject: string,
  email: string
): Promise<User> {
  return getService().then(
    (service) =>
      nativeGetOrCreateIapUser(service, subject, email) as unknown as User
  );
}

export function createChat(input: {
  id: string;
  userId: string;
  title: string;
  visibility: Chat["visibility"];
  createdAt: Timestamp;
}): Promise<Chat> {
  return getService().then(
    (service) =>
      nativeCreateChat(
        service,
        input.id,
        input.userId,
        input.title,
        input.visibility,
        String(
          input.createdAt instanceof Date
            ? input.createdAt.toISOString()
            : input.createdAt
        )
      ) as unknown as Chat
  );
}

export function getChat(userId: string, chatId: string): Promise<Chat | null> {
  return getService().then(
    (service) =>
      nativeGetChat(service, userId, chatId) as unknown as Promise<Chat | null>
  );
}

export function updateChatTitle(
  userId: string,
  chatId: string,
  title: string
): Promise<Chat> {
  return getService().then(
    (service) =>
      nativeUpdateChatTitle(
        service,
        userId,
        chatId,
        title
      ) as unknown as Promise<Chat>
  );
}

export function updateChatVisibility(
  userId: string,
  chatId: string,
  visibility: Chat["visibility"]
): Promise<Chat> {
  return getService().then(
    (service) =>
      nativeUpdateChatVisibility(
        service,
        userId,
        chatId,
        visibility
      ) as unknown as Promise<Chat>
  );
}

export function deleteChat(userId: string, chatId: string): Promise<Chat> {
  return getService().then(
    (service) =>
      nativeDeleteChat(service, userId, chatId) as unknown as Promise<Chat>
  );
}

export function deleteAllChats(userId: string): Promise<number> {
  return getService().then((service) => nativeDeleteAllChats(service, userId));
}

export function getChatHistory(input: {
  userId: string;
  limit: number;
  startingAfter: string | null;
  endingBefore: string | null;
}): Promise<{ chats: Chat[]; hasMore: boolean; nextCursor?: string | null }> {
  return getService().then(
    (service) =>
      nativeGetChatHistory(
        service,
        input.userId,
        input.limit,
        input.startingAfter ?? undefined,
        input.endingBefore ?? undefined
      ) as unknown as Promise<{ chats: Chat[]; hasMore: boolean }>
  );
}

function messageInput(message: DBMessage) {
  return {
    attachments: JSON.stringify(message.attachments),
    chatId: message.chatId,
    createdAt:
      message.createdAt instanceof Date
        ? message.createdAt.toISOString()
        : message.createdAt,
    id: message.id,
    parts: JSON.stringify(message.parts),
    role: message.role,
    userId: message.userId,
  };
}

function decodeMessage(message: MessageDto): DBMessage {
  return {
    ...message,
    attachments: JSON.parse(message.attachments),
    parts: JSON.parse(message.parts),
  } as DBMessage;
}

export function saveMessages(messages: DBMessage[]): Promise<DBMessage[]> {
  return getService()
    .then((service) => nativeSaveMessages(service, messages.map(messageInput)))
    .then((saved) => saved.map(decodeMessage));
}

export function updateMessage(message: DBMessage): Promise<DBMessage> {
  return getService()
    .then((service) => nativeUpdateMessage(service, messageInput(message)))
    .then(decodeMessage);
}

export function getMessage(
  userId: string,
  chatId: string,
  messageId: string
): Promise<DBMessage | null> {
  return getService()
    .then((service) => nativeGetMessage(service, userId, chatId, messageId))
    .then((message) => (message ? decodeMessage(message) : null));
}

export function getMessages(
  userId: string,
  chatId: string
): Promise<DBMessage[]> {
  return getService()
    .then((service) => nativeGetMessages(service, userId, chatId))
    .then((messages) => messages.map(decodeMessage));
}

export function getMessageCount(
  userId: string,
  cutoff: Timestamp
): Promise<number> {
  return getService().then((service) =>
    nativeGetMessageCount(
      service,
      userId,
      cutoff instanceof Date ? cutoff.toISOString() : String(cutoff)
    )
  );
}

export function deleteMessagesAfter(
  userId: string,
  chatId: string,
  cutoff: Timestamp
): Promise<DBMessage[]> {
  return getService()
    .then((service) =>
      nativeDeleteMessagesAfter(
        service,
        userId,
        chatId,
        cutoff instanceof Date ? cutoff.toISOString() : cutoff
      )
    )
    .then((messages) => messages.map(decodeMessage));
}

export function saveDocument(input: {
  id: string;
  userId: string;
  title: string;
  kind: Document["kind"];
  content: string;
}): Promise<Document> {
  return getService().then(
    (service) =>
      nativeCreateDocument(
        service,
        input.id,
        input.userId,
        input.title,
        input.kind,
        input.content
      ) as unknown as Promise<Document>
  );
}

export function getDocuments(
  userId: string,
  documentId: string
): Promise<Document[]> {
  return getService().then(
    (service) =>
      nativeGetDocuments(service, userId, documentId) as unknown as Promise<
        Document[]
      >
  );
}

export function getDocument(
  userId: string,
  documentId: string
): Promise<Document | null> {
  return getService().then(
    (service) =>
      nativeGetDocument(
        service,
        userId,
        documentId
      ) as unknown as Promise<Document | null>
  );
}

export function deleteDocumentsAfter(
  userId: string,
  documentId: string,
  cutoff: Timestamp
): Promise<Document[]> {
  return getService().then(
    (service) =>
      nativeDeleteDocumentsAfter(
        service,
        userId,
        documentId,
        cutoff instanceof Date ? cutoff.toISOString() : cutoff
      ) as unknown as Promise<Document[]>
  );
}

export function saveSuggestions(
  suggestions: Suggestion[]
): Promise<Suggestion[]> {
  return getService()
    .then((service) =>
      nativeSaveSuggestions(
        service,
        suggestions.map((suggestion) => ({
          ...suggestion,
          createdAt:
            suggestion.createdAt instanceof Date
              ? suggestion.createdAt.toISOString()
              : suggestion.createdAt,
          description: suggestion.description ?? undefined,
          versionId: suggestion.versionId ?? "",
        }))
      )
    )
    .then((saved) => saved as unknown as Suggestion[]);
}

export function getSuggestions(
  userId: string,
  documentId: string
): Promise<Suggestion[]> {
  return getService().then(
    (service) =>
      nativeGetSuggestions(service, userId, documentId) as unknown as Promise<
        Suggestion[]
      >
  );
}

export function voteMessage(
  userId: string,
  chatId: string,
  messageId: string,
  isUpvoted: boolean
) {
  return getService().then((service) =>
    nativeVoteMessage(service, userId, chatId, messageId, isUpvoted)
  );
}

export function getVotes(userId: string, chatId: string) {
  return getService().then((service) =>
    nativeGetVotes(service, userId, chatId)
  );
}

export function createStream(
  userId: string,
  streamId: string,
  chatId: string,
  createdAt: Timestamp
) {
  return getService().then((service) =>
    nativeCreateStream(
      service,
      userId,
      streamId,
      chatId,
      createdAt instanceof Date ? createdAt.toISOString() : createdAt
    )
  );
}

export function getStreams(userId: string, chatId: string) {
  return getService().then((service) =>
    nativeGetStreams(service, userId, chatId)
  );
}
