import {debounce, leadingAndTrailing} from "@solid-primitives/scheduled";
import {Accessor, createEffect, createSignal} from "solid-js";

export function createResourceDebounced<T, R>(source: Accessor<T>, fetcher: (v: T) => Promise<R>, wait: number): Accessor<R | undefined> {
	const [resource, setResource] = createSignal<R | undefined>(undefined);
	const trigger = leadingAndTrailing(
		debounce,
		(v: T) => {
			fetcher(v).then(setResource);
		},
		wait
	);
	createEffect(() => {
		trigger(source());
	});

	return resource;
}
