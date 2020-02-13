# syntax=docker/dockerfile:experimental
FROM rust:1.41.0-slim as builder

WORKDIR /usr/src/crdocs

COPY backend backend

WORKDIR /usr/src/crdocs/backend

RUN --mount=type=cache,target=/usr/local/cargo,from=rust,source=/usr/local/cargo \
    --mount=type=cache,target=target \
    cargo build && mkdir -p out && cp target/release/crdocs_app out/
# build the frontend

FROM rust:1.41.0-slim as frontend

RUN apt-get update && apt-get install -y pkg-config libssl-dev curl gnupg2

RUN curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | apt-key add - && \
    echo "deb https://dl.yarnpkg.com/debian/ stable main" | tee /etc/apt/sources.list.d/yarn.list && \
    apt-get update && \
    apt-get install -y yarn

WORKDIR /usr/src/crdocs
RUN cargo install wasm-pack

RUN USER=root cargo new --lib frontend

WORKDIR /usr/src/crdocs/frontend

COPY ./frontend/Cargo.lock .
COPY ./frontend/Cargo.toml .

# cache dependencies
RUN wasm-pack build --release

RUN rm -rf target/wasm32-unknown-unknown/release/.fingerprint/crdocs-* 
COPY frontend .

RUN wasm-pack build --mode=force --release

WORKDIR ./www/

# tmp refactor after 
RUN yarn install
RUN yarn build --mode=production

FROM debian:stretch-slim
COPY --from=builder /usr/src/crdocs/backend/out/crdocs_app /usr/local/bin/crdocs
COPY --from=frontend /usr/src/crdocs/frontend/www/dist /usr/local/share/crdocs/

ENV RUST_LOG=info
ENV PORT=3012

CMD crdocs --files "/usr/local/share/crdocs" --port=$PORT

