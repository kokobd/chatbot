export type Timestamp = string | Date;
export type Visibility = "private" | "public";
export type MessageRole = "user" | "assistant" | "system" | "tool";
export type ArtifactKind = "text" | "code" | "image" | "sheet";

export type User = {
  id: string;
  email: string;
  iapSubject: string | null;
  createdAt: Timestamp;
  updatedAt: Timestamp;
  image?: string | null;
  name?: string | null;
};

export type Chat = {
  id: string;
  userId: string;
  title: string;
  visibility: Visibility;
  lifecycle: "active" | "deleting" | "deleted";
  createdAt: Timestamp;
  deletedAt: Timestamp | null;
  lifecycleRevision: number;
};

export type DBMessage = {
  id: string;
  chatId: string;
  userId: string;
  role: MessageRole;
  parts: unknown;
  attachments: unknown;
  createdAt: Timestamp;
};

export type Vote = {
  chatId: string;
  messageId: string;
  isUpvoted: boolean;
};

export type Document = {
  id: string;
  versionId?: string;
  userId: string;
  title: string;
  kind: ArtifactKind;
  content: string | null;
  createdAt: Timestamp;
};

export type Suggestion = {
  id: string;
  documentId: string;
  versionId?: string;
  userId: string;
  originalText: string;
  suggestedText: string;
  description: string | null;
  isResolved: boolean;
  createdAt: Timestamp;
  documentCreatedAt?: Date;
};
