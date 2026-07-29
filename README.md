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
  <a href="#model-providers"><strong>Model Providers</strong></a> ·
  <a href="#running-locally"><strong>Running locally</strong></a>
</p>
<br/>

## Features

- [Next.js](https://nextjs.org) App Router
  - Advanced routing for seamless navigation and performance
  - React Server Components (RSCs) and Server Actions for server-side rendering and increased performance
- [AI SDK](https://ai-sdk.dev/docs/introduction)
  - Unified API for text and vision chat with LLMs
  - Hooks for building streaming chat interfaces
  - Supports OpenAI, Anthropic, Google, DeepSeek, Moonshot, and Z.ai through OpenRouter
- [shadcn/ui](https://ui.shadcn.com)
  - Styling with [Tailwind CSS](https://tailwindcss.com)
  - Component primitives from [Radix UI](https://radix-ui.com) for accessibility and flexibility
- Data Persistence
  - Google Cloud Firestore for chat history, users, and messages
  - Google Cloud Storage for uploaded images
- [GCP Identity-Aware Proxy](https://cloud.google.com/security/products/iap)
  - Authentication and access control at the Cloud Run boundary

## Model Providers

This application uses [OpenRouter](https://openrouter.ai/) to access multiple AI models through a unified interface. Models are configured in `lib/ai/models.ts`. Included models: Kimi K3, DeepSeek V4 Pro, GLM 5.2, ChatGPT Latest, Claude Sonnet Latest, and Gemini Flash Latest.

### OpenRouter Authentication

Provide an OpenRouter API key by setting the `OPENROUTER_API_KEY` environment variable in your `.env.local` file. `OPENROUTER_HTTP_REFERER` and `OPENROUTER_APP_NAME` are optional metadata headers.

With the [AI SDK](https://ai-sdk.dev/docs/introduction), you can also switch to direct LLM providers like [OpenAI](https://openai.com), [Anthropic](https://anthropic.com), [Cohere](https://cohere.com/), and [many more](https://ai-sdk.dev/providers/ai-sdk-providers) with just a few lines of code.

## Running locally

You will need to use the environment variables [defined in `.env.example`](.env.example) to run Chatbot. A `.env` file is sufficient for local development.

Production authentication is provided by GCP Identity-Aware Proxy. Configure
IAP on the Cloud Run service, grant the appropriate IAP access policy, and set
`IAP_JWT_AUDIENCE` to the signed-header audience for that service. The Rust
authentication provider validates the signed IAP assertion using Google's
rotating public keys.

For local development, set `IAP_AUTH_PROVIDER=test`, `IAP_TEST_EMAIL`, and
`IAP_TEST_SUBJECT`. The local Next proxy adds namespaced IAP identity headers to
incoming requests, so a normal browser session is authenticated as that user.
This mode is intended only for local development and tests; the native service
refuses to start in test mode when `NODE_ENV=production`.

Uploaded files use Google Cloud Storage. Set `GCS_BUCKET` and configure Application Default Credentials with `gcloud auth application-default login`, or provide a production service identity through the standard Google Cloud credential mechanisms.

> Note: You should not commit your `.env` file or it will expose secrets that will allow others to control access to your AI provider accounts.

```bash
pnpm install
pnpm dev
```

The application should now be running on [localhost:3000](http://localhost:3000).
