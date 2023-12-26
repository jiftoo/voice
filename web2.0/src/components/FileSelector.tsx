import {JSX, Signal, createEffect} from "solid-js";
import "./FileSelector.css";

export default function FileSelector(props: {signal: Signal<File | null>; accept?: string; disabled?: boolean}) {
	const handleFileChange: JSX.ChangeEventHandler<HTMLInputElement, Event> = (e) => {
		props.signal[1](e.target.files ? e.target.files[0] : null);
	};

	let inputRef: HTMLInputElement | undefined;

	createEffect(() => {
		if (props.signal[0]() === null && inputRef) {
			inputRef.value = "";
		}
	});

	return (
		<div class="file-selector rounded" classList={{disabled: props.disabled}}>
			<input type="file" disabled={props.disabled} ref={inputRef} accept={props.accept} onChange={handleFileChange} />
		</div>
	);
}
