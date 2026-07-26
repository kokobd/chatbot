"use server";

import { auth } from "@/app/(auth)/auth";
import { getSuggestionsByDocumentId } from "@/lib/db/queries";

export async function getSuggestions({ documentId }: { documentId: string }) {
  const session = await auth();
  const suggestions = session?.user
    ? await getSuggestionsByDocumentId({ documentId, userId: session.user.id })
    : [];
  return suggestions ?? [];
}
