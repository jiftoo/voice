import {JSX, createSignal} from "solid-js";
import "./Slider.css";

export default function Slider(props: {
	disabled?: boolean;
	hideKnobOnDisabled?: boolean;
	lighter?: boolean;
	fillSpace?: boolean;
	min: number;
	max: number;
	step: number;
	value: number;
	onInput: JSX.ChangeEventHandlerUnion<HTMLInputElement, Event>;
	children?: JSX.Element;
}) {
	return (
		<label
			class="range-slider"
			classList={{
				disabled: props.disabled,
				lighter: props.lighter,
				fillSpace: props.fillSpace,
				hideKnob: props.hideKnobOnDisabled
			}}
		>
			<span>{props.children}</span>
			<input
				disabled={props.disabled}
				class="rounded"
				type="range"
				min={props.min}
				max={props.max}
				step={props.step}
				value={props.value}
				onInput={ev => (props.onInput as any)(ev)}
			/>
		</label>
	);
}
