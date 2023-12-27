import {JSX} from "solid-js/jsx-runtime";
import "./ProgressBar.css";

export default function ProgressBar(props: {value: number; style?: JSX.CSSProperties; class?: string}) {
	return (
		<div
			class={"progress-bar rounded" + (props.class ? " " + props.class : "")}
			style={{...(props.style ?? {}), "--progress": Math.max(0, Math.min(1, props.value))}}
		/>
	);
}
