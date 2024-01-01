import {debounce, leadingAndTrailing} from "@solid-primitives/scheduled";
import {Accessor, createEffect, createSignal} from "solid-js";
import "./util.css";

export function createResourceDebounced<T, R>(
	source: Accessor<T>,
	fetcher: (v: T) => Promise<R>,
	wait: number
): [
	Accessor<R | undefined>,
	{
		loading: Accessor<boolean>;
	}
] {
	const [resource, setResource] = createSignal<R | undefined>(undefined);
	const [loading, setLoading] = createSignal(false);
	const trigger = leadingAndTrailing(
		debounce,
		(v: T) => {
			setLoading(true);
			fetcher(v)
				.then(setResource)
				.finally(() => setLoading(false));
		},
		wait
	);
	createEffect(() => {
		trigger(source());
	});

	return [resource, {loading}];
}

export function Loading() {
	return (
		<div class="lds-ring">
			<div></div>
			<div></div>
			<div></div>
			<div></div>
		</div>
	);
}
