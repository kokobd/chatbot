import { headers } from "next/headers";
import { getOrCreateIapUser } from "@/lib/db/queries";
import { authenticateIapRequest } from "@/lib/native";

export type UserType = "regular";

export type AuthUser = {
  email: string;
  id: string;
  image: string | null;
  name: string | null;
  type: UserType;
};

export type Session = {
  user: AuthUser;
};

export async function auth(): Promise<Session | null> {
  const requestHeaders = await headers();
  const identity = await authenticateIapRequest({
    authenticatedUserEmail: requestHeaders.get(
      "x-goog-authenticated-user-email"
    ),
    authenticatedUserId: requestHeaders.get("x-goog-authenticated-user-id"),
    jwtAssertion: requestHeaders.get("x-goog-iap-jwt-assertion"),
  });

  if (!identity) {
    return null;
  }

  const user = await getOrCreateIapUser(identity);

  return {
    user: {
      email: user.email,
      id: user.id,
      image: user.image ?? null,
      name: user.name ?? null,
      type: "regular",
    },
  };
}
