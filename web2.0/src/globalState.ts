/* eslint-disable solid/reactivity */
import {createRoot, createSignal} from "solid-js";

// this used to break if vite hot reloadsed Main.tsx before i extracted state here
export const GLOBAL_STATE = createRoot(() => ({
	premium: createSignal<boolean>(true),
}));
