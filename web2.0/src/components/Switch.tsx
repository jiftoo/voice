import {Signal} from "solid-js";
import "./Switch.css";

export default function Switch(props: {signal: Signal<boolean>; label?: string; disabled?: boolean}) {
	let disabled = () => props.disabled ?? false;
	return (
		<div class="switch-container" classList={{disabled: props.disabled}}>
			{props.label && (
				<label>
					{props.label}
					<SwitchElement signal={props.signal} disabled={disabled()} />
				</label>
			)}
			{!props.label && <SwitchElement signal={props.signal} disabled={disabled()} />}
		</div>
	);
}
function SwitchElement(props: {signal: Signal<boolean>; disabled: boolean}) {
	let inputRef: HTMLInputElement | undefined;
	return (
		<label
			class="switch"
			tabIndex={0}
			onKeyDown={(ev) => {
				if (ev.key == "Enter" || ev.key == " ") {
					inputRef?.click();
				}
			}}
		>
			<input
				type="checkbox"
				tabIndex={-1}
				ref={inputRef}
				disabled={props.disabled}
				checked={props.signal[0]()}
				onInput={(e) => {
					console.log("input", e.currentTarget.checked, props.signal[1]);
					props.signal[1](e.currentTarget.checked);
				}}
			/>
			<span class="slider rounded" />
		</label>
	);
}
