FROM rust:1.73
WORKDIR /dist

COPY ./src src
COPY ./Cargo* ./

# ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN apt-get update
# RUN apt-get install -y libpango1.0-dev
RUN cargo build --release --target x86_64-unknown-linux-gnu

FROM alpine
RUN apk add --no-cache gcompat libstdc++ ffmpeg
COPY ./certificates /certificates
COPY ./web /web
COPY --from=0 /dist/target/x86_64-unknown-linux-gnu/release/voice /voice
ENTRYPOINT ["/voice"]
EXPOSE 443