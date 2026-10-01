FROM golang:1.27.1-bookworm@sha256:69a7b9788769bec032d238959b61854e9ae87f57be9029ec04e9885fabf99195 AS build
ENV CGO_ENABLED=0

FROM build AS server-build
ADD https://github.com/minio/minio.git#0d7408fc9969caf07de6a8c3a84f9fbb10a6739e /source
WORKDIR /source
RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build \
    go build -trimpath -ldflags="-s -w" -o /minio .

FROM build AS client-build
ADD https://github.com/minio/mc.git#c1d5d4cbb4caf05afef3ea06a91a56bd778336de /source
WORKDIR /source
RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build \
    go build -trimpath -ldflags="-s -w" -o /mc .

FROM alpine:3.22 AS runtime
RUN apk add --no-cache ca-certificates

FROM runtime AS minio
COPY --from=server-build /minio /usr/local/bin/minio
COPY --from=server-build /source/LICENSE /usr/share/licenses/minio/LICENSE
EXPOSE 9000
ENTRYPOINT ["minio"]
CMD ["server", "/data"]

FROM runtime AS mc
COPY --from=client-build /mc /usr/local/bin/mc
COPY --from=client-build /source/LICENSE /usr/share/licenses/mc/LICENSE
ENTRYPOINT ["mc"]
