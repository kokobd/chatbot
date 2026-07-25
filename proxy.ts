import { type NextRequest, NextResponse } from "next/server";

export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;

  if (pathname.startsWith("/ping")) {
    return new Response("pong", { status: 200 });
  }

  if (process.env.IAP_AUTH_PROVIDER === "test") {
    const email = process.env.IAP_TEST_EMAIL;
    const subject = process.env.IAP_TEST_SUBJECT;

    if (email && subject) {
      const requestHeaders = new Headers(request.headers);
      requestHeaders.set(
        "x-goog-authenticated-user-email",
        `accounts.google.com:${email}`
      );
      requestHeaders.set(
        "x-goog-authenticated-user-id",
        `accounts.google.com:${subject}`
      );

      return NextResponse.next({
        request: {
          headers: requestHeaders,
        },
      });
    }
  }

  return NextResponse.next();
}

export const config = {
  matcher: [
    "/",
    "/chat/:id",
    "/api/:path*",
    "/((?!_next/static|_next/image|favicon.ico|sitemap.xml|robots.txt).*)",
  ],
};
