import {useParams} from "@solidjs/router";
import "./Task.css";
import clipboardSvg from "../assets/clipboard.svg";
import {createEffect, createSignal} from "solid-js";

export default function Task() {
	const {id: taskId} = useParams();

	const [clicked, setClicked] = createSignal(false);
	createEffect(() => {
		if (clicked()) {
			navigator.clipboard.writeText(taskId);
			setTimeout(() => {
				setClicked(false);
			}, 150);
		}
	});
	return (
		<>
			<h4>
				Task{" "}
				<code
					class="rounded"
					classList={{clicked: clicked()}}
					id="task-id-code"
					onMouseDown={() => {
						setClicked(true);
					}}
				>
					{taskId} <img src={clipboardSvg} />
				</code>
			</h4>
		</>
	);
}
