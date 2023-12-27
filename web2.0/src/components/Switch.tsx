import {JSX, Signal} from "solid-js";
import "./Switch.css";

export default function Switch(props: {
	value: boolean;
	onChange: (value: boolean) => void;
	children?: JSX.Element;
	disabled?: boolean;
	small?: boolean;
	reverse?: boolean;
}) {
	let disabled = () => props.disabled ?? false;
	return (
		<div
			class="switch-container"
			classList={{disabled: props.disabled, small: props.small, reverse: props.reverse}}
		>
			{props.children && (
				<label>
					<SwitchElement value={props.value} onChange={props.onChange} disabled={disabled()} />
					{props.children}
				</label>
			)}
			{!props.children && <SwitchElement value={props.value} onChange={props.onChange} disabled={disabled()} />}
		</div>
	);
}
function SwitchElement(props: {value: boolean; onChange: (value: boolean) => void; disabled: boolean}) {
	let inputRef: HTMLInputElement | undefined;
	return (
		<label
			class="switch"
			tabIndex={0}
			onKeyDown={ev => {
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
				checked={props.value}
				onInput={ev => props.onChange(ev.currentTarget.checked)}
			/>
			<span class="slider rounded" />
		</label>
	);
}
