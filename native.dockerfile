FROM alpine:latest AS cargo
# copy source code & filter files needed to build deps
WORKDIR /source
COPY ./kube-rs ./kube-rs
COPY ./poc ./poc
RUN mkdir /workdir \
    && find . -name 'Cargo.toml' -exec cp --parents '{}' /workdir \; \
    && find . -name 'Cargo.lock' -exec cp --parents '{}' /workdir \; \
    && find . -name 'lib.rs' -exec cp --parents '{}' /workdir \; \
    && find . -name 'main.rs' -exec cp --parents '{}' /workdir \;

# clear contents of entry rust files that are needed to build deps
WORKDIR /workdir
RUN echo "fn main() {}" > /tmp/main.rs \
    && find . -name 'lib.rs' -exec cp /tmp/main.rs '{}' \; \
    && find . -name 'main.rs' -exec cp /tmp/main.rs '{}' \;

FROM rust:1.56-slim-buster AS builder

RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev \
	&& apt-get clean \
	&& rm -fr /var/lib/apt/lists/*

WORKDIR /workdir

# build deps
COPY --from=cargo /workdir .
RUN cd poc \
    && cargo build -p simple-pod-example --release --features client \
    && mv target /tmp/target \
    && cd .. \
    && rm -rf *

# build releases
COPY ./kube-rs ./kube-rs
COPY ./poc ./poc
RUN mv /tmp/target .
WORKDIR /workdir/poc
RUN cargo build -p simple-pod-example --release --features client

# Use distroless as minimal base image to package the manager binary
# Refer to https://github.com/GoogleContainerTools/distroless for more details
FROM gcr.io/distroless/cc:nonroot
WORKDIR /

COPY --from=builder /workdir/poc/target/release/simple-pod-example .

USER 65532:65532

ENTRYPOINT ["/simple-pod-example"]
