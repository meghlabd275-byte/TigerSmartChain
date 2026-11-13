# TigerScan WebSocket image.
#
# The WebSocket fan-out server lives in the root Go module (`ws/`) and is built
# by `cmd/ws_server`. It reads real-time events from Redis and pushes them to
# subscribed browser clients.

FROM golang:1.23-bookworm AS builder

WORKDIR /src

# The module's dependency graph is copied first so the download layer is cached
# independently of the source.
COPY go.mod go.sum ./
RUN go mod download

COPY ws/ ws/
COPY cmd/ws_server/ cmd/ws_server/

RUN CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags="-s -w" \
    -o /out/ws-server ./cmd/ws_server


FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /out/ws-server /usr/local/bin/ws-server

USER nonroot
EXPOSE 8081

ENTRYPOINT ["/usr/local/bin/ws-server"]
