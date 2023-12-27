import {JSX} from "solid-js";
import "./Button.css";

type ButtonVariant = "normal" | "accent" | "danger";

export default function Button(props: {
	children: JSX.Element;
	onClick?: () => void;
	variant?: ButtonVariant;
	disabled?: boolean;
	small?: boolean;
}) {
	let variant = () => props.variant ?? "normal";
	return (
		<button
			class="custom-button"
			classList={{[variant()!]: true, disabled: props.disabled, small: props.small}}
			disabled={props.disabled}
			onClick={() => {
				if (props.onClick) {
					props.onClick();
				}
			}}
		>
			{props.children}
		</button>
	);
}
