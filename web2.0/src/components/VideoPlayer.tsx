import {createEffect, createSignal, onMount} from "solid-js";
import {Ref, mergeRefs} from "@solid-primitives/refs";
import "./VideoPlayer.css";
import Button from "./Button";
import Slider from "./Slider";
import speakerIcon from "../assets/speaker.svg";
import mutedIcon from "../assets/muted.svg";
import playIcon from "../assets/play.svg";
import pauseIcon from "../assets/pause.svg";

function formatTime(time: number) {
	const minutes = Math.floor(time / 60);
	const seconds = Math.floor(time % 60);
	return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export default function VideoPlayer(props: {src: string; ref?: Ref<HTMLVideoElement>; seekbarBackground?: string}) {
	const [isPlaying, setIsPlaying] = createSignal(false);
	const [position, setPosition] = createSignal(0);
	const [volume, setVolume] = createSignal(0.333);
	const [muted, setMuted] = createSignal(false);

	const actualVolume = () => (muted() ? 0 : volume());

	const [videoRef, setVideoRef] = createSignal<HTMLVideoElement>(undefined as any);

	const [videoTime, setVideoTime] = createSignal(0);
	const [videoDuration, setVideoDuration] = createSignal<number | null>(null);

	const onTimeUpdate = (ev: any) => {
		setVideoTime(ev.currentTarget.currentTime);
	};

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

	const keyDownActions = (ev: KeyboardEvent) => {
		if (ev.key === " ") {
			togglePlay();
		}
		if (ev.key === "ArrowLeft") {
			ev.preventDefault();
			videoRef().currentTime -= 5;
		}
		if (ev.key === "ArrowRight") {
			ev.preventDefault();
			videoRef().currentTime += 5;
		}
		if (ev.key === "ArrowUp") {
			ev.preventDefault();
			setVolume(v => Math.min(1, v + 0.1));
		}
		if (ev.key === "ArrowDown") {
			ev.preventDefault();
			setVolume(v => Math.max(0, v - 0.1));
		}
		if (ev.key === "m") {
			setMuted(v => !v);
		}
	};

	return (
		<div class="video-player rounded" onKeyDown={keyDownActions} tabIndex={-1}>
			<div class="video-wrapper">
				<div class="overlay-indicator">
					{isPlaying() ? <img class="playing" src={playIcon} /> : <img class="paused" src={pauseIcon} />}
				</div>
				<video
					class=""
					src={props.src}
					ref={mergeRefs(props.ref, setVideoRef)}
					onTimeUpdate={ev => setPosition(ev.currentTarget.currentTime)}
					onEnded={() => setIsPlaying(false)}
					onClick={togglePlay}
					ontimeupdate={onTimeUpdate}
					onLoadedMetadata={() => setVideoDuration(videoRef().duration)}
					onWaiting={() => console.log("waiting")}
				/>
			</div>
			<input
				class="seekbar rounded"
				style={{"background-image": props.seekbarBackground}}
				type="range"
				min="0"
				max={videoDuration()!}
				step="0.01"
				value={position()}
				onInput={seek}
			/>
			<div class="video-controls rounded smaller-slider-gap-hack">
				<div class="left-wrapper">
					<Button variant="accent" lineHeight={0} onClick={togglePlay}>
						<img src={isPlaying() ? pauseIcon : playIcon} />
					</Button>
					<span>
						{formatTime(videoTime())} / {formatTime(videoDuration()!)}
					</span>
				</div>
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
					onKeyDown={ev => ev.stopPropagation()}
				>
					<Button small lineHeight={0} onClick={() => setMuted(v => !v)}>
						<img src={muted() ? mutedIcon : speakerIcon} style={{filter: "invert()"}} height="24px" />
					</Button>
				</Slider>
			</div>
		</div>
	);
}
