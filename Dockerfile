FROM node:24-bookworm AS build

ENV PNPM_HOME="/pnpm"
ENV PATH="${PNPM_HOME}:/root/.cargo/bin:${PATH}"

WORKDIR /app

# The native N-API module is compiled for the Linux image during the build.
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    libssl-dev \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --profile minimal --default-toolchain 1.96.0

RUN corepack enable

# Keep dependency installation cacheable until application sources change.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY native/package.json native/package.json
RUN pnpm install --frozen-lockfile

COPY . .
RUN pnpm build

FROM build AS test

RUN pnpm check \
  && cargo test --manifest-path native/Cargo.toml --lib --locked -- \
    --skip infrastructure::firestore::tests::firestore_supports_required_primitives

FROM node:24-bookworm-slim AS runtime

ENV NODE_ENV="production"
ENV HOSTNAME="0.0.0.0"

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends libssl3 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
COPY --from=build /app/public ./public
COPY --from=build /app/native/index.js /app/native/package.json /app/native/chatbot_native.node ./native/

# The workspace package is externalized by Next.js and loads its N-API binary
# relative to the process working directory.
RUN mkdir -p node_modules/@chatbot \
  && rm -rf node_modules/@chatbot/native \
  && ln -s /app/native node_modules/@chatbot/native

EXPOSE 3000

CMD ["node", "server.js"]
