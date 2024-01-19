import {useParams} from "@solidjs/router";
import "./Task.css";
import CopyableToken from "../components/CopyableToken";
import {createEffect, createResource, createSignal, onCleanup, onMount} from "solid-js";
import {IntervalTree} from "../intervalTree";
import VideoPlayer from "../components/VideoPlayer";
import {fetchSkips, getAnalyzeEndpoint, getReadFileEndpoint, getWaveformEndpoint} from "../rest";

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
	return `url(${getWaveformEndpoint(videoId)}), ${gradient}`;
}

export default function Task() {
	const {id: videoId} = useParams();

	let [skipsJson] = createResource(() => videoId, fetchSkips, {
		deferStream: true
	});

	const skipsFiltered = () => {
		if (skipsJson.loading) return null;
		// TODO: memoize
		return skipsJson()!.filter(([a, b]) => b - a > 0.2) as [number, number][];
	};

	const skipIntervalTree = () => {
		const skips = skipsFiltered();
		if (!skips) return null;
		return new IntervalTree(skips);
	};

	const [videoRef, setVideoRef] = createSignal<HTMLVideoElement | undefined>(undefined);
	const [videoDuration, setVideoDuration] = createSignal<number | null>(null);

	onMount(() => {
		const video = videoRef()!;
		video.addEventListener("loadedmetadata", () => {
			setVideoDuration(video.duration);
		});

		let skipped = false;
		const skipTimerHandle = setInterval(() => {
			if (!video) return;
			if (!skipIntervalTree()) return;
			const skip = skipIntervalTree()!.search(video.currentTime);
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
		console.log("videoDuration", videoDuration(), "skipsFiltered", skipsFiltered());
	});

	return (
		<>
			<h4>
				Task <CopyableToken>{videoId}</CopyableToken>
			</h4>
			<VideoPlayer
				src={getReadFileEndpoint(videoId)}
				ref={setVideoRef}
				seekbarBackground={
					(videoDuration() && skipsFiltered())
						? generateSkipLinearGradient(
								videoId,
								skipsFiltered() as any,
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
