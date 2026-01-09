# Minimal backend server Dockerfile
FROM golang:1.21-alpine AS builder

WORKDIR /app
COPY minimal-server.go .

RUN go build -ldflags="-s -w" -o minimal-server minimal-server.go

FROM alpine:latest

RUN apk --no-cache add ca-certificates

WORKDIR /root/
COPY --from=builder /app/minimal-server .

EXPOSE 8000

CMD ["./minimal-server"]
