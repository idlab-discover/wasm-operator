# Use distroless as minimal base image to package the manager binary
# Refer to https://github.com/GoogleContainerTools/distroless for more details
FROM gcr.io/distroless/cc:nonroot
WORKDIR /

COPY ./controller .

COPY ./*.wasm ./
COPY ./wasm_config.yaml .
ENV  RUST_BACKTRACE=full
USER 65532:65532

ENTRYPOINT ["/controller", "/"]
