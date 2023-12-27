import {createEffect, createSignal, onMount} from "solid-js";
import {Ref, mergeRefs} from "@solid-primitives/refs";
import "./VideoPlayer.css";
import Button from "./Button";
import Slider from "./Slider";
import speakerIcon from "../assets/speaker.svg";
import mutedIcon from "../assets/muted.svg";

export default function VideoPlayer(props: {src: string; ref?: Ref<HTMLVideoElement>; seekbarBackground?: string}) {
	const [isPlaying, setIsPlaying] = createSignal(false);
	const [position, setPosition] = createSignal(0);
	const [volume, setVolume] = createSignal(0.333);
	const [muted, setMuted] = createSignal(false);

	const actualVolume = () => (muted() ? 0 : volume());

	const [videoRef, setVideoRef] = createSignal<HTMLVideoElement>(undefined as any);
	const [videoDuration, setVideoDuration] = createSignal<number | null>(null);

	onMount(() => {
		videoRef().addEventListener("loadedmetadata", () => {
			setVideoDuration(videoRef().duration);
		});
	});

	const togglePlay = () => {
		if (!videoDuration()) return;

		if (videoRef().paused) {
			videoRef().play();
			setIsPlaying(true);
		} else {
			videoRef().pause();
			setIsPlaying(false);
		}
	};

	const seek = (event: Event) => {
		if (!videoDuration()) return;

		const position = (event.target as HTMLInputElement).value;
		videoRef().currentTime = +position;
		setPosition(+position);
	};

	return (
		<div class="video-player rounded" onKeyPress={togglePlay}>
			<video
				class=""
				src={props.src}
				ref={mergeRefs(props.ref, setVideoRef)}
				onTimeUpdate={ev => setPosition(ev.currentTarget.currentTime)}
				onEnded={() => setIsPlaying(false)}
				onClick={togglePlay}
			/>
			<input
				class="seekbar rounded"
				style={{background: props.seekbarBackground}}
				type="range"
				min="0"
				max={videoDuration()!}
				step="0.01"
				value={position()}
				onInput={seek}
			/>
			<div class="video-controls rounded smaller-slider-gap-hack">
				<Button variant="accent" onClick={togglePlay}>
					{isPlaying() ? "Pause" : "Play"}
				</Button>
				<Slider
					min={0}
					max={1}
					step={0.01}
					value={actualVolume()}
					onInput={ev =>
						(
							// this is a race condition if solid signals aren't syncronous
							setVolume(+ev.currentTarget.value === 0 && muted() ? 0.333 : +ev.currentTarget.value),
							setMuted(+ev.currentTarget.value === 0)
						)
					}
				>
					<Button small onClick={() => setMuted(v => !v)}>
						<img src={muted() ? mutedIcon : speakerIcon} style={{filter: "invert()"}} height="24px" />
					</Button>
				</Slider>
			</div>
		</div>
	);
}
