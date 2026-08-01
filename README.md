<a href="https://chatbot.ai-sdk.dev/demo">
  <img alt="Chatbot" src="app/(chat)/opengraph-image.png">
  <h1 align="center">Chatbot</h1>
</a>

<p align="center">
    Chatbot (formerly AI Chatbot) is a free, open-source chatbot application built with Next.js and the AI SDK.
</p>

<p align="center">
  <a href="https://chatbot.ai-sdk.dev/docs"><strong>Read Docs</strong></a> ·
  <a href="#features"><strong>Features</strong></a> ·
  <a href="#running-locally"><strong>Running locally</strong></a> ·
  <a href="./terraform/README.md"><strong>GCP infrastructure</strong></a>
</p>
<br/>

## Environment model

Local development runs only the application on your machine:

- Next.js, the Rust native service, and the browser run locally.
- Firestore and Google Cloud Storage are real GCP services provisioned by
  Terraform, not local emulators.
- The local application uses the Terraform `test` workspace's named Firestore
  database and upload bucket.
- Google Application Default Credentials authenticate local calls to those
  resources.

Cloud Run, Cloud Build, and production IAP are deployed infrastructure. They
are not started by `pnpm dev`. The complete provisioning workflow is in
[`terraform/README.md`](./terraform/README.md).

## Features

- [Next.js](https://nextjs.org) App Router
  - Advanced routing for seamless navigation and performance
  - React Server Components (RSCs) and Server Actions for server-side rendering and increased performance
- [AI SDK](https://ai-sdk.dev/docs/introduction)
  - Unified API for text and vision chat with streaming responses
  - Supports OpenAI, Anthropic, Google, DeepSeek, Moonshot, and Z.ai through OpenRouter
- [shadcn/ui](https://ui.shadcn.com)
  - Styling with [Tailwind CSS](https://tailwindcss.com)
  - Component primitives from [Radix UI](https://radix-ui.com) for accessibility and flexibility
- Data persistence
  - Google Cloud Firestore for chat history, users, and messages
  - Google Cloud Storage for uploaded images
- [GCP Identity-Aware Proxy](https://cloud.google.com/security/products/iap)
  - Authentication and access control at the Cloud Run boundary

## Model providers

This application uses [OpenRouter](https://openrouter.ai/) to access multiple
AI models through a unified interface. Models are configured in
[`lib/ai/models.ts`](./lib/ai/models.ts). Included models are Kimi K3, DeepSeek
V4 Pro, GLM 5.2, ChatGPT Latest, Claude Sonnet Latest, and Gemini Flash Latest.

### OpenRouter authentication

Set `OPENROUTER_API_KEY` in `.env`. `OPENROUTER_HTTP_REFERER` and
`OPENROUTER_APP_NAME` are optional metadata headers. With the
[AI SDK](https://ai-sdk.dev/docs/introduction), you can also switch to direct
providers such as [OpenAI](https://openai.com),
[Anthropic](https://anthropic.com), and
[many more](https://ai-sdk.dev/providers/ai-sdk-providers).

## Running locally

### 1. Provision or select the test workspace

Do this once for a new project, or whenever the Terraform-backed test
environment has not yet been created. Run the commands from the repository
root; they provision remote GCP resources and do not start a local server.

```bash
gcloud auth application-default login
cp terraform/backend.hcl.example terraform/backend.hcl
terraform -chdir=terraform init -backend-config=backend.hcl
terraform -chdir=terraform workspace new test
terraform -chdir=terraform apply
```

If the workspace already exists, use this instead of `workspace new`:

```bash
terraform -chdir=terraform workspace select test
```

The `test` workspace creates the named Firestore database, upload bucket,
service accounts, and the other environment resources described in the
[Terraform guide](./terraform/README.md). Do not use Terraform's `default`
workspace.

### 2. Configure the local process

Copy the example environment file and fill in the local values. The bucket and
database identifiers should come from the active Terraform `test` workspace:

```bash
cp .env.example .env
terraform -chdir=terraform output -raw bucket_name
terraform -chdir=terraform output -raw firestore_project_id
terraform -chdir=terraform output -raw firestore_database_id
```

Set those three outputs as `GCS_BUCKET`, `FIRESTORE_PROJECT_ID`, and
`FIRESTORE_DATABASE_ID` in `.env`. Also set:

```dotenv
IAP_AUTH_PROVIDER=test
IAP_TEST_EMAIL=local@example.com
IAP_TEST_SUBJECT=local-development-user
OPENROUTER_API_KEY=your-key
```

Local test IAP creates a deterministic local identity; it does not emulate
Firestore or GCS. Keep Google ADC available for the real GCP API calls:

```bash
gcloud auth application-default login
```

`SECRETS_GCS_PATH` is optional for local development. Cloud Run receives its
secret path from Terraform; locally, the OpenRouter key can be supplied
directly through `.env`.

### 3. Run the application

Only the application runs locally:

```bash
pnpm install
pnpm dev
```

Open [http://localhost:3000](http://localhost:3000). The local Next.js/Rust
process connects to the Terraform-provisioned `test` Firestore database and
GCS bucket over Google APIs.

For native or end-to-end verification, see
[`native/README.md`](./native/README.md). Those capability tests also require
the real Terraform-backed resources and ADC; they do not use an emulator.

### Browser UI verification

This workspace provides a Playwright MCP browser for direct local UI testing.
Agents should use it proactively when changing UI behavior or layout:

1. Start the local server with `pnpm dev`, or use a polling watcher if the
   development watcher hits the macOS file-descriptor limit:

   ```bash
   WATCHPACK_POLLING=true pnpm exec next dev --webpack -H 0.0.0.0 -p 3100
   ```

2. Open the relevant route in the Playwright browser.
3. Use an accessibility snapshot to select elements, exercise the interaction,
   inspect console errors, and take a screenshot for layout changes.

Use mocked API responses for deterministic UI checks. Keep real-model smoke
tests short and remove any temporary chats when they are finished.

### Message editing persistence

Submitting an edited message deletes the selected message and all later
messages in that chat atomically. The boundary is the full `(createdAt, id)`
position, including the selected message, which avoids duplicate messages when
the edited message is saved again. The operation supports at most 500 deletes;
larger branches are rejected without partial writes and are not chunked.
Firestore's separate 10 MiB transaction limit still applies and remains
all-or-nothing.

Firestore must have the collection-scoped `messages_position` index on
`createdAt ASC` and `id ASC` in READY state before this path is deployed.
RFC3339 timestamps remain strings across the TypeScript/native boundary to
preserve sub-millisecond precision. The transaction protects its own snapshot;
post-commit writes from another tab or stream are outside its scope.

## Production and deployed environments

Production uses the `main` Terraform workspace. Feature or staging
environments can use separate workspaces. Terraform provisions the Cloud Run
service, Cloud Build trigger, Artifact Registry configuration, IAP policy,
Firestore database, storage bucket, and service accounts for each workspace.

Production authentication is provided by GCP Identity-Aware Proxy. The Rust
authentication provider validates the signed IAP assertion using Google's
rotating public keys. See [`terraform/README.md`](./terraform/README.md) for
workspace deployment, secrets, and Cloud Build details.

> Do not commit `.env`; it contains credentials that can control access to AI
> provider accounts and GCP resources.
