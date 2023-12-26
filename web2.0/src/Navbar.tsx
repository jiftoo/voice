import "./Navbar.css";
import Switch from "./components/Switch";
import {GLOBAL_STATE} from "./globalState";
import {useNavigate} from "@solidjs/router";

export default function Navbar() {
	const navigate = useNavigate();
	return (
		<nav class="rounded">
			<div id="logo" onClick={() => navigate("/")}>
				Voice
			</div>
			<Switch label="Premium mode" signal={GLOBAL_STATE.premium} />
		</nav>
	);
}
