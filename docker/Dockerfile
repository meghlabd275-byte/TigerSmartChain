# Dockerfile for TigerSmartChain Node

FROM golang:1.21-alpine AS builder

# Install build dependencies
RUN apk add --no-cache gcc musl-dev linux-headers

# Set working directory
WORKDIR /app

# Copy go mod files
COPY go.mod go.sum ./
RUN go mod download

# Copy source code
COPY . .

# Build the binary
RUN CGO_ENABLED=1 GOOS=linux go build -o /tigersmartchaind ./cmd/tigersmartchaind

# Final stage
FROM alpine:latest

# Install required packages
RUN apk add --no-cache ca-certificates curl iputils-openrc

# Create non-root user
RUN adduser -D -u 1000 tigersmartchain

# Set working directory
WORKDIR /app

# Copy binary from builder
COPY --from=builder /tigersmartchaind /usr/local/bin/

# Copy configuration
COPY --chown=tigersmartchain:tigersmartchain ./config /app/config

# Set ownership
RUN chown -R tigersmartchain:tigersmartchain /app

# Switch to non-root user
USER tigersmartchain

# Expose ports
EXPOSE 30303 8545 9090

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8545/health || exit 1

# Start the node
CMD ["tigersmartchaind", "start"]