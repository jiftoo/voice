FROM rust:1.73
WORKDIR /dist

COPY ./src /dist
COPY ./web /dist
COPY ./Cargo* /dist

ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target x86_64-unknown-linux-gnu

FROM alpine
COPY --from=0 /dist/target/x86_64-unknown-linux-gnu/release/voice /voice
CMD ["/voice"]
EXPOSE 3000