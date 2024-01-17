import {useParams} from "@solidjs/router";
import "./Task.css";
import CopyableToken from "../components/CopyableToken";
import skipsJson from "../assets/mitSkips.json";
import {createEffect, createSignal, onCleanup, onMount} from "solid-js";
import {IntervalTree} from "../intervalTree";
import VideoPlayer from "../components/VideoPlayer";
import {readFileUrl, waveformUrl} from "../rest";

function generateSkipLinearGradient(
	videoId: string,
	ranges: [number, number][],
	total: number,
	skipColor: string,
	noSkipColor: string
): string {
	let gradient = `linear-gradient(to right, ${noSkipColor} `;

	for (let i = 0; i < ranges.length; i++) {
		const start_pct = (ranges[i][0] / total) * 100;
		const end_pct = (ranges[i][1] / total) * 100;
		gradient += `, ${noSkipColor} ${start_pct}%, ${skipColor} ${start_pct}%, ${skipColor} ${end_pct}%, ${noSkipColor} ${end_pct}%`;
	}

	gradient += ")";

	// return gradient;
	return `url(${waveformUrl(videoId)}), ${gradient}`;
}

export default function Task() {
	const skips = skipsJson.filter(([a, b]) => b - a > 0.2) as [number, number][];
	const {id: videoId} = useParams();

	const [videoRef, setVideoRef] = createSignal<HTMLVideoElement | undefined>(undefined);
	const [videoDuration, setVideoDuration] = createSignal<number | null>(null);

	onMount(() => {
		const video = videoRef()!;
		video.addEventListener("loadedmetadata", () => {
			setVideoDuration(video.duration);
		});

		const intervalTree = new IntervalTree(skips);

		let skipped = false;
		const skipTimerHandle = setInterval(() => {
			if (!video) return;
			const skip = intervalTree.search(video.currentTime);
			if (skip && !skipped) {
				console.log("skipped", skip);
				video.currentTime = skip.end;
				skipped = true;
			}
			if (!skip) {
				skipped = false;
			}
		}, 10);
		onCleanup(() => clearInterval(skipTimerHandle));
	});

	createEffect(() => {
		console.log(videoDuration());
	});

	return (
		<>
			<h4>
				Task <CopyableToken>{videoId}</CopyableToken>
			</h4>
			<VideoPlayer
				src={readFileUrl(videoId)}
				ref={setVideoRef}
				seekbarBackground={
					videoDuration()
						? generateSkipLinearGradient(
								videoId,
								skips as any,
								videoDuration()!,
								"var(--skip-segment-color)",
								"var(--non-skip-segment-color)"
							)
						: undefined
				}
			/>
		</>
	);
}
