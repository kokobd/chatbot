import { expect, test } from "@playwright/test";

const requiredEnvironment = ["GCS_BUCKET", "POSTGRES_URL"];

function assertConfiguredEnvironment() {
  const missing = requiredEnvironment.filter((name) => !process.env[name]);

  if (missing.length > 0) {
    throw new Error(
      `File upload e2e prerequisites are missing: ${missing.join(", ")}`
    );
  }
}

test("uploads a PNG through the chat UI and serves the public object", async ({
  page,
}) => {
  assertConfiguredEnvironment();

  await page.goto("/");

  const png = Buffer.from(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63606060000000050001a5a2cdd40000000049454e44ae426082",
    "hex"
  );
  const fileInput = page.getByTestId("file-input");
  const uploadResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/api/files/upload") &&
      response.request().method() === "POST"
  );

  await fileInput.setInputFiles({
    buffer: png,
    mimeType: "image/png",
    name: "acceptance.png",
  });

  const uploadResponse = await uploadResponsePromise;
  if (!uploadResponse.ok()) {
    throw new Error(
      `File upload failed with HTTP ${uploadResponse.status()}. Verify Google credentials, GCS_BUCKET, and local server configuration: ${await uploadResponse.text()}`
    );
  }
  expect(uploadResponse.ok()).toBe(true);
  const upload = await uploadResponse.json();

  expect(upload.contentType).toBe("image/png");
  expect(upload.pathname).toBe("acceptance.png");
  expect(upload.url).toMatch(
    new RegExp(
      `^https://storage\\.googleapis\\.com/${process.env.GCS_BUCKET}/uploads/[^/]+/acceptance\\.png$`
    )
  );

  const publicResponse = await page.request.get(upload.url);
  expect(publicResponse.ok()).toBe(true);
  expect(publicResponse.headers()["content-type"]).toMatch(/^image\/png/);

  const attachmentPreview = page.getByTestId("input-attachment-preview");
  await expect(attachmentPreview).toBeVisible();
  await expect(page.getByAltText("acceptance.png")).toBeVisible();
});
