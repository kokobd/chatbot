import "server-only";

import {
  createService,
  authenticateIapRequest as nativeAuthenticateIapRequest,
  uploadObject as nativeUploadObject,
} from "@chatbot/native";

let servicePromise: ReturnType<typeof createService> | undefined;

function getService() {
  servicePromise ??= createService();
  return servicePromise;
}

export function authenticateIapRequest(headers: {
  authenticatedUserEmail: string | null;
  authenticatedUserId: string | null;
  jwtAssertion: string | null;
}) {
  return getService().then((service) =>
    nativeAuthenticateIapRequest(service, {
      authenticatedUserEmail: headers.authenticatedUserEmail ?? undefined,
      authenticatedUserId: headers.authenticatedUserId ?? undefined,
      jwtAssertion: headers.jwtAssertion ?? undefined,
    })
  );
}

export function uploadObject(
  data: Buffer,
  filename: string,
  contentType: string
) {
  return getService().then((service) =>
    nativeUploadObject(service, data, filename, contentType)
  );
}
