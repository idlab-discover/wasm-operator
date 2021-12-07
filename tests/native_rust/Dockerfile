# Use distroless as minimal base image to package the manager binary
# Refer to https://github.com/GoogleContainerTools/distroless for more details
FROM gcr.io/distroless/cc:nonroot
WORKDIR /

COPY ./ring-rust-controller .

USER 65532:65532

ENTRYPOINT ["/ring-rust-controller"]
