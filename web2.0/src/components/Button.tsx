import "./Button.css";

type ButtonVariant = "normal" | "accent" | "danger";

export default function Button(props: {children: string; onClick?: () => void; variant?: ButtonVariant; disabled?: boolean}) {
	let variant = () => props.variant ?? "normal";
	return (
		<button
			class="custom-button"
			classList={{[variant()!]: true, disabled: props.disabled}}
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
