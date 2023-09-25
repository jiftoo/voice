<script lang="ts">
	import {createFFmpeg, fetchFile} from "@ffmpeg/ffmpeg";

	const ffmpeg = createFFmpeg({log: true});

	(async () => {
		// await ffmpeg.load();
        await fetch("https://github.com/ffmpegwasm/testdata/raw/master/video-3s.avi");
		// ffmpeg.FS("writeFile", "test.avi", await fetchFile("https://github.com/ffmpegwasm/testdata/raw/master/video-3s.avi"));
	})();
</script>

<main />

<style>
</style>
