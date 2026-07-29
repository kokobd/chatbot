export type Timestamp = string | Date;
export type Visibility = "private" | "public";
export type MessageRole = "user" | "assistant" | "system" | "tool";

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
