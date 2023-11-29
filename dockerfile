FROM rust as builder
	WORKDIR /dist
	COPY ./src src
	COPY ./Cargo* ./

	ENV RUSTFLAGS="-C target-cpu=icelake-server"
	RUN cargo build --release --target x86_64-unknown-linux-gnu

FROM alpine
	RUN apk add --no-cache gcompat ffmpeg
	# RUN apk add --no-cache ffmpeg
	COPY ./certificates /certificates
	COPY ./web /web
	COPY --from=builder /dist/target/x86_64-unknown-linux-gnu/release/voice /voice
	EXPOSE 443
ENTRYPOINT ["/voice"]