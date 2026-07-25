import "server-only";

import {
  createService,
  uploadObject as nativeUploadObject,
} from "@chatbot/native";

let servicePromise: ReturnType<typeof createService> | undefined;

function getService() {
  servicePromise ??= createService();
  return servicePromise;
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
