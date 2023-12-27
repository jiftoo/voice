import questionIcon from "../assets/help-circle.svg";
import "./InfoTooltip.css";

export default function InfoTooltip(props: {text: string}) {
	return (
		<div class="info-tooltip-container">
			<span class="info-tooltip-icon">
				<img src={questionIcon} />
			</span>
			<div class="info-tooltip-popup rounded">{props.text}</div>
		</div>
	);
}
