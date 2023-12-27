import {createEffect, createSignal} from "solid-js";
import clipboardSvg from "../assets/clipboard.svg";
import "./CopyableToken.css";

function CopyableToken(props: {children: string}) {
	const [clicked, setClicked] = createSignal(false);
	createEffect(() => {
		let timeoutHandle: number | undefined;
		if (clicked()) {
			navigator.clipboard.writeText(props.children);
			timeoutHandle = setTimeout(() => {
				setClicked(false);
			}, 150);
		}
		return () => {
			clearTimeout(timeoutHandle);
		};
	});
	return (
		<code
			class="copyable-token rounded"
			classList={{clicked: clicked()}}
			title="Click to copy"
			onMouseDown={() => {
				setClicked(true);
			}}
		>
			{props.children} <img src={clipboardSvg} />
		</code>
	);
}

export default CopyableToken;
