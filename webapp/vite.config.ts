import {defineConfig} from "vite";
import {svelte} from "@sveltejs/vite-plugin-svelte";
import crossoriginIsolation from "vite-plugin-cross-origin-isolation";

// https://vitejs.dev/config/
export default defineConfig({
	plugins: [svelte()],
});
