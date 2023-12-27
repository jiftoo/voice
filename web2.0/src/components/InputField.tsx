import {JSX, Signal} from "solid-js";
import "./InputField.css";

type InputFieldProps = {type: "text" | "password"; signal: Signal<string>; placeholder?: string; disabled?: boolean};

export default function InputField(props: InputFieldProps) {
	return (
		<input
			class="input-field"
			disabled={props.disabled}
			placeholder={props.placeholder}
			type={props.type}
			value={props.signal[0]()}
			onInput={ev => props.signal[1](ev.target.value)}
		/>
	);
}
