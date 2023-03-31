####################################################################################################
## Builder
####################################################################################################
FROM rust:latest AS builder

RUN update-ca-certificates

# Create appuser
ENV USER=app
ENV UID=10001

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    "${USER}"


WORKDIR /app

# build dependencies (they are slow-moving)
COPY ./Cargo.toml ./Cargo.lock .
RUN mkdir src && echo "fn main() {}" > src/main.rs

RUN cargo build --release
RUN rm src/main.rs

COPY ./src ./src

# build for release
RUN touch src/main.rs
RUN cargo build --release

####################################################################################################
## Final image
####################################################################################################
FROM debian:buster-slim

# Import from builder.
COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

WORKDIR /app

# install ssl
RUN apt-get update && apt install -y openssl ca-certificates

# Copy our build
COPY --from=builder /app/target/release/app ./

# Use an unprivileged user.
USER app:app

EXPOSE 3000
CMD ["/app/app"]
