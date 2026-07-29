FROM node:24-bookworm AS chef

ENV PNPM_HOME="/pnpm"
ENV PATH="${PNPM_HOME}:/root/.cargo/bin:${PATH}"
# Keep cargo-chef, napi-rs, and the test command on the same artifacts path.
ENV CARGO_TARGET_DIR="/app/native/target"

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

# Pin cargo-chef so dependency recipes remain reproducible across builds.
RUN cargo install cargo-chef --version 0.1.77 --locked

# cargo-chef derives a dependency-only recipe from the native crate before the
# application build copies its source into the cacheable build stage.
FROM chef AS planner

WORKDIR /app/native

COPY native ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cache

WORKDIR /app

# Keep dependency installation cacheable until application sources change.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY native/package.json native/package.json
RUN pnpm install --frozen-lockfile

# Cache Rust dependencies for both the debug test target and the release N-API
# build. The generated recipe changes only when Cargo metadata changes.
COPY --from=planner /app/native/recipe.json native/recipe.json
WORKDIR /app/native
RUN cargo chef cook --locked --tests --recipe-path recipe.json \
  && cargo chef cook --locked --release --recipe-path recipe.json
# recipe.json is an internal cache input and must not reach repository checks.
RUN rm recipe.json

FROM cache AS build

WORKDIR /app
COPY . .
RUN pnpm build

FROM build AS test

RUN pnpm check \
  && cd native \
  && cargo test --lib --locked -- \
    --skip infrastructure::firestore::tests::firestore_supports_required_primitives

FROM node:24-bookworm-slim AS runtime

ENV NODE_ENV="production"
ENV HOSTNAME="0.0.0.0"

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 \
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
